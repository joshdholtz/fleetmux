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

pub async fn capture_pane(
    target: &str,
    pane_id: &str,
    lines: usize,
    ssh_cfg: &SshConfig,
) -> Result<PaneCapture> {
    let cmd = format!(
        "tmux display-message -p -t {pane_id} '#{{pane_current_command}}\t#{{pane_title}}' \
         && tmux capture-pane -p -J -t {pane_id} -S -{lines}"
    );
    let output = ssh::run_ssh_command(target, ssh_cfg, &cmd)
        .await
        .with_context(|| format!("capture-pane failed for {target}"))?;
    parse_capture(&output)
}

pub fn list_panes_blocking(target: &str, ssh_cfg: &SshConfig) -> Result<Vec<PaneInfo>> {
    let cmd = "tmux list-panes -a -F \"#{session_name}\t#{window_index}\t#{pane_id}\t#{pane_current_command}\t#{pane_title}\"";
    let output = ssh::run_ssh_command_blocking(target, ssh_cfg, cmd)
        .with_context(|| format!("list-panes failed for {target}"))?;
    Ok(parse_pane_list(&output))
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
