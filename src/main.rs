mod config;
mod model;
mod poller;
mod ssh;
mod tmux;
mod ui;
mod wizard;

use anyhow::{anyhow, Context, Result};
use config::Config;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures_util::StreamExt;
use model::{AppState, HostColors};
use poller::PollerHandle;
use ratatui::style::Color;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = config::config_path()?;
    let mut config = ensure_hosts(&config_path)?;
    config.tracked = wizard::select_windows(&config)?;

    let host_colors = build_host_colors(&config);
    let mut state = AppState::new(config.clone(), host_colors);

    let resolver = Arc::new(Mutex::new(ssh::HostResolver::new()));
    let (update_tx, mut update_rx) = mpsc::channel(100);
    let mut pollers = poller::start_pollers(&config, Arc::clone(&resolver), update_tx.clone());

    let mut terminal = ui::enter_terminal()?;
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(200));

    loop {
        terminal.draw(|f| ui::dashboard::draw(f, &state))?;

        tokio::select! {
            maybe_update = update_rx.recv() => {
                if let Some(update) = maybe_update {
                    state.apply_update(update);
                }
            }
            maybe_event = events.next() => {
                if let Some(Ok(event)) = maybe_event {
                    if handle_event(
                        event,
                        &mut state,
                        &config_path,
                        &mut terminal,
                        &resolver,
                        &mut pollers,
                        &update_tx,
                        &mut config,
                    ).await? {
                        break;
                    }
                }
            }
            _ = tick.tick() => {
                state.refresh_stale();
            }
        }
    }

    ui::exit_terminal(&mut terminal)?;
    Ok(())
}

fn ensure_hosts(path: &Path) -> Result<Config> {
    if path.exists() {
        let config = config::load(path)?;
        if config.hosts.is_empty() {
            return wizard::run_host_setup(path);
        }
        return Ok(config);
    }

    wizard::run_host_setup(path)
}

async fn handle_event(
    event: Event,
    state: &mut AppState,
    config_path: &Path,
    terminal: &mut ui::AppTerminal,
    resolver: &Arc<Mutex<ssh::HostResolver>>,
    pollers: &mut PollerHandle,
    update_tx: &mpsc::Sender<model::PaneUpdate>,
    config: &mut Config,
) -> Result<bool> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('?') => state.show_help = !state.show_help,
            KeyCode::Char('z') => state.zoomed = !state.zoomed,
            KeyCode::Char('r') => {
                reload_config(
                    config_path,
                    terminal,
                    resolver,
                    pollers,
                    update_tx,
                    state,
                    config,
                )
                .await?;
            }
            KeyCode::Char('e') => {
                edit_config(config_path, terminal)?;
                reload_config(
                    config_path,
                    terminal,
                    resolver,
                    pollers,
                    update_tx,
                    state,
                    config,
                )
                .await?;
            }
            KeyCode::Enter => {
                take_control(state, resolver, terminal).await?;
            }
            KeyCode::Tab => move_focus(state, FocusMove::Next),
            KeyCode::Left | KeyCode::Char('h') => move_focus(state, FocusMove::Left),
            KeyCode::Right | KeyCode::Char('l') => move_focus(state, FocusMove::Right),
            KeyCode::Up | KeyCode::Char('k') => move_focus(state, FocusMove::Up),
            KeyCode::Down | KeyCode::Char('j') => move_focus(state, FocusMove::Down),
            _ => {}
        },
        _ => {}
    }
    Ok(false)
}

fn move_focus(state: &mut AppState, direction: FocusMove) {
    let count = state.panes.len();
    if count == 0 {
        return;
    }
    if matches!(direction, FocusMove::Next) {
        state.focused = (state.focused + 1) % count;
        return;
    }

    let (rows, cols) = grid_dimensions(count);
    let row = state.focused / cols;
    let col = state.focused % cols;

    let (new_row, new_col) = match direction {
        FocusMove::Left => (row, col.saturating_sub(1)),
        FocusMove::Right => (row, (col + 1).min(cols.saturating_sub(1))),
        FocusMove::Up => (row.saturating_sub(1), col),
        FocusMove::Down => ((row + 1).min(rows.saturating_sub(1)), col),
        FocusMove::Next => (row, col),
    };

    let mut index = new_row * cols + new_col;
    if index >= count {
        index = count - 1;
    }
    state.focused = index;
}

fn grid_dimensions(count: usize) -> (usize, usize) {
    let cols = (count as f64).sqrt().ceil() as usize;
    let rows = (count + cols - 1) / cols;
    (rows, cols)
}

async fn reload_config(
    config_path: &Path,
    terminal: &mut ui::AppTerminal,
    resolver: &Arc<Mutex<ssh::HostResolver>>,
    pollers: &mut PollerHandle,
    update_tx: &mpsc::Sender<model::PaneUpdate>,
    state: &mut AppState,
    config: &mut Config,
) -> Result<()> {
    let new_config = config::load(config_path)
        .with_context(|| format!("Failed to reload {}", config_path.display()))?;
    if new_config.hosts.is_empty() {
        return Err(anyhow!("Config missing hosts"));
    }

    let tracked = select_windows_with_terminal(&new_config, terminal)?;

    pollers.stop().await;
    let mut new_config = new_config.clone();
    new_config.tracked = tracked;
    *config = new_config.clone();
    let host_colors = build_host_colors(&new_config);
    *state = AppState::new(new_config.clone(), host_colors);
    *pollers = poller::start_pollers(&new_config, Arc::clone(resolver), update_tx.clone());
    Ok(())
}

fn edit_config(path: &Path, terminal: &mut ui::AppTerminal) -> Result<()> {
    ui::exit_terminal(terminal)?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(editor)
        .arg(path)
        .status()
        .context("Failed to launch editor")?;
    if !status.success() {
        eprintln!("Editor exited with status {status}");
    }

    *terminal = ui::enter_terminal()?;
    Ok(())
}

fn select_windows_with_terminal(
    config: &Config,
    terminal: &mut ui::AppTerminal,
) -> Result<Vec<config::TrackedPane>> {
    ui::exit_terminal(terminal)?;
    let result = wizard::select_windows(config);
    *terminal = ui::enter_terminal()?;
    result
}

async fn take_control(
    state: &AppState,
    resolver: &Arc<Mutex<ssh::HostResolver>>,
    terminal: &mut ui::AppTerminal,
) -> Result<()> {
    let pane = state
        .panes
        .get(state.focused)
        .ok_or_else(|| anyhow!("No focused pane"))?;
    let host_cfg = state
        .config
        .hosts
        .iter()
        .find(|host| host.name == pane.tracked.host)
        .ok_or_else(|| anyhow!("Unknown host: {}", pane.tracked.host))?;

    let target = {
        let mut resolver = resolver.lock().await;
        resolver.resolve_target(host_cfg, &state.config.ssh).await
    }?;

    let remote_cmd = format!(
        "tmux attach -t {session} \\; select-window -t {session}:{window} \\; select-pane -t {pane_id}",
        session = pane.tracked.session,
        window = pane.tracked.window,
        pane_id = pane.tracked.pane_id
    );
    let remote_cmd = ssh::wrap_remote_cmd(&state.config.ssh, &remote_cmd);

    ui::exit_terminal(terminal)?;

    let mut cmd = tokio::process::Command::new("ssh");
    cmd.arg("-t");
    for arg in ssh::build_ssh_args(&state.config.ssh) {
        cmd.arg(arg);
    }
    cmd.arg(&target);
    cmd.arg(remote_cmd);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let status = cmd.status().await.context("Failed to launch ssh")?;
    if !status.success() {
        eprintln!("ssh exited with status {status}");
    }

    *terminal = ui::enter_terminal()?;
    Ok(())
}

fn build_host_colors(config: &Config) -> HashMap<String, HostColors> {
    let mut map = HashMap::new();
    for host in &config.hosts {
        let color_name = host
            .color
            .clone()
            .unwrap_or_else(|| deterministic_color_name(&host.name, &config.colors.default_host_palette));
        let base_color = parse_color(&color_name).unwrap_or(Color::Blue);
        let focus = focus_color(base_color);
        map.insert(
            host.name.clone(),
            HostColors {
                base: base_color,
                focus,
            },
        );
    }
    map
}

fn deterministic_color_name(host: &str, palette: &[String]) -> String {
    if palette.is_empty() {
        return "Blue".to_string();
    }
    let hash = fnv1a(host.as_bytes());
    let index = (hash % palette.len() as u64) as usize;
    palette[index].clone()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

fn parse_color(name: &str) -> Option<Color> {
    match name.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => None,
    }
}

fn focus_color(color: Color) -> Color {
    match color {
        Color::Blue => Color::LightBlue,
        Color::Green => Color::LightGreen,
        Color::Yellow => Color::LightYellow,
        Color::Magenta => Color::LightMagenta,
        Color::Cyan => Color::LightCyan,
        Color::Red => Color::LightRed,
        Color::Gray | Color::DarkGray => Color::White,
        _ => color,
    }
}

enum FocusMove {
    Left,
    Right,
    Up,
    Down,
    Next,
}
