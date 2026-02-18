use crate::config::{HostConfig, SshConfig};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;

const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct CacheEntry {
    target: String,
    checked_at: Instant,
}

#[derive(Debug, Default)]
pub struct HostResolver {
    cache: HashMap<String, CacheEntry>,
}

impl HostResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub async fn resolve_target(&mut self, host: &HostConfig, ssh: &SshConfig) -> Result<String> {
        if let Some(entry) = self.cache.get(&host.name) {
            if entry.checked_at.elapsed() < CACHE_TTL {
                return Ok(entry.target.clone());
            }
        }

        for target in &host.targets {
            if test_target(target, ssh).await.is_ok() {
                self.cache.insert(
                    host.name.clone(),
                    CacheEntry {
                        target: target.clone(),
                        checked_at: Instant::now(),
                    },
                );
                return Ok(target.clone());
            }
        }

        Err(anyhow!("No reachable targets for host {}", host.name))
    }

    pub fn resolve_target_blocking(
        &mut self,
        host: &HostConfig,
        ssh: &SshConfig,
    ) -> Result<String> {
        if let Some(entry) = self.cache.get(&host.name) {
            if entry.checked_at.elapsed() < CACHE_TTL {
                return Ok(entry.target.clone());
            }
        }

        for target in &host.targets {
            if test_target_blocking(target, ssh).is_ok() {
                self.cache.insert(
                    host.name.clone(),
                    CacheEntry {
                        target: target.clone(),
                        checked_at: Instant::now(),
                    },
                );
                return Ok(target.clone());
            }
        }

        Err(anyhow!("No reachable targets for host {}", host.name))
    }
}

pub async fn run_ssh_command(target: &str, ssh: &SshConfig, remote_cmd: &str) -> Result<String> {
    let mut cmd = Command::new("ssh");
    for arg in build_ssh_args(ssh) {
        cmd.arg(arg);
    }
    cmd.arg(target).arg(remote_cmd);
    cmd.stdin(Stdio::null());
    let output = cmd
        .output()
        .await
        .with_context(|| format!("ssh command failed for {target}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_string())
    } else {
        Err(anyhow!(
            "ssh command failed for {target}: {}",
            String::from_utf8_lossy(&output.stderr).trim_end()
        ))
    }
}

pub fn run_ssh_command_blocking(
    target: &str,
    ssh: &SshConfig,
    remote_cmd: &str,
) -> Result<String> {
    let mut cmd = std::process::Command::new("ssh");
    for arg in build_ssh_args(ssh) {
        cmd.arg(arg);
    }
    cmd.arg(target).arg(remote_cmd);
    cmd.stdin(Stdio::null());
    let output = cmd
        .output()
        .with_context(|| format!("ssh command failed for {target}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_string())
    } else {
        Err(anyhow!(
            "ssh command failed for {target}: {}",
            String::from_utf8_lossy(&output.stderr).trim_end()
        ))
    }
}

pub async fn test_target(target: &str, ssh: &SshConfig) -> Result<()> {
    let cmd = "tmux -V";
    let timeout = Duration::from_secs(ssh.connect_timeout_sec.max(1));
    let result = tokio::time::timeout(timeout, run_ssh_command(target, ssh, cmd)).await;
    match result {
        Ok(res) => res.map(|_| ()),
        Err(_) => Err(anyhow!("ssh connect timeout for {target}")),
    }
}

pub fn test_target_blocking(target: &str, ssh: &SshConfig) -> Result<()> {
    let cmd = "tmux -V";
    run_ssh_command_blocking(target, ssh, cmd).map(|_| ())
}

pub fn build_ssh_args(ssh: &SshConfig) -> Vec<String> {
    let mut args = Vec::new();
    args.push("-o".to_string());
    args.push(format!("ConnectTimeout={}", ssh.connect_timeout_sec));
    args.push("-o".to_string());
    args.push("BatchMode=yes".to_string());
    args.push("-o".to_string());
    args.push("StrictHostKeyChecking=accept-new".to_string());
    args.push("-o".to_string());
    args.push("LogLevel=ERROR".to_string());
    if ssh.control_master {
        args.push("-o".to_string());
        args.push("ControlMaster=auto".to_string());
        args.push("-o".to_string());
        args.push(format!("ControlPersist={}", ssh.control_persist_sec));
        args.push("-o".to_string());
        args.push(format!("ControlPath={}", control_path()));
    }
    args
}

fn control_path() -> String {
    "/tmp/fleetmux-%r@%h:%p".to_string()
}
