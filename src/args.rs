use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct InterfaceAssignment {
    pub output: String,
    pub input: String,
}

#[derive(Debug)]
pub struct Args {
    pub input: Option<PathBuf>,
    pub iface: Option<String>,
    pub assignments: Vec<InterfaceAssignment>,
    pub bind: String,
    pub no_server: bool,
    pub dry_run: bool,
    pub autoplay: bool,
    pub interactive: bool,
    pub ignore_timestamps: bool,
    pub speed: f64,
    pub start_ms: u64,
    pub loop_count: u64,
    pub infinite_loop: bool,
    pub frame_limit: Option<usize>,
    pub min_gap_ms: Option<u64>,
    pub skip_gap_s: Option<u64>,
    pub disable_loopback: bool,
    pub verbose: u8,
    pub help: bool,
}

impl Args {
    pub fn parse() -> Result<Self, String> {
        Self::parse_from_iter(env::args().skip(1))
    }

    pub fn parse_from_iter<I, S>(iterable: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut input = None;
        let mut iface = None;
        let mut assignments = Vec::new();
        let mut bind = String::from("127.0.0.1:4011");
        let mut no_server = false;
        let mut dry_run = false;
        let mut autoplay = false;
        let mut interactive = false;
        let mut ignore_timestamps = false;
        let mut speed = 1.0_f64;
        let mut start_ms = 0_u64;
        let mut loop_count = 1_u64;
        let mut infinite_loop = false;
        let mut frame_limit = None;
        let mut min_gap_ms = None;
        let mut skip_gap_s = None;
        let mut disable_loopback = false;
        let mut verbose = 0_u8;
        let mut help = false;

        let mut iter = iterable.into_iter().map(Into::into);
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" | "-?" => help = true,
                "-I" | "--input" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| String::from("-I/--input requires a path"))?;
                    input = Some(PathBuf::from(value));
                }
                "-i" | "--interactive" => interactive = true,
                "-t" | "--ignore-timestamps" => ignore_timestamps = true,
                "-x" | "--disable-loopback" => disable_loopback = true,
                "-v" | "--verbose" => verbose = verbose.saturating_add(1),
                "--iface" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| String::from("--iface requires an interface name"))?;
                    iface = Some(value);
                }
                "--bind" => {
                    bind = iter
                        .next()
                        .ok_or_else(|| String::from("--bind requires host:port"))?;
                }
                "--no-server" => no_server = true,
                "--dry-run" => dry_run = true,
                "--autoplay" => autoplay = true,
                "--loop" => infinite_loop = true,
                "--speed" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| String::from("--speed requires a numeric value"))?;
                    speed = parse_positive_f64(&value, "--speed")?;
                }
                "--start-ms" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| String::from("--start-ms requires an integer"))?;
                    start_ms = value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid --start-ms value: {value}"))?;
                }
                "-l" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| String::from("-l requires a loop count or 'i'"))?;
                    if value == "i" {
                        infinite_loop = true;
                    } else {
                        loop_count = value
                            .parse::<u64>()
                            .map_err(|_| format!("invalid -l value: {value}"))?;
                        if loop_count == 0 {
                            return Err(String::from("-l requires a positive integer or 'i'"));
                        }
                    }
                }
                "-n" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| String::from("-n requires a frame count"))?;
                    let parsed = value
                        .parse::<usize>()
                        .map_err(|_| format!("invalid -n value: {value}"))?;
                    if parsed == 0 {
                        return Err(String::from("-n requires a positive integer"));
                    }
                    frame_limit = Some(parsed);
                }
                "-g" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| String::from("-g requires a millisecond value"))?;
                    min_gap_ms = Some(
                        value
                            .parse::<u64>()
                            .map_err(|_| format!("invalid -g value: {value}"))?,
                    );
                }
                "-s" => {
                    let value = iter
                        .next()
                        .ok_or_else(|| String::from("-s requires a second value"))?;
                    let parsed = value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid -s value: {value}"))?;
                    if parsed == 0 {
                        return Err(String::from("-s requires an integer greater than zero"));
                    }
                    skip_gap_s = Some(parsed);
                }
                other if other.contains('=') => {
                    assignments.push(parse_assignment(other)?);
                }
                other => {
                    return Err(format!(
                        "unknown argument: {other}\n\n{}",
                        Self::help_text()
                    ));
                }
            }
        }

        if help {
            return Ok(Self {
                input,
                iface,
                assignments,
                bind,
                no_server,
                dry_run,
                autoplay,
                interactive,
                ignore_timestamps,
                speed,
                start_ms,
                loop_count,
                infinite_loop,
                frame_limit,
                min_gap_ms,
                skip_gap_s,
                disable_loopback,
                verbose,
                help,
            });
        }

        if iface.is_some() && !assignments.is_empty() {
            return Err(String::from(
                "--iface cannot be combined with canplayer-style interface assignments",
            ));
        }

        if input.is_none() && interactive {
            return Err(String::from(
                "-i/--interactive requires -I/--input because stdin would already be consumed by the logfile",
            ));
        }

        Ok(Self {
            input,
            iface,
            assignments,
            bind,
            no_server,
            dry_run,
            autoplay,
            interactive,
            ignore_timestamps,
            speed,
            start_ms,
            loop_count,
            infinite_loop,
            frame_limit,
            min_gap_ms,
            skip_gap_s,
            disable_loopback,
            verbose,
            help,
        })
    }

    pub fn help_text() -> &'static str {
        "\
canplayer_plus

Usage:
  canplayer_plus [options] [dst_if=src_if ...]

canplayer-compatible options:
  -I, --input PATH          input log file (default: stdin)
  -l NUM                    process the input NUM times
  -l i                      process the input in an infinite loop
  -t                        ignore timestamps and send immediately
  -i, --interactive         wait for ENTER to process the next frame
  -n COUNT                  terminate after COUNT transmitted frames
  -g MS                     enforce a minimum gap between frames
  -s SEC                    skip timestamp gaps larger than SEC
  -x                        disable local loopback on transmit sockets
  -v                        verbose transmit logging

Additions:
      --iface IFACE         override every output frame to one interface
      --bind HOST:PORT      HTTP control bind address (default: 127.0.0.1:4011)
      --no-server           disable HTTP control server
      --dry-run             print replayed frames instead of transmitting them
      --autoplay            start replay immediately instead of paused
      --speed FLOAT         playback rate multiplier (default: 1.0)
      --start-ms INT        initial seek position in milliseconds
      --loop                infinite looping alias
  -h, --help                show this help text

Interface assignments:
  vcan2=can0               replay log frames from can0 onto vcan2
  stdout=can0              print log frames from can0 to stdout

Web/UI control endpoints:
  GET  /
  GET  /status
  POST /play
  POST /pause
  POST /seek?ms=2500
  POST /step?count=1
  POST /speed?value=0.5
  POST /quit
"
    }

    pub fn input_label(&self) -> String {
        self.input
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| String::from("stdin"))
    }
}

fn parse_assignment(value: &str) -> Result<InterfaceAssignment, String> {
    let (output, input) = value
        .split_once('=')
        .ok_or_else(|| format!("invalid interface assignment: {value}"))?;
    if output.is_empty() || input.is_empty() {
        return Err(format!("invalid interface assignment: {value}"));
    }

    Ok(InterfaceAssignment {
        output: output.to_string(),
        input: input.to_string(),
    })
}

fn parse_positive_f64(value: &str, flag: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("invalid {flag} value: {value}"))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Err(format!("{flag} must be a finite value greater than 0"));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::Args;

    #[test]
    fn parses_canplayer_compatible_switches_and_assignments() {
        let args = Args::parse_from_iter([
            "-I",
            "trace.log",
            "-l",
            "3",
            "-n",
            "12",
            "-g",
            "25",
            "-s",
            "4",
            "-x",
            "-v",
            "vcan2=can0",
            "stdout=can1",
        ])
        .expect("args parse");

        assert_eq!(args.input_label(), "trace.log");
        assert_eq!(args.loop_count, 3);
        assert_eq!(args.frame_limit, Some(12));
        assert_eq!(args.min_gap_ms, Some(25));
        assert_eq!(args.skip_gap_s, Some(4));
        assert!(args.disable_loopback);
        assert_eq!(args.verbose, 1);
        assert_eq!(args.assignments.len(), 2);
        assert_eq!(args.assignments[0].output, "vcan2");
        assert_eq!(args.assignments[0].input, "can0");
        assert_eq!(args.assignments[1].output, "stdout");
        assert_eq!(args.assignments[1].input, "can1");
    }

    #[test]
    fn rejects_iface_with_assignments() {
        let error = Args::parse_from_iter(["-I", "trace.log", "--iface", "vcan0", "vcan2=can0"])
            .expect_err("rejects conflicting routing");
        assert!(error.contains("--iface cannot be combined"));
    }

    #[test]
    fn rejects_interactive_without_file_input() {
        let error = Args::parse_from_iter(["-i"]).expect_err("rejects stdin interactive mode");
        assert!(error.contains("-i/--interactive requires -I/--input"));
    }
}
