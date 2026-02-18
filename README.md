# FleetMux

FleetMux is a Rust TUI that turns your scattered tmux panes into a single mission‑control dashboard.
Think: distributed tmux, read‑only by default, with one‑key jump‑in when you need full control.

```
┌──────────────────────────── FleetMux Dashboard ────────────────────────────┐
│  local:dev ─ pane 1                buildbox:ci ─ pane 7                     │
│  ┌──────────────────────────┐      ┌──────────────────────────┐             │
│  │  Status: OK              │      │  Status: OK              │             │
│  │  make test               │      │  cargo build             │             │
│  │  ...                     │      │  ...                     │             │
│  └──────────────────────────┘      └──────────────────────────┘             │
│                                                                            │
│  staging:api ─ pane 3             prod:jobs ─ pane 9                        │
│  ┌──────────────────────────┐      ┌──────────────────────────┐             │
│  │  Status: OK              │      │  Status: STALE           │             │
│  │  node server.js          │      │  ...                     │             │
│  └──────────────────────────┘      └──────────────────────────┘             │
└────────────────────────────────────────────────────────────────────────────┘
```

## Why this exists

- You have multiple machines running multiple tmux sessions.
- You want a clean, unified overview that updates fast.
- You still want to jump into any pane instantly.

FleetMux gives you the “brrrr dashboard” view without holding interactive SSH sessions.

## What you get

- Read‑only tiled dashboard of up to 10 panes
- Multi‑target SSH resolution with failover + caching
- Deterministic host colors so machines stay recognizable
- Startup wizard + structured selection (host → session → window → pane)
- One‑key “take control” that drops you into the real tmux pane
- ANSI rendering so output looks like the actual pane
- Activity indicator + “last change” time so you can spot live panes fast
- Local tmux support alongside remote hosts
- Pinned bookmark strip for quick jump‑in panes

## Quick start

```sh
cargo build --release
./target/release/fleetmux
```

First run opens the in‑app setup UI and saves:

```
~/.config/fleetmux/config.toml
```

## Setup flow

FleetMux uses a full‑screen in‑app setup UI for both host management and pane selection.
On first run it opens automatically; later you can press `s` from the dashboard to reopen it.

Selection is a tree:

```
Host → Session → Window → Pane
```

Previously selected panes are preselected if they still exist. If `local.enabled = true`, the
local tmux server shows up as an extra host named by `local.name`.

While selecting panes, press `m` to toggle a bookmark (shown in the bottom strip of the dashboard).

## Diagnostics

```sh
fleetmux doctor
```

Prints host resolution, tmux version, windows/panes, and a sample capture.

## Configuration

See `config.example.toml` for a full example. Common fields:

- `ui.refresh_ms`: polling interval (ms)
- `ui.lines`: lines captured per pane
- `ui.compact`: hide metadata rows to show more output
- `ui.ansi`: render ANSI colors/styles
- `ui.join_lines`: join wrapped lines (tmux `-J`)
- `ui.bell_on_stop`: ring a terminal bell when a pane stops changing
- `ui.macos_notification_on_stop`: macOS notification when a pane stops changing
- `ssh.connect_timeout_sec`: SSH connection timeout
- `ssh.path_extra`: extra PATH entries for tmux on remote hosts
- `local.enabled`: include local tmux in discovery/selection
- `local.name`: display name for the local host
- `hosts`: logical hosts + SSH targets
- `tracked`: optional, updated on each selection
- `bookmarks`: optional quick‑jump panes (not rendered in the main tiles)

### Host colors

```toml
[[hosts]]
name = "buildbox"
targets = ["buildbox.local"]
color = "LightBlue"
```

Supported colors: `Black`, `Red`, `Green`, `Yellow`, `Blue`, `Magenta`, `Cyan`, `Gray`,
`DarkGray`, `LightRed`, `LightGreen`, `LightYellow`, `LightBlue`, `LightMagenta`, `LightCyan`, `White`.

### Pane labels

While running, press `n` to set a label for the focused pane. Labels are saved immediately and
persist across restarts.

## Keyboard shortcuts

- `h` `j` `k` `l` / arrows: move focus
- `Tab`: next tile
- `Enter`: take control of focused pane
- `r`: reload config
- `e`: edit config
- `n`: set label for focused pane
- `b`: toggle bookmark for focused pane
- `1-9`/`0`: jump to bookmarks 1–10
- `s`: open setup
- `c`: toggle compact mode
- `z`: zoom focused tile
- `?`: toggle help
- `q`: quit

## Notes

- Dashboard mode is strictly read‑only; no interactive SSH sessions are held.
- “Take control” runs `ssh -t` and returns to FleetMux on exit.
- If tmux isn’t on PATH for non‑interactive shells, set `ssh.path_extra`.
