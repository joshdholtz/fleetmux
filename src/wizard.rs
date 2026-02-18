use crate::config::{Config, HostConfig, TrackedPane};
use crate::ssh::HostResolver;
use crate::tmux::{self, WindowInfo};
use anyhow::{anyhow, Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug)]
struct PaneSelection {
    host: String,
    session: String,
    window: u32,
    window_name: String,
    pane_id: String,
    command: String,
    title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct PaneKey {
    host: String,
    session: String,
    window: u32,
    pane_id: String,
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

pub fn select_panes(config: &Config) -> Result<Vec<TrackedPane>> {
    let theme = ColorfulTheme::default();
    let previous = previous_pane_keys(&config.tracked);
    let previous_labels = previous_pane_labels(&config.tracked);

    loop {
        let panes = discover_panes(config, &theme)?;
        if panes.is_empty() {
            return Err(anyhow!("No tmux panes discovered"));
        }

        let selected = select_panes_tree(&panes, &previous, &theme)?;
        let tracked = build_tracked_from_panes(&panes, &selected, &previous_labels);

        if tracked.is_empty() {
            return Err(anyhow!("At least one pane is required"));
        }

        if tracked.len() > 10 {
            println!(
                "Selected panes: {}. Limit is 10. Select fewer panes.",
                tracked.len()
            );
            continue;
        }

        return Ok(tracked);
    }
}

fn discover_panes(config: &Config, theme: &ColorfulTheme) -> Result<Vec<PaneSelection>> {
    let mut resolver = HostResolver::new();
    let mut panes = Vec::new();

    for host in &config.hosts {
        let target = match resolver.resolve_target_blocking(host, &config.ssh) {
            Ok(target) => target,
            Err(err) => {
                println!("Skipping {}: {err}", host.name);
                continue;
            }
        };

        println!("Discovering panes on {} ({})", host.name, target);

        let window_names = tmux::list_windows_blocking(&target, &config.ssh)
            .unwrap_or_else(|_| Vec::<WindowInfo>::new());
        let mut window_map: HashMap<(String, u32), String> = HashMap::new();
        for window in window_names {
            window_map.insert((window.session, window.window), window.name);
        }

        let mut host_panes = tmux::list_panes_blocking(&target, &config.ssh)
            .with_context(|| format!("Failed to list panes on {}", host.name))?;
        host_panes.sort_by(|a, b| a.session.cmp(&b.session));
        for pane in host_panes {
            let window_name = window_map
                .get(&(pane.session.clone(), pane.window))
                .cloned()
                .unwrap_or_default();
            panes.push(PaneSelection {
                host: host.name.clone(),
                session: pane.session,
                window: pane.window,
                window_name,
                pane_id: pane.pane_id,
                command: pane.command,
                title: pane.title,
            });
        }
    }

    if panes.is_empty() {
        let retry = Confirm::with_theme(theme)
            .with_prompt("No panes found. Continue anyway?")
            .default(false)
            .interact()?;
        if !retry {
            return Err(anyhow!("No tmux panes discovered"));
        }
    }

    Ok(panes)
}

fn select_panes_tree(
    panes: &[PaneSelection],
    previous: &HashSet<PaneKey>,
    theme: &ColorfulTheme,
) -> Result<Vec<PaneKey>> {
    let mut by_host: BTreeMap<String, BTreeMap<String, BTreeMap<u32, Vec<PaneSelection>>>> =
        BTreeMap::new();
    for pane in panes {
        by_host
            .entry(pane.host.clone())
            .or_default()
            .entry(pane.session.clone())
            .or_default()
            .entry(pane.window)
            .or_default()
            .push(pane.clone());
    }

    let hosts: Vec<String> = by_host.keys().cloned().collect();
    let selected_hosts = if hosts.len() == 1 {
        hosts.clone()
    } else {
        let defaults: Vec<bool> = hosts
            .iter()
            .map(|host| previous.iter().any(|key| &key.host == host))
            .collect();
        let selected_indexes = MultiSelect::with_theme(theme)
            .with_prompt("Select hosts")
            .items(&hosts)
            .defaults(&defaults)
            .interact()?;
        selected_indexes
            .into_iter()
            .filter_map(|idx| hosts.get(idx).cloned())
            .collect()
    };

    if selected_hosts.is_empty() {
        return Err(anyhow!("At least one host is required"));
    }

    let mut selected = Vec::new();

    for host in selected_hosts {
        let sessions_map = match by_host.get(&host) {
            Some(map) => map,
            None => continue,
        };
        let sessions: Vec<String> = sessions_map.keys().cloned().collect();
        let session_defaults: Vec<bool> = sessions
            .iter()
            .map(|session| {
                previous
                    .iter()
                    .any(|key| key.host == host && key.session == *session)
            })
            .collect();
        let selected_sessions_idx = MultiSelect::with_theme(theme)
            .with_prompt(format!("Select sessions on {host}"))
            .items(&sessions)
            .defaults(&session_defaults)
            .interact()?;
        if selected_sessions_idx.is_empty() {
            continue;
        }

        for session_idx in selected_sessions_idx {
            let session = match sessions.get(session_idx) {
                Some(val) => val.clone(),
                None => continue,
            };
            let windows_map = match sessions_map.get(&session) {
                Some(map) => map,
                None => continue,
            };
            let mut windows: Vec<(u32, String, usize)> = Vec::new();
            for (window, panes) in windows_map {
                let name = panes
                    .first()
                    .map(|pane| pane.window_name.clone())
                    .unwrap_or_default();
                windows.push((*window, name, panes.len()));
            }
            windows.sort_by(|a, b| a.0.cmp(&b.0));
            let window_items: Vec<String> = windows
                .iter()
                .map(|(window, name, count)| {
                    let label = if name.is_empty() {
                        "(unnamed)".to_string()
                    } else {
                        name.clone()
                    };
                    format!("{}:{} {} ({} panes)", session, window, label, count)
                })
                .collect();
            let window_defaults: Vec<bool> = windows
                .iter()
                .map(|(window, _, _)| {
                    previous.iter().any(|key| {
                        key.host == host && key.session == session && key.window == *window
                    })
                })
                .collect();
            let selected_windows_idx = MultiSelect::with_theme(theme)
                .with_prompt(format!("Select windows in {host} {session}"))
                .items(&window_items)
                .defaults(&window_defaults)
                .interact()?;
            if selected_windows_idx.is_empty() {
                continue;
            }

            for window_idx in selected_windows_idx {
                let (window_id, _, _) = match windows.get(window_idx) {
                    Some(val) => val.clone(),
                    None => continue,
                };
                let panes_in_window = match windows_map.get(&window_id) {
                    Some(val) => val,
                    None => continue,
                };
                let pane_items: Vec<String> = panes_in_window
                    .iter()
                    .map(|pane| {
                        let title = if pane.title.is_empty() {
                            "".to_string()
                        } else {
                            format!(" - {}", pane.title)
                        };
                        format!("{} {}{}", pane.pane_id, pane.command, title)
                    })
                    .collect();
                let pane_defaults: Vec<bool> = panes_in_window
                    .iter()
                    .map(|pane| {
                        previous.contains(&PaneKey {
                            host: host.clone(),
                            session: session.clone(),
                            window: window_id,
                            pane_id: pane.pane_id.clone(),
                        })
                    })
                    .collect();
                let selected_panes_idx = MultiSelect::with_theme(theme)
                    .with_prompt(format!(
                        "Select panes in {host} {session}:{window_id}"
                    ))
                    .items(&pane_items)
                    .defaults(&pane_defaults)
                    .interact()?;

                for pane_idx in selected_panes_idx {
                    if let Some(pane) = panes_in_window.get(pane_idx) {
                        selected.push(PaneKey {
                            host: host.clone(),
                            session: session.clone(),
                            window: window_id,
                            pane_id: pane.pane_id.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(selected)
}

fn build_tracked_from_panes(
    panes: &[PaneSelection],
    selections: &[PaneKey],
    previous_labels: &HashMap<PaneKey, String>,
) -> Vec<TrackedPane> {
    let mut pane_map: HashMap<PaneKey, PaneSelection> = HashMap::new();
    for pane in panes {
        pane_map.insert(
            PaneKey {
                host: pane.host.clone(),
                session: pane.session.clone(),
                window: pane.window,
                pane_id: pane.pane_id.clone(),
            },
            pane.clone(),
        );
    }

    let mut tracked = Vec::new();
    for key in selections {
        let pane = match pane_map.get(key) {
            Some(pane) => pane,
            None => continue,
        };
        let label = previous_labels.get(key).cloned();
        tracked.push(TrackedPane {
            host: pane.host.clone(),
            session: pane.session.clone(),
            window: pane.window,
            pane_id: pane.pane_id.clone(),
            label,
        });
    }

    tracked
}

fn previous_pane_keys(tracked: &[TrackedPane]) -> HashSet<PaneKey> {
    tracked
        .iter()
        .map(|pane| PaneKey {
            host: pane.host.clone(),
            session: pane.session.clone(),
            window: pane.window,
            pane_id: pane.pane_id.clone(),
        })
        .collect()
}

fn previous_pane_labels(tracked: &[TrackedPane]) -> HashMap<PaneKey, String> {
    let mut labels = HashMap::new();
    for pane in tracked {
        if let Some(label) = &pane.label {
            if label.is_empty() {
                continue;
            }
            labels
                .entry(PaneKey {
                    host: pane.host.clone(),
                    session: pane.session.clone(),
                    window: pane.window,
                    pane_id: pane.pane_id.clone(),
                })
                .or_insert_with(|| label.clone());
        }
    }
    labels
}
