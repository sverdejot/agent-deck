# Agent Deck

A SenseCAP Indicator D1 that shows the Claude Code sessions running in your Mac's terminal and
brings the one you tap to the front. The side button cycles through plan limits and token usage.

```
firmware/         ESP32-S3 firmware (Rust, esp-idf)
crates/deck-ui/   screens, layout and protocol (Rust, runs and tests on the host)
host/             Mac-side agent (Go)
docs/protocol.md  serial protocol
```

## Usage

Requires [mise](https://mise.jdx.dev) and rustup.

```sh
mise install && mise run setup   # tools + Xtensa toolchain
mise run fw:flash                # flash the deck
mise run host:install            # install the Mac agent (LaunchAgent + statusline)
mise run check                   # what CI runs
```
