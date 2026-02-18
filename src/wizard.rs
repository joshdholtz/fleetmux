use crate::config::{Config, HostConfig, TrackedPane};
use crate::ssh::HostResolver;
use crate::tmux::{self, PaneInfo};
use anyhow::{anyhow, Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug)]
struct WindowSelection {
    host: String,
    session: String,
    window: u32,
    window_name: String,
    panes: Vec<PaneInfo>,
}

pub fn run_host_setup(path: &Path) -> Result<Config> {
    println!("FleetMux setup wizard");
    println!("Config will be saved to {}", path.display());

    let mut config = Config::default();
    let theme = ColorfulTheme::default();

    loop {
        if !config.hosts.is_empty() {
            let add_more = Confirm::with_theme(&theme)
                .with_prompt("Add another host?")
                .default(true)
                .interact()?;
            if !add_more {
                break;
            }
        }

        let name: String = Input::with_theme(&theme)
            .with_prompt("Host name")
            .interact_text()?;
        let targets_raw: String = Input::with_theme(&theme)
            .with_prompt("SSH targets (comma separated)")
            .interact_text()?;
        let targets: Vec<String> = targets_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if targets.is_empty() {
            return Err(anyhow!("At least one SSH target is required"));
        }

        let mut palette = config.colors.default_host_palette.clone();
        palette.insert(0, "Auto".to_string());
        let selection = Select::with_theme(&theme)
            .with_prompt("Host color")
            .items(&palette)
            .default(0)
            .interact()?;
        let color = if selection == 0 {
            None
        } else {
            Some(palette[selection].clone())
        };

        let host = HostConfig {
            name: name.clone(),
            targets,
            strategy: Some("auto".to_string()),
            color,
            tags: None,
        };

        println!("Testing connectivity for {name}...");
        for target in &host.targets {
            match crate::ssh::test_target_blocking(target, &config.ssh) {
                Ok(()) => println!("  OK  {target}"),
                Err(err) => println!("  FAIL {target}: {err}"),
            }
        }

        config.hosts.push(host);
    }

    if config.hosts.is_empty() {
        return Err(anyhow!("No hosts configured"));
    }

    crate::config::save(path, &config).context("Unable to save config")?;
    println!("Saved config to {}", path.display());

    Ok(config)
}

pub fn select_windows(config: &Config) -> Result<Vec<TrackedPane>> {
    let theme = ColorfulTheme::default();

    loop {
        let windows = discover_windows(config, &theme)?;
        if windows.is_empty() {
            return Err(anyhow!("No tmux windows discovered"));
        }

        let selections = select_window_items(&windows, &theme)?;
        let tracked = build_tracked_from_windows(&windows, &selections);

        if tracked.is_empty() {
            return Err(anyhow!("At least one window is required"));
        }

        if tracked.len() > 10 {
            println!(
                "Selected windows contain {} panes. Limit is 10. Select fewer windows.",
                tracked.len()
            );
            continue;
        }

        return Ok(tracked);
    }
}

fn discover_windows(config: &Config, theme: &ColorfulTheme) -> Result<Vec<WindowSelection>> {
    let mut resolver = HostResolver::new();
    let mut windows = Vec::new();

    for host in &config.hosts {
        let target = match resolver.resolve_target_blocking(host, &config.ssh) {
            Ok(target) => target,
            Err(err) => {
                println!("Skipping {}: {err}", host.name);
                continue;
            }
        };

        println!("Discovering windows on {} ({})", host.name, target);

        let window_names = match tmux::list_windows_blocking(&target, &config.ssh) {
            Ok(list) => list,
            Err(err) => {
                println!("Warning: failed to list windows on {}: {err}", host.name);
                Vec::new()
            }
        };

        let panes = tmux::list_panes_blocking(&target, &config.ssh)
            .with_context(|| format!("Failed to list panes on {}", host.name))?;

        let mut panes_by_window: HashMap<(String, u32), Vec<PaneInfo>> = HashMap::new();
        for pane in panes {
            panes_by_window
                .entry((pane.session.clone(), pane.window))
                .or_default()
                .push(pane);
        }

        let mut names_by_window: HashMap<(String, u32), String> = HashMap::new();
        for window in window_names {
            names_by_window.insert((window.session, window.window), window.name);
        }

        for ((session, window), panes) in panes_by_window {
            let window_name = names_by_window
                .remove(&(session.clone(), window))
                .unwrap_or_default();
            windows.push(WindowSelection {
                host: host.name.clone(),
                session,
                window,
                window_name,
                panes,
            });
        }
    }

    windows.sort_by(|a, b| {
        (a.host.as_str(), a.session.as_str(), a.window)
            .cmp(&(b.host.as_str(), b.session.as_str(), b.window))
    });

    if windows.is_empty() {
        let retry = Confirm::with_theme(theme)
            .with_prompt("No windows found. Continue anyway?")
            .default(false)
            .interact()?;
        if !retry {
            return Err(anyhow!("No tmux windows discovered"));
        }
    }

    Ok(windows)
}

fn select_window_items(
    windows: &[WindowSelection],
    theme: &ColorfulTheme,
) -> Result<Vec<usize>> {
    let items: Vec<String> = windows
        .iter()
        .map(|window| {
            let name = if window.window_name.is_empty() {
                "(unnamed)".to_string()
            } else {
                window.window_name.clone()
            };
            format!(
                "{} {}:{} {} ({} panes)",
                window.host,
                window.session,
                window.window,
                name,
                window.panes.len()
            )
        })
        .collect();

    let selections = MultiSelect::with_theme(theme)
        .with_prompt("Select windows to monitor (total panes must be <= 10)")
        .items(&items)
        .interact()?;

    if selections.is_empty() {
        return Err(anyhow!("At least one window is required"));
    }

    Ok(selections)
}

fn build_tracked_from_windows(
    windows: &[WindowSelection],
    selections: &[usize],
) -> Vec<TrackedPane> {
    let mut tracked = Vec::new();

    for &index in selections {
        if let Some(window) = windows.get(index) {
            let label = if window.window_name.is_empty() {
                None
            } else {
                Some(window.window_name.clone())
            };
            for pane in &window.panes {
                tracked.push(TrackedPane {
                    host: window.host.clone(),
                    session: pane.session.clone(),
                    window: pane.window,
                    pane_id: pane.pane_id.clone(),
                    label: label.clone(),
                });
            }
        }
    }

    tracked
}
