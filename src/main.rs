mod args;
mod model;
mod parser;
mod player;
mod server;
mod socketcan;
mod web_ui;

use std::io::{self, BufRead};
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use args::Args;
use parser::parse_log_input;
use player::{LoopMode, Player};
use server::run_server;
use socketcan::{DryRunSink, FrameSink, RouteConfig, SocketCanSink};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse()?;
    if args.help {
        print!("{}", Args::help_text());
        return Ok(());
    }

    if args.no_server && !args.autoplay && !args.interactive {
        return Err(String::from(
            "--no-server requires --autoplay or -i/--interactive so playback can progress",
        ));
    }

    let mut timeline = parse_log_input(args.input.as_deref())?;
    timeline.apply_timing_options(
        args.ignore_timestamps || args.interactive,
        args.min_gap_ms.map(Duration::from_millis),
        args.skip_gap_s.map(Duration::from_secs),
    );
    if timeline.frames.is_empty() {
        return Err(String::from(
            "input log did not contain any playable CAN frames",
        ));
    }

    let routes = RouteConfig::new(args.iface.clone(), &args.assignments)?;
    let timeline = Arc::new(timeline);
    let sink: Box<dyn FrameSink> = if args.dry_run {
        Box::new(DryRunSink::new(routes, args.verbose > 0))
    } else {
        Box::new(SocketCanSink::new(
            routes,
            timeline.as_ref(),
            args.disable_loopback,
            args.verbose > 0,
        )?)
    };

    let loop_mode = if args.infinite_loop {
        LoopMode::Infinite
    } else {
        LoopMode::Finite(args.loop_count)
    };
    let mut player = Player::new(
        Arc::clone(&timeline),
        sink,
        args.speed,
        loop_mode,
        args.autoplay,
        Duration::from_millis(args.start_ms),
        args.frame_limit,
    )?;
    let handle = player.handle();

    print_startup_banner(&args, timeline.as_ref());

    if args.interactive {
        spawn_interactive_stepper(handle.clone());
    }

    if args.no_server {
        wait_for_local_completion(&handle);
    } else {
        run_server(&args.bind, handle)?;
    }

    player.shutdown_and_join();
    Ok(())
}

fn spawn_interactive_stepper(handle: player::PlayerHandle) {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut locked = stdin.lock();
        let mut line = String::new();
        while !handle.is_shutdown() {
            line.clear();
            match locked.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => handle.process_steps(1),
                Err(_) => break,
            }
        }
    });
}

fn wait_for_local_completion(handle: &player::PlayerHandle) {
    while !handle.is_finished() && !handle.is_shutdown() {
        thread::sleep(Duration::from_millis(50));
    }
}

fn print_startup_banner(args: &Args, timeline: &model::Timeline) {
    println!(
        "loaded {} frames spanning {} ms from {}",
        timeline.frames.len(),
        timeline.duration.as_millis(),
        args.input_label()
    );

    if args.dry_run {
        println!("output: dry-run");
    }

    if let Some(iface) = &args.iface {
        println!("output interface override: {iface}");
    } else if !args.assignments.is_empty() {
        println!("output assignments:");
        for assignment in &args.assignments {
            println!("  {} <= {}", assignment.output, assignment.input);
        }
    } else if !args.dry_run {
        println!("output interface mode: preserve log interface names");
    }

    println!(
        "startup: {} at {}x, start={} ms, repeat={}, interactive={}, ignore_timestamps={}",
        if args.autoplay { "autoplay" } else { "paused" },
        args.speed,
        args.start_ms,
        if args.infinite_loop {
            String::from("infinite")
        } else {
            args.loop_count.to_string()
        },
        args.interactive,
        args.ignore_timestamps || args.interactive
    );

    if let Some(limit) = args.frame_limit {
        println!("frame limit: {limit}");
    }
    if let Some(gap) = args.min_gap_ms {
        println!("minimum gap: {gap} ms");
    }
    if let Some(skip) = args.skip_gap_s {
        println!("skip gaps larger than: {skip} s");
    }
    if args.disable_loopback {
        println!("socket loopback: disabled");
    }

    if args.interactive {
        println!("interactive mode: press ENTER to process the next frame");
    }

    if !args.no_server {
        println!("control server: http://{}", args.bind);
        println!("web ui:  http://{}/", args.bind);
        println!("status: curl http://{}/status", args.bind);
        println!("play:   curl -X POST http://{}/play", args.bind);
        println!("seek:   curl -X POST 'http://{}/seek?ms=2500'", args.bind);
        println!("pause:  curl -X POST http://{}/pause", args.bind);
        println!("step:   curl -X POST 'http://{}/step?count=1'", args.bind);
        println!(
            "speed:  curl -X POST 'http://{}/speed?value=0.5'",
            args.bind
        );
        println!("quit:   curl -X POST http://{}/quit", args.bind);
    }
}
