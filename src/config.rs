use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
    pub colors: ColorConfig,
    pub ssh: SshConfig,
    pub local: LocalConfig,
    pub hosts: Vec<HostConfig>,
    pub tracked: Vec<TrackedPane>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
            colors: ColorConfig::default(),
            ssh: SshConfig::default(),
            local: LocalConfig::default(),
            hosts: Vec::new(),
            tracked: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub refresh_ms: u64,
    pub lines: usize,
    pub layout: String,
    pub theme: String,
    pub compact: bool,
    pub ansi: bool,
    pub join_lines: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            refresh_ms: 750,
            lines: 40,
            layout: "auto".to_string(),
            theme: "default".to_string(),
            compact: false,
            ansi: true,
            join_lines: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorConfig {
    pub default_host_palette: Vec<String>,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            default_host_palette: vec![
                "Blue".to_string(),
                "Cyan".to_string(),
                "Green".to_string(),
                "Magenta".to_string(),
                "Yellow".to_string(),
                "LightBlue".to_string(),
                "LightGreen".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SshConfig {
    pub connect_timeout_sec: u64,
    pub control_master: bool,
    pub control_persist_sec: u64,
    pub path_extra: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalConfig {
    pub enabled: bool,
    pub name: String,
    pub color: Option<String>,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            name: "local".to_string(),
            color: None,
        }
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            connect_timeout_sec: 2,
            control_master: true,
            control_persist_sec: 600,
            path_extra: vec![
                "/usr/local/bin".to_string(),
                "/opt/homebrew/bin".to_string(),
            ],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostConfig {
    pub name: String,
    pub targets: Vec<String>,
    pub strategy: Option<String>,
    pub color: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackedPane {
    pub host: String,
    pub session: String,
    pub window: u32,
    pub pane_id: String,
    pub label: Option<String>,
}

pub fn config_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME not set"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("fleetmux")
        .join("config.toml"))
}

pub fn load(path: &Path) -> Result<Config> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Unable to read config file: {}", path.display()))?;
    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("Unable to parse config file: {}", path.display()))?;
    Ok(config)
}

pub fn save(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Unable to create config directory: {}", parent.display())
        })?;
    }
    let contents = toml::to_string_pretty(config).context("Unable to serialize config")?;
    fs::write(path, contents)
        .with_context(|| format!("Unable to write config file: {}", path.display()))?;
    Ok(())
}
