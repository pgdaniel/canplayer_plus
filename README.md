# canplayer_plus

**Work In Progress - Not Tested**

`canplayer_plus` is a seekable CAN log replayer written in Rust. It keeps the log indexed in memory and exposes a small HTTP control API so timeline scrubbing is a first-class operation instead of an afterthought.

It also tracks a practical subset of the existing `canplayer` CLI so you do not have to relearn the basics just to get scrubbing and a control UI.

## Why this shape

Classic `canplayer` is good at replaying a trace from start to finish with original timing. Scrubbing changes the problem:

- the player needs random access to a timestamped frame timeline
- control commands need to interrupt playback cleanly
- a separate control plane is easier than trying to multiplex replay and seek commands through one blocking loop

This implementation solves that by using:

- a parser for `candump`/`canplayer`-style logs
- a playback worker thread that replays from a logical cursor
- a tiny HTTP server for `play`, `pause`, `seek`, `step`, `speed`, `status`, and `quit`
- a built-in web UI with a slider for scrubbing the replay timeline
- a separate terminal scrubber sample built with `ratatui`

## CLI parity

Supported `canplayer`-style switches:

- `-I <file>` for logfile input, with stdin still supported by default
- `-l <num>` and `-l i` for finite or infinite replay loops
- `-t` to ignore timestamps
- `-i` for interactive ENTER-to-process-next-frame mode
- `-n <count>` to stop after a fixed number of transmitted frames
- `-g <ms>` to enforce a minimum inter-frame gap
- `-s <sec>` to collapse large timestamp gaps
- `-x` to disable local loopback on transmit sockets
- `-v` for verbose transmit logging
- interface assignments such as `vcan2=can0` and `stdout=can1`

This is intended as practical operator-facing parity, not a byte-for-byte clone of upstream `canplayer` behavior in every edge case.

Added switches remain available for the extra functionality:

- `--bind`
- `--no-server`
- `--dry-run`
- `--autoplay`
- `--speed`
- `--start-ms`
- `--iface`

## Supported log format

Examples:

```text
(1712599552.000000) can0 123#DEADBEEF
(1712599552.100000) can0 1ABCDEFF#R
(1712599552.250000) can1 456##1AABBCCDD
```

- `123#...` is classic CAN
- `#R` is a remote frame
- `##` is CAN FD, where the first nibble after `##` is the FD flag nibble

## Build

```bash
cargo build --release
```

You can also run the binaries directly during development with `cargo run`.

## Testing

Run the test suite to verify functionality:

```bash
cargo test
```

This includes unit tests for all modules, integration tests for end-to-end flows, and edge case tests.

For performance benchmarks (parsing and playback):

```bash
cargo bench
```

For code coverage (requires `cargo-tarpaulin` installed):

```bash
cargo tarpaulin --ignore-tests
```

## Usage

Start the player in dry-run mode with the control server enabled:

```bash
cargo run -- -I examples/simple_can.log --dry-run
```

See `examples/README.md` for additional sample logs and usage scenarios.

Then either open the web UI:

```text
http://127.0.0.1:4011/
```

Or launch the terminal scrubber in another terminal:

```bash
cargo run --bin canplayer_tui --
```

Replay onto a real interface:

```bash
cargo run -- -I examples/canfd.log --iface vcan0 --autoplay
```

Run without the server and just play through once:

```bash
cargo run -- -I capture.log --dry-run --autoplay --no-server
```

Use canplayer-style remapping:

```bash
cargo run -- -I capture.log vcan2=can0
```

Use interactive stepping:

```bash
cargo run -- -I capture.log -i --no-server stdout=can0
```

## Control API

The server binds to `127.0.0.1:4011` by default.

```bash
curl http://127.0.0.1:4011/status
curl -X POST http://127.0.0.1:4011/play
curl -X POST http://127.0.0.1:4011/pause
curl -X POST 'http://127.0.0.1:4011/seek?ms=2500'
curl -X POST 'http://127.0.0.1:4011/step?count=1'
curl -X POST 'http://127.0.0.1:4011/speed?value=0.5'
curl -X POST http://127.0.0.1:4011/quit
```

## Web UI

The built-in page is intentionally simple and self-contained:

- timeline slider for scrubbing by millisecond
- play/pause buttons
- single-step backward and forward
- speed control
- live status polling with next-frame preview

## Terminal Scrubber

The sample terminal app is a modern TUI built with `ratatui` and `crossterm`, intended as the terminal equivalent of the web scrubber. It talks to the same HTTP control API as the browser UI.

Start the player:

```bash
cargo run -- -I examples/sample.log --dry-run
```

In another terminal:

```bash
cargo run --bin canplayer_tui --
```

To point it at a different player instance:

```bash
cargo run --bin canplayer_tui -- --server 127.0.0.1:4011
```

Useful keys:

- `Space` play or pause
- `Left` and `Right` seek by 100 ms
- `Ctrl+Left` and `Ctrl+Right` seek by 1000 ms
- `[` and `]` step backward or forward by one frame
- `-` and `+` change speed
- `g` and `G` jump to start or end
- `x` asks the server to quit
- `q` quits only the TUI

## Notes

- If `--iface` is omitted, the original interface names from the log are preserved.
- `--dry-run` is the safe way to validate seek and timing behavior before transmitting on a live bus.
- CAN FD transmit support is enabled on the socket up front so mixed classic/FD traces can replay through one process.
- The browser UI and terminal scrubber are optional control clients layered on top of the same replay server.
