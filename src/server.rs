use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use crate::player::{FramePreview, PlayerHandle, PlayerStatus};
use crate::web_ui;

pub fn run_server(bind: &str, player: PlayerHandle) -> Result<(), String> {
    let listener = TcpListener::bind(bind)
        .map_err(|error| format!("failed to bind control server on {bind}: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to set control server nonblocking mode: {error}"))?;

    loop {
        if player.is_shutdown() {
            return Ok(());
        }

        match listener.accept() {
            Ok((stream, _)) => {
                if let Err(error) = handle_connection(stream, &player) {
                    eprintln!("control request failed: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return Err(format!("control server accept failed: {error}"));
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, player: &PlayerHandle) -> Result<(), String> {
    let mut buffer = [0_u8; 4096];
    let read = stream
        .read(&mut buffer)
        .map_err(|error| format!("failed to read request: {error}"))?;
    if read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..read]);
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| String::from("malformed HTTP request"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| String::from("missing HTTP method"))?;
    let target = parts
        .next()
        .ok_or_else(|| String::from("missing HTTP target"))?;

    let response = handle_request(method, target, player);
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("failed to write response: {error}"))?;
    Ok(())
}

fn handle_request(method: &str, target: &str, player: &PlayerHandle) -> String {
    let (path, query) = split_target(target);
    match (method, path) {
        ("GET", "/") | ("GET", "/ui") => html_response(200, web_ui::html()),
        ("GET", "/status") => json_response(200, &status_json(&player.status())),
        ("POST", "/play") => {
            player.play();
            json_response(200, &status_json(&player.status()))
        }
        ("POST", "/pause") => {
            player.pause();
            json_response(200, &status_json(&player.status()))
        }
        ("POST", "/seek") => match required_query_value(&query, "ms") {
            Ok(ms) => match ms.parse::<u64>() {
                Ok(value) => {
                    player.seek(Duration::from_millis(value));
                    json_response(200, &status_json(&player.status()))
                }
                Err(_) => json_response(400, "{\"error\":\"invalid ms value\"}"),
            },
            Err(error) => json_response(400, &json_error(&error)),
        },
        ("POST", "/step") => match required_query_value(&query, "count") {
            Ok(count) => match count.parse::<isize>() {
                Ok(value) => {
                    player.step(value);
                    json_response(200, &status_json(&player.status()))
                }
                Err(_) => json_response(400, "{\"error\":\"invalid count value\"}"),
            },
            Err(error) => json_response(400, &json_error(&error)),
        },
        ("POST", "/speed") => match required_query_value(&query, "value") {
            Ok(speed) => match speed.parse::<f64>() {
                Ok(value) => match player.set_speed(value) {
                    Ok(()) => json_response(200, &status_json(&player.status())),
                    Err(error) => json_response(400, &json_error(&error)),
                },
                Err(_) => json_response(400, "{\"error\":\"invalid speed value\"}"),
            },
            Err(error) => json_response(400, &json_error(&error)),
        },
        ("POST", "/quit") => {
            player.shutdown();
            json_response(200, "{\"status\":\"shutting down\"}")
        }
        _ => json_response(404, "{\"error\":\"not found\"}"),
    }
}

fn split_target(target: &str) -> (&str, HashMap<String, String>) {
    if let Some((path, raw_query)) = target.split_once('?') {
        (path, parse_query(raw_query))
    } else {
        (target, HashMap::new())
    }
}

fn parse_query(raw_query: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pair in raw_query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        out.insert(key.to_string(), value.to_string());
    }
    out
}

fn required_query_value<'a>(
    query: &'a HashMap<String, String>,
    key: &str,
) -> Result<&'a str, String> {
    query
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing query parameter: {key}"))
}

fn json_response(status: u16, body: &str) -> String {
    response(status, "application/json; charset=utf-8", body)
}

fn html_response(status: u16, body: &str) -> String {
    response(status, "text/html; charset=utf-8", body)
}

fn response(status: u16, content_type: &str, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };

    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn status_json(status: &PlayerStatus) -> String {
    let next_frame = match &status.next_frame {
        Some(frame) => frame_json(frame),
        None => String::from("null"),
    };
    let last_error = match &status.last_error {
        Some(error) => format!("\"{}\"", json_escape(error)),
        None => String::from("null"),
    };

    format!(
        "{{\"playing\":{},\"current_ms\":{},\"duration_ms\":{},\"cursor_index\":{},\"total_frames\":{},\"speed\":{},\"loop\":{},\"last_error\":{},\"next_frame\":{}}}",
        status.playing,
        status.current_ms,
        status.duration_ms,
        status.cursor_index,
        status.total_frames,
        status.speed,
        status.loop_playback,
        last_error,
        next_frame
    )
}

fn frame_json(frame: &FramePreview) -> String {
    format!(
        "{{\"iface\":\"{}\",\"can_id\":\"{}\",\"len\":{},\"fd\":{}}}",
        json_escape(&frame.iface),
        json_escape(&frame.can_id),
        frame.len,
        frame.is_fd
    )
}

fn json_error(message: &str) -> String {
    format!("{{\"error\":\"{}\"}}", json_escape(message))
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::model::{CanRecord, Timeline};
    use crate::player::{LoopMode, Player};
    use crate::socketcan::FrameSink;

    use super::{handle_request, split_target};

    struct NullSink;

    impl FrameSink for NullSink {
        fn send(&mut self, _frame: &CanRecord) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn player_with_timeline() -> Player {
        let timeline = Arc::new(Timeline::from_frames(vec![
            CanRecord {
                timestamp: Duration::from_millis(0),
                iface: "can0".into(),
                can_id: 0x123,
                data: vec![1],
                is_extended: false,
                is_remote: false,
                is_fd: false,
                fd_flags: 0,
                line_number: 1,
                raw_line: "(0.0) can0 123#01".into(),
            },
            CanRecord {
                timestamp: Duration::from_millis(100),
                iface: "can0".into(),
                can_id: 0x124,
                data: vec![2],
                is_extended: false,
                is_remote: false,
                is_fd: false,
                fd_flags: 0,
                line_number: 2,
                raw_line: "(0.1) can0 124#02".into(),
            },
        ]));

        Player::new(
            timeline,
            Box::new(NullSink),
            1.0,
            LoopMode::Finite(1),
            false,
            Duration::ZERO,
            None,
        )
        .expect("player")
    }

    #[test]
    fn split_target_parses_query_params() {
        let (path, query) = split_target("/seek?ms=100&foo=bar");
        assert_eq!(path, "/seek");
        assert_eq!(query.get("ms"), Some(&"100".to_string()));
        assert_eq!(query.get("foo"), Some(&"bar".to_string()));
    }

    #[test]
    fn handle_request_serves_status_and_seek_validation() {
        let mut player = player_with_timeline();
        let handle = player.handle();

        let status = handle_request("GET", "/status", &handle);
        assert!(status.contains("\"current_ms\":0"));
        assert!(status.contains("\"cursor_index\":0"));

        let invalid = handle_request("POST", "/seek", &handle);
        assert!(invalid.starts_with("HTTP/1.1 400"));
        assert!(invalid.contains("missing query parameter: ms"));

        let valid = handle_request("POST", "/seek?ms=100", &handle);
        assert!(valid.starts_with("HTTP/1.1 200"));
        assert!(valid.contains("\"current_ms\":100"));

        player.shutdown_and_join();
    }

    #[test]
    fn handle_request_serves_ui_and_unknown_route() {
        let mut player = player_with_timeline();
        let handle = player.handle();

        let ui = handle_request("GET", "/", &handle);
        assert!(ui.starts_with("HTTP/1.1 200"));
        assert!(ui.contains("<!DOCTYPE html>"));

        let missing = handle_request("GET", "/missing", &handle);
        assert!(missing.starts_with("HTTP/1.1 404"));

        player.shutdown_and_join();
    }
}
