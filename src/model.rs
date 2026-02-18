use crate::config::{Config, TrackedPane};
use ratatui::style::Color;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct HostColors {
    pub base: Color,
    pub focus: Color,
}

pub fn default_host_colors() -> HostColors {
    HostColors {
        base: Color::Blue,
        focus: Color::LightBlue,
    }
}

#[derive(Clone, Debug)]
pub struct PaneCapture {
    pub command: String,
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneStatus {
    Ok,
    Down,
    Stale,
}

#[derive(Clone, Debug)]
pub struct PaneUpdate {
    pub index: usize,
    pub capture: Option<PaneCapture>,
    pub status: PaneStatus,
    pub error: Option<String>,
    pub at: Instant,
}

#[derive(Clone, Debug)]
pub struct PaneState {
    pub tracked: TrackedPane,
    pub status: PaneStatus,
    pub last_capture: Option<PaneCapture>,
    pub last_update: Option<Instant>,
    pub last_change: Option<Instant>,
    pub error: Option<String>,
    pub last_hash: Option<u64>,
}

impl PaneState {
    fn new(tracked: TrackedPane) -> Self {
        Self {
            tracked,
            status: PaneStatus::Stale,
            last_capture: None,
            last_update: None,
            last_change: None,
            error: None,
            last_hash: None,
        }
    }
}

#[derive(Debug)]
pub struct AppState {
    pub config: Config,
    pub panes: Vec<PaneState>,
    pub focused: usize,
    pub show_help: bool,
    pub zoomed: bool,
    pub host_colors: HashMap<String, HostColors>,
}

impl AppState {
    pub fn new(config: Config, host_colors: HashMap<String, HostColors>) -> Self {
        let panes = config
            .tracked
            .iter()
            .cloned()
            .map(PaneState::new)
            .collect();
        Self {
            config,
            panes,
            focused: 0,
            show_help: false,
            zoomed: false,
            host_colors,
        }
    }

    pub fn apply_update(&mut self, update: PaneUpdate) {
        if let Some(pane) = self.panes.get_mut(update.index) {
            pane.status = update.status;
            pane.error = update.error;
            pane.last_update = Some(update.at);
            if let Some(capture) = update.capture {
                let new_hash = hash_capture(&capture);
                if pane.last_hash.map(|h| h != new_hash).unwrap_or(true) {
                    pane.last_change = Some(update.at);
                }
                pane.last_hash = Some(new_hash);
                pane.last_capture = Some(capture);
            }
        }
    }

    pub fn refresh_stale(&mut self) {
        let stale_after = Duration::from_millis(self.config.ui.refresh_ms.saturating_mul(2));
        let now = Instant::now();
        for pane in &mut self.panes {
            if matches!(pane.status, PaneStatus::Down) {
                continue;
            }
            match pane.last_update {
                Some(last) if now.duration_since(last) > stale_after => {
                    pane.status = PaneStatus::Stale;
                }
                Some(_) => {
                    pane.status = PaneStatus::Ok;
                }
                None => {
                    pane.status = PaneStatus::Stale;
                }
            }
        }
    }

    pub fn is_active(&self, index: usize) -> bool {
        let active_for = Duration::from_secs(2);
        self.panes
            .get(index)
            .and_then(|pane| pane.last_change)
            .map(|when| Instant::now().duration_since(when) <= active_for)
            .unwrap_or(false)
    }
}

pub fn hash_capture(capture: &PaneCapture) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let prime = 0x100000001b3u64;
    for line in &capture.lines {
        for byte in line.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(prime);
        }
        hash ^= b'\n' as u64;
        hash = hash.wrapping_mul(prime);
    }
    for byte in capture.command.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(prime);
    }
    hash
}
