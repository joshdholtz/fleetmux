# FleetMux — Contributor Guide

This repo is a Rust TUI for monitoring tmux panes across local + remote hosts.
Keep changes small and focused.

## Quick commands

```sh
cargo build
cargo run
fleetmux doctor
```

## Runtime behavior

- Startup selection is always per‑pane (host → session → window → pane).
- The selection is saved back into `~/.config/fleetmux/config.toml`.
- Dashboard mode is read‑only. “Take control” attaches to tmux.

## Key modules

- `src/main.rs`: app loop, event handling, take control, config reload
- `src/config.rs`: config schema + defaults
- `src/setup.rs`: full-screen setup UI (hosts + pane selection)
- `src/poller.rs`: async polling tasks
- `src/tmux.rs`: tmux list/capture helpers
- `src/ssh.rs`: SSH + local command execution, target resolution
- `src/ui/`: ratatui rendering

## Config paths

- Config: `~/.config/fleetmux/config.toml`
- Example: `config.example.toml`

## Design notes

- The dashboard is snapshot‑based via `tmux capture-pane`.
- ANSI rendering is enabled by default (`ui.ansi = true`).
- The local tmux server is included by default (`local.enabled = true`).
