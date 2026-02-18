use crate::config::SshConfig;
use crate::model::PaneCapture;
use crate::ssh;
use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct PaneInfo {
    pub session: String,
    pub window: u32,
    pub pane_id: String,
    pub command: String,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub session: String,
    pub window: u32,
    pub name: String,
}

pub async fn capture_pane(
    target: &str,
    pane_id: &str,
    lines: usize,
    join_lines: bool,
    ansi: bool,
    ssh_cfg: &SshConfig,
) -> Result<PaneCapture> {
    let mut capture_cmd = String::from("tmux capture-pane -p ");
    if ansi {
        capture_cmd.push_str("-e ");
    }
    if join_lines {
        capture_cmd.push_str("-J ");
    }
    capture_cmd.push_str(&format!("-t {pane_id} -S -{lines}"));
    let cmd = format!(
        "tmux display-message -p -t {pane_id} '#{{pane_current_command}}\t#{{pane_title}}' \
         && {capture_cmd}"
    );
    let output = ssh::run_ssh_command(target, ssh_cfg, &cmd)
        .await
        .with_context(|| format!("capture-pane failed for {target} ({cmd})"))?;
    parse_capture(&output)
}

pub async fn list_panes(target: &str, ssh_cfg: &SshConfig) -> Result<Vec<PaneInfo>> {
    let cmd = "tmux list-panes -a -F \"#{session_name}\t#{window_index}\t#{pane_id}\t#{pane_current_command}\t#{pane_title}\"";
    let output = ssh::run_ssh_command(target, ssh_cfg, cmd)
        .await
        .with_context(|| format!("list-panes failed for {target}"))?;
    Ok(parse_pane_list(&output))
}


pub async fn list_windows(target: &str, ssh_cfg: &SshConfig) -> Result<Vec<WindowInfo>> {
    let cmd = "tmux list-windows -a -F \"#{session_name}\t#{window_index}\t#{window_name}\"";
    let output = ssh::run_ssh_command(target, ssh_cfg, cmd)
        .await
        .with_context(|| format!("list-windows failed for {target}"))?;
    Ok(parse_window_list(&output))
}


fn parse_capture(output: &str) -> Result<PaneCapture> {
    let mut lines = output.lines();
    let header = lines.next().unwrap_or("");
    let mut header_parts = header.split('\t');
    let command = header_parts.next().unwrap_or("").to_string();
    let title = header_parts.next().unwrap_or("").to_string();
    let body_lines = lines.map(|line| line.to_string()).collect();
    Ok(PaneCapture {
        command,
        title,
        lines: body_lines,
    })
}

fn parse_pane_list(output: &str) -> Vec<PaneInfo> {
    let mut panes = Vec::new();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let session = match parts.next() {
            Some(val) if !val.is_empty() => val.to_string(),
            _ => continue,
        };
        let window = match parts.next().and_then(|val| val.parse::<u32>().ok()) {
            Some(val) => val,
            None => continue,
        };
        let pane_id = match parts.next() {
            Some(val) if !val.is_empty() => val.to_string(),
            _ => continue,
        };
        let command = parts.next().unwrap_or("").to_string();
        let title = parts.next().unwrap_or("").to_string();
        panes.push(PaneInfo {
            session,
            window,
            pane_id,
            command,
            title,
        });
    }
    panes
}

fn parse_window_list(output: &str) -> Vec<WindowInfo> {
    let mut windows = Vec::new();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let session = match parts.next() {
            Some(val) if !val.is_empty() => val.to_string(),
            _ => continue,
        };
        let window = match parts.next().and_then(|val| val.parse::<u32>().ok()) {
            Some(val) => val,
            None => continue,
        };
        let name = parts.next().unwrap_or("").to_string();
        windows.push(WindowInfo {
            session,
            window,
            name,
        });
    }
    windows
}
