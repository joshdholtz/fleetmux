# FleetMux — Distributed tmux “Brrrr” Dashboard (Rust TUI)

## Vision

FleetMux is a terminal-native Rust TUI that provides a unified, read-only dashboard of up to 10 tmux panes across multiple machines, with one-key full interactive control of any pane.

It must:

- Show up to 10 remote tmux panes in a unified grid
- Be strictly read-only in dashboard mode
- Allow instant full interactive control
- Support multiple SSH connection targets per logical host
- Support host-level color coding
- Include a guided setup wizard

It should feel like a distributed tmux mission control panel.

---

## Core Modes

### 1. Dashboard Mode (Default)

- Displays up to 10 tracked panes in a grid layout
- Each tile shows:
  - Logical host name (color-coded)
  - session:window.pane_id
  - Current command or pane title
  - Last N lines of output
  - Connection status
- Strictly read-only
- Uses polling via SSH + tmux capture-pane
- Default refresh interval: 750ms (configurable)

Polling command:

ssh <target> "tmux capture-pane -p -J -t <pane_id> -S -<LINES>"

No interactive SSH sessions are held during dashboard mode.

---

### 2. Take Control Mode

When Enter is pressed on a focused tile:

- Suspend the TUI
- Launch:

ssh -t <resolved_target> \
  "tmux attach -t <session> \; \
   select-window -t <session>:<window> \; \
   select-pane -t <pane_id>"

- Return to dashboard when SSH exits

---

## Host Model

Each logical host has:

- name
- one or more SSH targets
- optional color
- optional tags

Example:

```toml
[[hosts]]
name = "buildbox"
targets = ["buildbox.local", "100.64.12.34"]
strategy = "auto"
color = "Blue"
tags = ["dev"]
```

Target Resolution:

1. Try targets in order
2. Test with: ssh <target> "tmux -V"
3. First successful target wins
4. Cache active target for 60 seconds
5. Retry on failure
6. If all fail → host marked DOWN

---

## Color System

- Each logical host has a base color
- If not specified, assign deterministically via hash(host.name)
- Default palette:

["Blue", "Cyan", "Green", "Magenta", "Yellow", "LightBlue", "LightGreen"]

Tile rules:

- Border color = host base color
- Host name = base color + bold
- Focused tile = brighter/bold version
- DOWN status = DarkRed override
- STALE = dimmed base color
- Activity highlight when content changes

Do NOT color entire pane body.

---

## Configuration

Location:

~/.config/fleetmux/config.toml

Example:

```toml
[ui]
refresh_ms = 750
lines = 40
layout = "auto"
theme = "default"

[colors]
default_host_palette = [
  "Blue", "Cyan", "Green",
  "Magenta", "Yellow",
  "LightBlue", "LightGreen"
]

[ssh]
connect_timeout_sec = 2
control_master = true
control_persist_sec = 600

[[hosts]]
name = "buildbox"
targets = ["buildbox.local"]
strategy = "auto"
color = "Blue"

[[tracked]]
host = "buildbox"
session = "main"
window = 0
pane_id = "%3"
label = "Build Logs"
```

---

## Setup Wizard

On first run:

1. Add Logical Host
   - Name
   - Multiple SSH targets
   - Optional color
   - Test connectivity

2. Discover tmux Panes:

tmux list-panes -a -F \
"#{session_name}\t#{window_index}\t#{pane_id}\t#{pane_current_command}\t#{pane_title}"

User selects up to 10 panes total.
User assigns optional labels.

3. Confirm and save config.

---

## TUI Design

Layout:

- Adaptive tiled grid
- Host-colored borders
- Status indicators
- Activity highlighting

Keyboard:

- hjkl / arrows → move focus
- Enter → take control
- Tab → next tile
- e → edit config
- r → reload config
- ? → help
- q → quit
- z → zoom tile (optional)

---

## Technical Architecture

Stack:

- Rust stable
- ratatui
- crossterm
- tokio
- serde + toml
- anyhow or thiserror

Concurrency:

- One async task per tracked pane
- Each task:
  - Resolve host target
  - Poll capture-pane
  - Send snapshot via channel

UI thread:
- Render state
- Track activity
- Apply color styling

---

## Acceptance Criteria

- Supports multiple logical hosts
- Supports multiple SSH targets per host
- Shows up to 10 panes unified
- Strictly read-only dashboard
- Enter attaches interactively
- Setup wizard works
- Deterministic host colors
- Works on macOS and Linux

---

## Deliverable

cargo new fleetmux

Project includes:

- main.rs
- config.rs
- ssh.rs
- tmux.rs
- model.rs
- ui/
- README.md
- example config

Production-quality code only.
No stubs.
