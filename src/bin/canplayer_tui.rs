use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Wrap};
use serde::Deserialize;

const DEFAULT_SERVER: &str = "127.0.0.1:4011";
const POLL_INTERVAL: Duration = Duration::from_millis(150);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(350);

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
    let config = Config::parse()?;
    let mut app = App::new(ControlClient::new(config.server));
    app.refresh_status()?;

    enable_raw_mode().map_err(|error| format!("failed to enable raw mode: {error}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|error| format!("failed to enter alternate screen: {error}"))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|error| format!("failed to create terminal: {error}"))?;

    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode().map_err(|error| format!("failed to disable raw mode: {error}"))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|error| format!("failed to leave alternate screen: {error}"))?;
    terminal
        .show_cursor()
        .map_err(|error| format!("failed to restore terminal cursor: {error}"))?;

    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), String> {
    let mut last_poll = Instant::now();

    loop {
        terminal
            .draw(|frame| app.render(frame))
            .map_err(|error| format!("failed to draw TUI: {error}"))?;

        let timeout = POLL_INTERVAL.saturating_sub(last_poll.elapsed());
        if event::poll(timeout).map_err(|error| format!("event poll failed: {error}"))? {
            let event = event::read().map_err(|error| format!("event read failed: {error}"))?;
            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
                && !app.handle_key(key)?
            {
                return Ok(());
            }
        }

        if last_poll.elapsed() >= POLL_INTERVAL {
            app.refresh_status()?;
            last_poll = Instant::now();
        }
    }
}

struct Config {
    server: String,
}

impl Config {
    fn parse() -> Result<Self, String> {
        let mut server = DEFAULT_SERVER.to_string();
        let mut iter = std::env::args().skip(1);

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    print!("{}", Self::help_text());
                    std::process::exit(0);
                }
                "--server" => {
                    server = iter
                        .next()
                        .ok_or_else(|| String::from("--server requires host:port"))?;
                }
                other => {
                    return Err(format!(
                        "unknown argument: {other}\n\n{}",
                        Self::help_text()
                    ));
                }
            }
        }

        Ok(Self { server })
    }

    fn help_text() -> &'static str {
        "\
canplayer_tui

Usage:
  cargo run --bin canplayer_tui -- [options]

Options:
      --server HOST:PORT    canplayer_plus control server (default: 127.0.0.1:4011)
  -h, --help                show this help text

Keybindings:
  Space        play/pause
  Left/Right   seek by 100 ms
  Ctrl+Arrows  seek by 1000 ms
  [ / ]        step backward / forward one frame
  - / +        decrease / increase speed
  g / G        seek to start / end
  r            refresh status
  x            ask server to quit
  q            quit the TUI
"
    }
}

struct App {
    client: ControlClient,
    status: Option<Status>,
    error: Option<String>,
}

impl App {
    fn new(client: ControlClient) -> Self {
        Self {
            client,
            status: None,
            error: None,
        }
    }

    fn refresh_status(&mut self) -> Result<(), String> {
        match self.client.status() {
            Ok(status) => {
                self.status = Some(status);
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Result<bool, String> {
        match key.code {
            KeyCode::Char('q') => return Ok(false),
            KeyCode::Char('r') => {
                let _ = self.refresh_status();
            }
            KeyCode::Char('x') => {
                let _ = self.client.post_no_status("/quit");
                return Ok(false);
            }
            KeyCode::Char(' ') => {
                if self
                    .status
                    .as_ref()
                    .map(|status| status.playing)
                    .unwrap_or(false)
                {
                    self.apply_status(self.client.command("/pause")?);
                } else {
                    self.apply_status(self.client.command("/play")?);
                }
            }
            KeyCode::Left => {
                let step = if key.modifiers.contains(KeyModifiers::CONTROL) {
                    -1000
                } else {
                    -100
                };
                self.seek_relative(step)?;
            }
            KeyCode::Right => {
                let step = if key.modifiers.contains(KeyModifiers::CONTROL) {
                    1000
                } else {
                    100
                };
                self.seek_relative(step)?;
            }
            KeyCode::Char('[') => {
                self.apply_status(self.client.command("/step?count=-1")?);
            }
            KeyCode::Char(']') => {
                self.apply_status(self.client.command("/step?count=1")?);
            }
            KeyCode::Char('-') => self.adjust_speed(-0.1)?,
            KeyCode::Char('+') | KeyCode::Char('=') => self.adjust_speed(0.1)?,
            KeyCode::Char('g') => {
                self.apply_status(self.client.command("/seek?ms=0")?);
            }
            KeyCode::Char('G') => {
                let duration = self
                    .status
                    .as_ref()
                    .map(|status| status.duration_ms)
                    .unwrap_or(0);
                self.apply_status(self.client.command(&format!("/seek?ms={duration}"))?);
            }
            _ => {}
        }

        Ok(true)
    }

    fn apply_status(&mut self, status: Status) {
        self.status = Some(status);
        self.error = None;
    }

    fn seek_relative(&mut self, delta_ms: i64) -> Result<(), String> {
        let status = self
            .status
            .as_ref()
            .ok_or_else(|| String::from("no status available"))?;
        let next = clamp_add(status.current_ms, delta_ms, status.duration_ms);
        self.apply_status(self.client.command(&format!("/seek?ms={next}"))?);
        Ok(())
    }

    fn adjust_speed(&mut self, delta: f64) -> Result<(), String> {
        let current = self
            .status
            .as_ref()
            .map(|status| status.speed)
            .unwrap_or(1.0);
        let next = (current + delta).max(0.1);
        self.apply_status(self.client.command(&format!("/speed?value={next:.2}"))?);
        Ok(())
    }

    fn render(&self, frame: &mut ratatui::Frame<'_>) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Min(8),
                Constraint::Length(3),
            ])
            .split(frame.area());

        let status = self.status.as_ref();
        let title = if let Some(status) = status {
            if status.playing {
                "canplayer_plus TUI  PLAYING"
            } else {
                "canplayer_plus TUI  PAUSED"
            }
        } else {
            "canplayer_plus TUI  DISCONNECTED"
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(
                self.client.addr.as_str(),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Session"));
        frame.render_widget(header, layout[0]);

        let ratio = status
            .map(|status| ratio(status.current_ms, status.duration_ms))
            .unwrap_or(0.0);
        let gauge_label = if let Some(status) = status {
            format!(
                "{} / {} ms   speed {:.2}x   frame {}/{}",
                status.current_ms,
                status.duration_ms,
                status.speed,
                status.cursor_index,
                status.total_frames
            )
        } else {
            String::from("waiting for status")
        };

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Scrubber"))
            .gauge_style(
                Style::default()
                    .fg(Color::LightYellow)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .label(gauge_label)
            .ratio(ratio);
        frame.render_widget(gauge, layout[1]);

        let middle = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(layout[2]);

        let detail_text = detail_lines(status);
        let details = Paragraph::new(detail_text)
            .block(Block::default().borders(Borders::ALL).title("Playback"))
            .wrap(Wrap { trim: true });
        frame.render_widget(details, middle[0]);

        let controls = Paragraph::new(controls_lines())
            .block(Block::default().borders(Borders::ALL).title("Keys"))
            .wrap(Wrap { trim: true });
        frame.render_widget(controls, middle[1]);

        let footer_message = if let Some(error) = &self.error {
            Line::from(vec![
                Span::styled("error: ", Style::default().fg(Color::Red)),
                Span::raw(error),
            ])
        } else if let Some(status) = status {
            let next = status
                .next_frame
                .as_ref()
                .map(|frame| {
                    format!(
                        "next {} on {} ({} bytes)",
                        frame.can_id, frame.iface, frame.len
                    )
                })
                .unwrap_or_else(|| String::from("no next frame"));
            Line::from(vec![
                Span::styled("info: ", Style::default().fg(Color::Green)),
                Span::raw(next),
            ])
        } else {
            Line::from("info: waiting for server")
        };

        let footer = Paragraph::new(footer_message)
            .block(Block::default().borders(Borders::ALL).title("Status"));
        frame.render_widget(footer, layout[3]);
    }
}

fn detail_lines(status: Option<&Status>) -> Vec<Line<'static>> {
    let Some(status) = status else {
        return vec![Line::from("No status available yet.")];
    };

    let mut lines = vec![
        kv_line("playing", if status.playing { "true" } else { "false" }),
        kv_line("loop", if status.r#loop { "true" } else { "false" }),
        kv_line("last_error", status.last_error.as_deref().unwrap_or("none")),
    ];

    if let Some(frame) = &status.next_frame {
        let mut next = String::new();
        let _ = write!(
            &mut next,
            "{} on {} len={} mode={}",
            frame.can_id,
            frame.iface,
            frame.len,
            if frame.fd { "fd" } else { "classic" }
        );
        lines.push(kv_line("next_frame", &next));
    } else {
        lines.push(kv_line("next_frame", "none"));
    }

    lines
}

fn controls_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("Space     play/pause"),
        Line::from("Left      seek -100 ms"),
        Line::from("Right     seek +100 ms"),
        Line::from("Ctrl+<-   seek -1000 ms"),
        Line::from("Ctrl+->   seek +1000 ms"),
        Line::from("[ / ]     step backward/forward"),
        Line::from("- / +     adjust speed"),
        Line::from("g / G     start / end"),
        Line::from("r         refresh"),
        Line::from("x         server quit"),
        Line::from("q         leave TUI"),
    ]
}

fn kv_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:>10}: "), Style::default().fg(Color::Cyan)),
        Span::raw(value.to_string()),
    ])
}

fn ratio(current_ms: u64, duration_ms: u64) -> f64 {
    if duration_ms == 0 {
        0.0
    } else {
        (current_ms as f64 / duration_ms as f64).clamp(0.0, 1.0)
    }
}

fn clamp_add(current: u64, delta: i64, max: u64) -> u64 {
    let candidate = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as u64)
    };
    candidate.min(max)
}

struct ControlClient {
    addr: String,
}

impl ControlClient {
    fn new(addr: String) -> Self {
        Self { addr }
    }

    fn status(&self) -> Result<Status, String> {
        let response = self.request("GET", "/status")?;
        parse_json_response::<Status>(&response)
    }

    fn command(&self, path: &str) -> Result<Status, String> {
        let response = self.request("POST", path)?;
        parse_json_response::<Status>(&response)
    }

    fn post_no_status(&self, path: &str) -> Result<(), String> {
        let response = self.request("POST", path)?;
        let (status_code, _) = split_http_response(&response)?;
        if status_code != 200 {
            return Err(format!("request failed with status {status_code}"));
        }
        Ok(())
    }

    fn request(&self, method: &str, path: &str) -> Result<String, String> {
        let mut stream =
            TcpStream::connect(&self.addr).map_err(|error| format!("connect failed: {error}"))?;
        stream
            .set_read_timeout(Some(CONNECT_TIMEOUT))
            .map_err(|error| format!("set_read_timeout failed: {error}"))?;
        stream
            .set_write_timeout(Some(CONNECT_TIMEOUT))
            .map_err(|error| format!("set_write_timeout failed: {error}"))?;

        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            self.addr
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("request write failed: {error}"))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|error| format!("response read failed: {error}"))?;
        Ok(response)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Status {
    playing: bool,
    current_ms: u64,
    duration_ms: u64,
    cursor_index: usize,
    total_frames: usize,
    speed: f64,
    #[serde(rename = "loop")]
    r#loop: bool,
    last_error: Option<String>,
    next_frame: Option<NextFrame>,
}

#[derive(Clone, Debug, Deserialize)]
struct NextFrame {
    iface: String,
    can_id: String,
    len: usize,
    fd: bool,
}

fn parse_json_response<T>(response: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let (status_code, body) = split_http_response(response)?;
    if status_code != 200 {
        return Err(extract_error_message(status_code, body));
    }

    serde_json::from_str(body).map_err(|error| format!("invalid JSON response: {error}"))
}

fn split_http_response(response: &str) -> Result<(u16, &str), String> {
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| String::from("malformed HTTP response"))?;
    let status_line = head
        .lines()
        .next()
        .ok_or_else(|| String::from("missing HTTP status line"))?;
    let mut parts = status_line.split_whitespace();
    let _http = parts
        .next()
        .ok_or_else(|| String::from("missing HTTP version"))?;
    let status_code = parts
        .next()
        .ok_or_else(|| String::from("missing HTTP status code"))?
        .parse::<u16>()
        .map_err(|_| String::from("invalid HTTP status code"))?;
    Ok((status_code, body))
}

fn extract_error_message(status_code: u16, body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("request failed with status {status_code}"))
}

#[cfg(test)]
mod tests {
    use super::{clamp_add, ratio, split_http_response};

    #[test]
    fn clamp_add_respects_bounds() {
        assert_eq!(clamp_add(500, -600, 1_000), 0);
        assert_eq!(clamp_add(500, 600, 1_000), 1_000);
        assert_eq!(clamp_add(500, 125, 1_000), 625);
    }

    #[test]
    fn ratio_handles_zero_duration() {
        assert_eq!(ratio(0, 0), 0.0);
        assert_eq!(ratio(50, 100), 0.5);
    }

    #[test]
    fn split_http_response_parses_code_and_body() {
        let (status, body) = split_http_response("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
            .expect("response parses");
        assert_eq!(status, 200);
        assert_eq!(body, "{}");
    }
}
