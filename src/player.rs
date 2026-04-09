use std::io;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::model::{CanRecord, Timeline};
use crate::socketcan::FrameSink;

pub struct Player {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct PlayerHandle {
    shared: Arc<Shared>,
}

pub enum LoopMode {
    Finite(u64),
    Infinite,
}

struct Shared {
    timeline: Arc<Timeline>,
    state: Mutex<PlayerState>,
    cv: Condvar,
}

struct PlayerState {
    cursor_index: usize,
    anchor_time: Duration,
    anchor_instant: Instant,
    playing: bool,
    pending_steps: usize,
    loop_mode: LoopMode,
    speed: f64,
    shutdown: bool,
    last_error: Option<String>,
    sent_frames: usize,
    frame_limit: Option<usize>,
}

#[derive(Clone)]
pub struct PlayerStatus {
    pub playing: bool,
    pub current_ms: u64,
    pub duration_ms: u64,
    pub cursor_index: usize,
    pub total_frames: usize,
    pub speed: f64,
    pub loop_playback: bool,
    pub last_error: Option<String>,
    pub next_frame: Option<FramePreview>,
}

#[derive(Clone)]
pub struct FramePreview {
    pub iface: String,
    pub can_id: String,
    pub len: usize,
    pub is_fd: bool,
}

impl Player {
    pub fn new(
        timeline: Arc<Timeline>,
        sink: Box<dyn FrameSink>,
        speed: f64,
        loop_mode: LoopMode,
        autoplay: bool,
        start_at: Duration,
        frame_limit: Option<usize>,
    ) -> Result<Self, String> {
        let bounded_start = start_at.min(timeline.duration);
        let cursor_index = timeline.index_for_time(bounded_start);
        let now = Instant::now();
        let state = PlayerState {
            cursor_index,
            anchor_time: bounded_start,
            anchor_instant: now,
            playing: autoplay,
            pending_steps: 0,
            loop_mode,
            speed,
            shutdown: false,
            last_error: None,
            sent_frames: 0,
            frame_limit,
        };

        let shared = Arc::new(Shared {
            timeline,
            state: Mutex::new(state),
            cv: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name(String::from("canplayer-plus-worker"))
            .spawn(move || playback_loop(worker_shared, sink))
            .map_err(|error| format!("failed to spawn playback thread: {error}"))?;

        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    pub fn handle(&self) -> PlayerHandle {
        PlayerHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn shutdown_and_join(&mut self) {
        self.handle().shutdown();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl PlayerHandle {
    pub fn play(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        if state.shutdown {
            return;
        }

        let current = current_time_locked(&self.shared.timeline, &state, Instant::now());
        let cursor_index = self.shared.timeline.index_for_time(current);
        state.cursor_index = cursor_index;
        state.anchor_time = current;
        state.anchor_instant = Instant::now();
        state.playing = true;
        self.shared.cv.notify_all();
    }

    pub fn pause(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        if state.playing {
            let now = Instant::now();
            let current = current_time_locked(&self.shared.timeline, &state, now);
            let cursor_index = self.shared.timeline.index_for_time(current);
            state.cursor_index = cursor_index;
            state.anchor_time = current;
            state.anchor_instant = now;
            state.playing = false;
            self.shared.cv.notify_all();
        }
    }

    pub fn seek(&self, target: Duration) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        if state.shutdown {
            return;
        }

        let bounded = target.min(self.shared.timeline.duration);
        let cursor_index = self.shared.timeline.index_for_time(bounded);
        state.cursor_index = cursor_index;
        state.anchor_time = bounded;
        state.anchor_instant = Instant::now();
        self.shared.cv.notify_all();
    }

    pub fn step(&self, count: isize) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        if state.shutdown {
            return;
        }

        let now = Instant::now();
        let current = current_time_locked(&self.shared.timeline, &state, now);
        let mut cursor_index = self.shared.timeline.index_for_time(current);
        if count.is_negative() {
            cursor_index = cursor_index.saturating_sub(count.unsigned_abs());
        } else {
            cursor_index = cursor_index.saturating_add(count as usize);
        }
        cursor_index = cursor_index.min(self.shared.timeline.frames.len());
        let anchor_time = self.shared.timeline.timestamp_for_index(cursor_index);
        state.cursor_index = cursor_index;
        state.anchor_time = anchor_time;
        state.anchor_instant = now;
        state.playing = false;
        self.shared.cv.notify_all();
    }

    pub fn process_steps(&self, count: usize) {
        if count == 0 {
            return;
        }

        let mut state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        if state.shutdown {
            return;
        }

        state.playing = false;
        state.pending_steps = state.pending_steps.saturating_add(count);
        self.shared.cv.notify_all();
    }

    pub fn set_speed(&self, speed: f64) -> Result<(), String> {
        if !speed.is_finite() || speed <= 0.0 {
            return Err(String::from("speed must be a finite value greater than 0"));
        }

        let mut state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        if state.shutdown {
            return Ok(());
        }

        let now = Instant::now();
        let current = current_time_locked(&self.shared.timeline, &state, now);
        let cursor_index = self.shared.timeline.index_for_time(current);
        state.cursor_index = cursor_index;
        state.anchor_time = current;
        state.anchor_instant = now;
        state.speed = speed;
        self.shared.cv.notify_all();
        Ok(())
    }

    pub fn status(&self) -> PlayerStatus {
        let state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        let current = current_time_locked(&self.shared.timeline, &state, Instant::now());
        let next_frame = self
            .shared
            .timeline
            .frames
            .get(state.cursor_index)
            .map(preview_frame);

        PlayerStatus {
            playing: state.playing,
            current_ms: current.as_millis() as u64,
            duration_ms: self.shared.timeline.duration.as_millis() as u64,
            cursor_index: state.cursor_index,
            total_frames: self.shared.timeline.frames.len(),
            speed: state.speed,
            loop_playback: repeats_enabled(&state.loop_mode),
            last_error: state.last_error.clone(),
            next_frame,
        }
    }

    pub fn shutdown(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        state.shutdown = true;
        state.playing = false;
        self.shared.cv.notify_all();
    }

    pub fn is_finished(&self) -> bool {
        let state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        state.shutdown
            || (!state.playing && state.cursor_index >= self.shared.timeline.frames.len())
    }

    pub fn is_shutdown(&self) -> bool {
        let state = self
            .shared
            .state
            .lock()
            .expect("player state lock poisoned");
        state.shutdown
    }
}

fn preview_frame(frame: &CanRecord) -> FramePreview {
    FramePreview {
        iface: frame.iface.clone(),
        can_id: frame.id_string(),
        len: frame.wire_len(),
        is_fd: frame.is_fd,
    }
}

fn playback_loop(shared: Arc<Shared>, mut sink: Box<dyn FrameSink>) {
    let timeline = Arc::clone(&shared.timeline);
    let mut state = shared.state.lock().expect("player state lock poisoned");

    loop {
        while !state.shutdown && !state.playing && state.pending_steps == 0 {
            state = shared.cv.wait(state).expect("player state lock poisoned");
        }
        if state.shutdown {
            return;
        }

        if frame_limit_reached(&state) {
            finish_playback(&timeline, &mut state, true);
            shared.cv.notify_all();
            continue;
        }

        if state.cursor_index >= timeline.frames.len() {
            if should_repeat(&mut state.loop_mode) && !timeline.frames.is_empty() {
                state.cursor_index = 0;
                state.anchor_time = Duration::ZERO;
                state.anchor_instant = Instant::now();
                continue;
            }

            finish_playback(&timeline, &mut state, false);
            shared.cv.notify_all();
            continue;
        }

        let frame = &timeline.frames[state.cursor_index];
        let step_mode = state.pending_steps > 0;
        let current = current_time_locked(&timeline, &state, Instant::now());
        if !step_mode && frame.timestamp > current {
            let until_send = logical_delta_to_wall(frame.timestamp - current, state.speed);
            let (next_state, _) = shared
                .cv
                .wait_timeout(state, until_send)
                .expect("player state lock poisoned");
            state = next_state;
            continue;
        }

        if let Err(error) = sink.send(frame) {
            register_send_error(&mut state, error);
            shared.cv.notify_all();
            continue;
        }

        state.sent_frames += 1;
        state.cursor_index += 1;
        if step_mode {
            state.pending_steps = state.pending_steps.saturating_sub(1);
            state.anchor_time = timeline.timestamp_for_index(state.cursor_index);
            state.anchor_instant = Instant::now();
        }

        if frame_limit_reached(&state) {
            finish_playback(&timeline, &mut state, true);
            shared.cv.notify_all();
            continue;
        }

        if state.cursor_index >= timeline.frames.len() {
            if should_repeat(&mut state.loop_mode) && !timeline.frames.is_empty() {
                state.cursor_index = 0;
                state.anchor_time = Duration::ZERO;
                state.anchor_instant = Instant::now();
            } else {
                finish_playback(&timeline, &mut state, false);
            }
        }
        shared.cv.notify_all();
    }
}

fn current_time_locked(timeline: &Timeline, state: &PlayerState, now: Instant) -> Duration {
    if !state.playing {
        return state.anchor_time.min(timeline.duration);
    }

    let elapsed = now.saturating_duration_since(state.anchor_instant);
    let advanced = wall_to_logical_delta(elapsed, state.speed);
    let current = state.anchor_time.saturating_add(advanced);
    current.min(timeline.duration)
}

fn finish_playback(timeline: &Timeline, state: &mut PlayerState, shutdown: bool) {
    state.playing = false;
    state.anchor_time = timeline.duration;
    state.cursor_index = timeline.frames.len();
    if shutdown {
        state.shutdown = true;
    }
}

fn frame_limit_reached(state: &PlayerState) -> bool {
    state
        .frame_limit
        .map(|limit| state.sent_frames >= limit)
        .unwrap_or(false)
}

fn should_repeat(loop_mode: &mut LoopMode) -> bool {
    match loop_mode {
        LoopMode::Infinite => true,
        LoopMode::Finite(remaining) if *remaining > 1 => {
            *remaining -= 1;
            true
        }
        LoopMode::Finite(_) => false,
    }
}

fn repeats_enabled(loop_mode: &LoopMode) -> bool {
    match loop_mode {
        LoopMode::Infinite => true,
        LoopMode::Finite(remaining) => *remaining > 1,
    }
}

fn logical_delta_to_wall(delta: Duration, speed: f64) -> Duration {
    Duration::from_secs_f64(delta.as_secs_f64() / speed)
}

fn wall_to_logical_delta(delta: Duration, speed: f64) -> Duration {
    Duration::from_secs_f64(delta.as_secs_f64() * speed)
}

fn register_send_error(state: &mut PlayerState, error: io::Error) {
    state.playing = false;
    state.last_error = Some(error.to_string());
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::model::{CanRecord, Timeline};
    use crate::socketcan::FrameSink;

    use super::{LoopMode, Player};

    #[derive(Clone)]
    struct RecordingSink {
        sent: Arc<Mutex<Vec<u32>>>,
    }

    impl RecordingSink {
        fn new(sent: Arc<Mutex<Vec<u32>>>) -> Self {
            Self { sent }
        }
    }

    impl FrameSink for RecordingSink {
        fn send(&mut self, frame: &CanRecord) -> std::io::Result<()> {
            self.sent.lock().expect("sent lock").push(frame.can_id);
            Ok(())
        }
    }

    fn frame(timestamp_ms: u64, can_id: u32) -> CanRecord {
        CanRecord {
            timestamp: Duration::from_millis(timestamp_ms),
            iface: "can0".into(),
            can_id,
            data: vec![0x01],
            is_extended: false,
            is_remote: false,
            is_fd: false,
            fd_flags: 0,
            line_number: can_id as usize,
            raw_line: format!("({}) can0 {:03X}#01", timestamp_ms, can_id),
        }
    }

    fn sample_timeline() -> Arc<Timeline> {
        Arc::new(Timeline::from_frames(vec![
            frame(0, 0x101),
            frame(1, 0x102),
            frame(2, 0x103),
        ]))
    }

    fn wait_until<F>(timeout: Duration, predicate: F)
    where
        F: Fn() -> bool,
    {
        let start = Instant::now();
        while !predicate() {
            assert!(
                start.elapsed() < timeout,
                "condition not met before timeout"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn seek_and_speed_update_status_without_playing() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let timeline = sample_timeline();
        let mut player = Player::new(
            timeline,
            Box::new(RecordingSink::new(Arc::clone(&sent))),
            1.0,
            LoopMode::Finite(1),
            false,
            Duration::ZERO,
            None,
        )
        .expect("player");
        let handle = player.handle();

        handle.seek(Duration::from_millis(1));
        handle.set_speed(2.5).expect("speed update");
        let status = handle.status();

        assert_eq!(status.current_ms, 1);
        assert_eq!(status.cursor_index, 1);
        assert_eq!(status.speed, 2.5);
        assert_eq!(sent.lock().expect("sent lock").len(), 0);

        player.shutdown_and_join();
    }

    #[test]
    fn process_steps_sends_exact_number_of_frames() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let timeline = sample_timeline();
        let mut player = Player::new(
            timeline,
            Box::new(RecordingSink::new(Arc::clone(&sent))),
            1.0,
            LoopMode::Finite(1),
            false,
            Duration::ZERO,
            None,
        )
        .expect("player");
        let handle = player.handle();

        handle.process_steps(2);
        wait_until(Duration::from_millis(250), || {
            sent.lock().expect("sent lock").len() == 2
        });
        let status = handle.status();

        assert_eq!(*sent.lock().expect("sent lock"), vec![0x101, 0x102]);
        assert_eq!(status.cursor_index, 2);
        assert!(!status.playing);

        player.shutdown_and_join();
    }

    #[test]
    fn frame_limit_stops_replay_and_shuts_down_worker() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let timeline = sample_timeline();
        let mut player = Player::new(
            timeline,
            Box::new(RecordingSink::new(Arc::clone(&sent))),
            1.0,
            LoopMode::Infinite,
            false,
            Duration::ZERO,
            Some(4),
        )
        .expect("player");
        let handle = player.handle();

        handle.process_steps(10);
        wait_until(Duration::from_millis(250), || handle.is_shutdown());

        assert_eq!(sent.lock().expect("sent lock").len(), 4);
        assert!(handle.is_finished());

        player.shutdown_and_join();
    }

    #[test]
    fn finite_loop_repeats_expected_number_of_times() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let timeline = sample_timeline();
        let mut player = Player::new(
            timeline,
            Box::new(RecordingSink::new(Arc::clone(&sent))),
            1.0,
            LoopMode::Finite(2),
            false,
            Duration::ZERO,
            Some(6),
        )
        .expect("player");
        let handle = player.handle();

        handle.process_steps(6);
        wait_until(Duration::from_millis(250), || handle.is_shutdown());

        assert_eq!(
            *sent.lock().expect("sent lock"),
            vec![0x101, 0x102, 0x103, 0x101, 0x102, 0x103]
        );

        player.shutdown_and_join();
    }
}
