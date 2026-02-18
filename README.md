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

## Diagnostics

Run a quick connectivity and tmux discovery check without starting the TUI:

```sh
fleetmux doctor
```

On first run, FleetMux launches a setup wizard to save your hosts and writes a config to:

```
~/.config/fleetmux/config.toml
```

## Configuration

See `config.example.toml` for a complete example. The most common fields:

- `ui.refresh_ms`: polling interval in milliseconds
- `ui.lines`: number of captured lines per pane
- `ui.compact`: show more pane output by hiding metadata rows
- `ui.ansi`: render ANSI colors/styles from tmux output
- `ui.join_lines`: join wrapped lines (tmux `-J`)
- `ssh.connect_timeout_sec`: SSH connection timeout
- `ssh.path_extra`: extra PATH entries for locating tmux on remote hosts
- `hosts`: logical hosts with one or more SSH targets
- `tracked`: optional, FleetMux will prompt for windows on each start

To change host colors, set `color` in the host entry:

```toml
[[hosts]]
name = "buildbox"
targets = ["buildbox.local"]
color = "LightBlue"
```

Supported color names: `Black`, `Red`, `Green`, `Yellow`, `Blue`, `Magenta`, `Cyan`, `Gray`, `DarkGray`, `LightRed`, `LightGreen`, `LightYellow`, `LightBlue`, `LightMagenta`, `LightCyan`, `White`.

## Startup Selection

FleetMux prompts you on every startup to select which session windows to monitor. It then tracks all panes in the chosen windows (up to 10 total panes).

## Keyboard Shortcuts

- `h` `j` `k` `l` / arrows: move focus
- `Tab`: next tile
- `Enter`: take control of focused pane
- `r`: reload config
- `e`: edit config
- `c`: toggle compact mode
- `z`: zoom focused tile
- `?`: toggle help
- `q`: quit

## Notes

- Dashboard mode is strictly read-only. No interactive SSH sessions are held while polling.
- “Take control” runs a full `ssh -t` session and returns to the dashboard after exit.
