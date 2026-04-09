use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::Duration;

use crate::model::{CanRecord, MAX_CLASSIC_DATA_LEN, MAX_FD_DATA_LEN, Timeline};

pub fn parse_log_input(path: Option<&Path>) -> Result<Timeline, String> {
    let contents = match path {
        Some(path) => fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?,
        None => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .map_err(|error| format!("failed to read stdin: {error}"))?;
            input
        }
    };

    parse_log_contents(&contents)
}

pub fn parse_log_contents(contents: &str) -> Result<Timeline, String> {
    let mut frames = Vec::new();

    for (line_index, raw_line) in contents.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let frame = parse_line(line, line_number, raw_line)?;
        frames.push(frame);
    }

    Ok(Timeline::from_frames(frames))
}

fn parse_line(line: &str, line_number: usize, raw_line: &str) -> Result<CanRecord, String> {
    let mut parts = line.split_whitespace();
    let timestamp_token = parts
        .next()
        .ok_or_else(|| format!("line {line_number}: missing timestamp"))?;
    let iface = parts
        .next()
        .ok_or_else(|| format!("line {line_number}: missing interface"))?;
    let frame_token = parts
        .next()
        .ok_or_else(|| format!("line {line_number}: missing CAN frame"))?;

    let timestamp = parse_timestamp(timestamp_token, line_number)?;
    let (can_id, is_extended, is_remote, is_fd, fd_flags, data) =
        parse_frame_token(frame_token, line_number)?;

    Ok(CanRecord {
        timestamp,
        iface: iface.to_string(),
        can_id,
        data,
        is_extended,
        is_remote,
        is_fd,
        fd_flags,
        line_number,
        raw_line: raw_line.trim_end().to_string(),
    })
}

fn parse_timestamp(token: &str, line_number: usize) -> Result<Duration, String> {
    let trimmed = token.trim_matches(|ch| ch == '(' || ch == ')');
    let value = trimmed
        .parse::<f64>()
        .map_err(|_| format!("line {line_number}: invalid timestamp {token}"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("line {line_number}: invalid timestamp {token}"));
    }
    Ok(Duration::from_secs_f64(value))
}

fn parse_frame_token(
    token: &str,
    line_number: usize,
) -> Result<(u32, bool, bool, bool, u8, Vec<u8>), String> {
    if let Some((id_text, payload_text)) = token.split_once("##") {
        let can_id = parse_can_id(id_text, line_number)?;
        let is_extended = is_extended_id(id_text, can_id);
        let mut chars = payload_text.chars();
        let flags_char = chars
            .next()
            .ok_or_else(|| format!("line {line_number}: missing CAN FD flags in {token}"))?;
        let fd_flags = parse_hex_nibble(flags_char)
            .ok_or_else(|| format!("line {line_number}: invalid CAN FD flags in {token}"))?;
        let data_text: String = chars.collect();
        let data = parse_hex_bytes(&data_text, MAX_FD_DATA_LEN, line_number)?;
        return Ok((can_id, is_extended, false, true, fd_flags, data));
    }

    if let Some((id_text, payload_text)) = token.split_once('#') {
        let can_id = parse_can_id(id_text, line_number)?;
        let is_extended = is_extended_id(id_text, can_id);
        if payload_text.starts_with('R') || payload_text.starts_with('r') {
            return Ok((can_id, is_extended, true, false, 0, Vec::new()));
        }

        let data = parse_hex_bytes(payload_text, MAX_CLASSIC_DATA_LEN, line_number)?;
        return Ok((can_id, is_extended, false, false, 0, data));
    }

    Err(format!(
        "line {line_number}: unsupported frame token {token}"
    ))
}

fn parse_can_id(id_text: &str, line_number: usize) -> Result<u32, String> {
    let can_id = u32::from_str_radix(id_text, 16)
        .map_err(|_| format!("line {line_number}: invalid CAN id {id_text}"))?;
    if can_id > 0x1FFF_FFFF {
        return Err(format!("line {line_number}: CAN id out of range {id_text}"));
    }
    Ok(can_id)
}

fn is_extended_id(id_text: &str, can_id: u32) -> bool {
    id_text.len() > 3 || can_id > 0x7FF
}

fn parse_hex_bytes(payload: &str, max_bytes: usize, line_number: usize) -> Result<Vec<u8>, String> {
    if payload.len() % 2 != 0 {
        return Err(format!(
            "line {line_number}: payload must contain full bytes"
        ));
    }

    let byte_count = payload.len() / 2;
    if byte_count > max_bytes {
        return Err(format!(
            "line {line_number}: payload exceeds {max_bytes} bytes"
        ));
    }

    let mut data = Vec::with_capacity(byte_count);
    let bytes = payload.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let high = parse_hex_nibble(bytes[index] as char)
            .ok_or_else(|| format!("line {line_number}: invalid hex payload"))?;
        let low = parse_hex_nibble(bytes[index + 1] as char)
            .ok_or_else(|| format!("line {line_number}: invalid hex payload"))?;
        data.push((high << 4) | low);
        index += 2;
    }
    Ok(data)
}

fn parse_hex_nibble(value: char) -> Option<u8> {
    match value {
        '0'..='9' => Some((value as u8) - b'0'),
        'a'..='f' => Some((value as u8) - b'a' + 10),
        'A'..='F' => Some((value as u8) - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_log_contents;

    #[test]
    fn parses_classic_and_fd_frames() {
        let timeline = parse_log_contents(
            "\
(10.100000) can0 123#DEADBEEF
(10.200000) can0 1ABCDEFF#R
(10.400000) can1 456##1AABBCCDD
",
        )
        .expect("parse succeeds");

        assert_eq!(timeline.frames.len(), 3);
        assert_eq!(timeline.frames[0].timestamp.as_millis(), 0);
        assert_eq!(timeline.frames[1].timestamp.as_millis(), 100);
        assert_eq!(timeline.frames[2].timestamp.as_millis(), 300);
        assert_eq!(timeline.frames[0].data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(timeline.frames[1].is_remote);
        assert!(timeline.frames[2].is_fd);
        assert_eq!(timeline.frames[2].fd_flags, 0x1);
        assert_eq!(timeline.frames[0].raw_line, "(10.100000) can0 123#DEADBEEF");
    }

    #[test]
    fn rejects_odd_payload_length() {
        let error = parse_log_contents("(1.0) can0 123#ABC").expect_err("parse fails");
        assert!(error.contains("payload must contain full bytes"));
    }
}
