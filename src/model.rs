use std::time::Duration;

pub const MAX_CLASSIC_DATA_LEN: usize = 8;
pub const MAX_FD_DATA_LEN: usize = 64;

#[derive(Clone, Debug)]
pub struct CanRecord {
    pub timestamp: Duration,
    pub iface: String,
    pub can_id: u32,
    pub data: Vec<u8>,
    pub is_extended: bool,
    pub is_remote: bool,
    pub is_fd: bool,
    pub fd_flags: u8,
    pub line_number: usize,
    pub raw_line: String,
}

impl CanRecord {
    pub fn id_string(&self) -> String {
        if self.is_extended {
            format!("{:08X}", self.can_id)
        } else {
            format!("{:03X}", self.can_id)
        }
    }

    pub fn payload_string(&self) -> String {
        if self.is_remote {
            return String::from("R");
        }

        let mut out = String::with_capacity(self.data.len() * 2);
        for byte in &self.data {
            out.push(hex_char(byte >> 4));
            out.push(hex_char(byte & 0x0F));
        }
        out
    }

    pub fn wire_len(&self) -> usize {
        self.data.len()
    }
}

#[derive(Debug)]
pub struct Timeline {
    pub frames: Vec<CanRecord>,
    pub duration: Duration,
}

impl Timeline {
    pub fn from_frames(mut frames: Vec<CanRecord>) -> Self {
        normalize_from_first_timestamp(&mut frames);
        let duration = frames
            .last()
            .map(|frame| frame.timestamp)
            .unwrap_or(Duration::ZERO);

        Self { frames, duration }
    }

    pub fn index_for_time(&self, target: Duration) -> usize {
        self.frames
            .partition_point(|frame| frame.timestamp < target)
    }

    pub fn timestamp_for_index(&self, index: usize) -> Duration {
        self.frames
            .get(index)
            .map(|frame| frame.timestamp)
            .unwrap_or(self.duration)
    }

    pub fn apply_timing_options(
        &mut self,
        ignore_timestamps: bool,
        min_gap: Option<Duration>,
        skip_gap: Option<Duration>,
    ) {
        if self.frames.is_empty() {
            self.duration = Duration::ZERO;
            return;
        }

        let mut previous_original = self.frames[0].timestamp;
        let mut rewritten = Duration::ZERO;
        self.frames[0].timestamp = Duration::ZERO;

        for frame in self.frames.iter_mut().skip(1) {
            let original = frame.timestamp;
            let mut delta = if ignore_timestamps {
                Duration::ZERO
            } else {
                original.saturating_sub(previous_original)
            };

            if let Some(skip_gap) = skip_gap
                && delta > skip_gap
            {
                delta = Duration::ZERO;
            }

            if let Some(min_gap) = min_gap
                && delta < min_gap
            {
                delta = min_gap;
            }

            rewritten = rewritten.saturating_add(delta);
            frame.timestamp = rewritten;
            previous_original = original;
        }

        self.duration = rewritten;
    }
}

fn normalize_from_first_timestamp(frames: &mut [CanRecord]) {
    if let Some(first) = frames.first().map(|frame| frame.timestamp) {
        for frame in frames {
            frame.timestamp = frame.timestamp.saturating_sub(first);
        }
    }
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'A' + (value - 10)) as char,
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{CanRecord, Timeline};

    fn sample_timeline() -> Timeline {
        Timeline::from_frames(vec![
            CanRecord {
                timestamp: Duration::from_millis(100),
                iface: String::from("can0"),
                can_id: 0x123,
                data: vec![1],
                is_extended: false,
                is_remote: false,
                is_fd: false,
                fd_flags: 0,
                line_number: 1,
                raw_line: String::from("(0.1) can0 123#01"),
            },
            CanRecord {
                timestamp: Duration::from_millis(300),
                iface: String::from("can0"),
                can_id: 0x124,
                data: vec![2],
                is_extended: false,
                is_remote: false,
                is_fd: false,
                fd_flags: 0,
                line_number: 2,
                raw_line: String::from("(0.3) can0 124#02"),
            },
            CanRecord {
                timestamp: Duration::from_millis(5300),
                iface: String::from("can0"),
                can_id: 0x125,
                data: vec![3],
                is_extended: false,
                is_remote: false,
                is_fd: false,
                fd_flags: 0,
                line_number: 3,
                raw_line: String::from("(5.3) can0 125#03"),
            },
        ])
    }

    #[test]
    fn applies_skip_gap_and_min_gap() {
        let mut timeline = sample_timeline();
        timeline.apply_timing_options(
            false,
            Some(Duration::from_millis(50)),
            Some(Duration::from_secs(2)),
        );

        assert_eq!(timeline.frames[0].timestamp, Duration::ZERO);
        assert_eq!(timeline.frames[1].timestamp, Duration::from_millis(200));
        assert_eq!(timeline.frames[2].timestamp, Duration::from_millis(250));
        assert_eq!(timeline.duration, Duration::from_millis(250));
    }

    #[test]
    fn ignores_timestamps_when_requested() {
        let mut timeline = sample_timeline();
        timeline.apply_timing_options(true, Some(Duration::from_millis(10)), None);

        assert_eq!(timeline.frames[0].timestamp, Duration::ZERO);
        assert_eq!(timeline.frames[1].timestamp, Duration::from_millis(10));
        assert_eq!(timeline.frames[2].timestamp, Duration::from_millis(20));
    }
}
