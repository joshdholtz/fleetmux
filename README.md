# FleetMux

FleetMux is a Rust TUI for monitoring up to 10 remote tmux panes across multiple machines, with one-key interactive control via SSH.

## Features

- Read-only dashboard powered by `tmux capture-pane` polling
- Multi-target SSH host resolution with failover and caching
- Deterministic host color system
- Guided setup wizard with pane discovery
- One-key “take control” using `ssh -t` + tmux attach

## Requirements

- Rust (stable)
- `ssh` client
- `tmux` installed on each remote host
- SSH keys or agent configured for non-interactive access

## Install and Run

```sh
cargo build --release
./target/release/fleetmux
```

On first run, FleetMux launches a setup wizard and writes a config to:

```
~/.config/fleetmux/config.toml
```

## Configuration

See `config.example.toml` for a complete example. The most common fields:

- `ui.refresh_ms`: polling interval in milliseconds
- `ui.lines`: number of captured lines per pane
- `ssh.connect_timeout_sec`: SSH connection timeout
- `ssh.path_extra`: extra PATH entries for locating tmux on remote hosts
- `hosts`: logical hosts with one or more SSH targets
- `tracked`: tmux panes to monitor

## Keyboard Shortcuts

- `h` `j` `k` `l` / arrows: move focus
- `Tab`: next tile
- `Enter`: take control of focused pane
- `r`: reload config
- `e`: edit config
- `z`: zoom focused tile
- `?`: toggle help
- `q`: quit

## Notes

- Dashboard mode is strictly read-only. No interactive SSH sessions are held while polling.
- “Take control” runs a full `ssh -t` session and returns to the dashboard after exit.
