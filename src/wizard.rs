use crate::config::{Config, HostConfig, TrackedPane};
use crate::ssh::HostResolver;
use crate::tmux::{self, PaneInfo};
use anyhow::{anyhow, Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use std::path::Path;

pub fn run(path: &Path) -> Result<Config> {
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

    let panes = discover_panes(&config, &theme)?;
    if panes.is_empty() {
        return Err(anyhow!("No tmux panes discovered"));
    }

    let selections = select_panes(&panes, &theme)?;
    let tracked = build_tracked(&panes, &selections, &theme)?;
    config.tracked = tracked;

    crate::config::save(path, &config).context("Unable to save config")?;
    println!("Saved config to {}", path.display());

    Ok(config)
}

fn discover_panes(config: &Config, theme: &ColorfulTheme) -> Result<Vec<(String, PaneInfo)>> {
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
        let mut host_panes = tmux::list_panes_blocking(&target, &config.ssh)
            .with_context(|| format!("Failed to list panes on {}", host.name))?;
        host_panes.sort_by(|a, b| a.session.cmp(&b.session));
        for pane in host_panes {
            panes.push((host.name.clone(), pane));
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

fn select_panes(
    panes: &[(String, PaneInfo)],
    theme: &ColorfulTheme,
) -> Result<Vec<usize>> {
    let items: Vec<String> = panes
        .iter()
        .map(|(host, pane)| {
            let mut summary = format!(
                "{} {}:{} {} {}",
                host, pane.session, pane.window, pane.pane_id, pane.command
            );
            if !pane.title.is_empty() {
                summary.push_str(" - ");
                summary.push_str(&pane.title);
            }
            summary
        })
        .collect();

    let selections = MultiSelect::with_theme(theme)
        .with_prompt("Select up to 10 panes")
        .items(&items)
        .interact()?;

    if selections.len() > 10 {
        return Err(anyhow!("Please select at most 10 panes"));
    }

    if selections.is_empty() {
        return Err(anyhow!("At least one pane is required"));
    }

    Ok(selections)
}

fn build_tracked(
    panes: &[(String, PaneInfo)],
    selections: &[usize],
    theme: &ColorfulTheme,
) -> Result<Vec<TrackedPane>> {
    let mut tracked = Vec::new();

    for &index in selections {
        let (host, pane) = &panes[index];
        let prompt = format!("Label for {} {}:{} {}", host, pane.session, pane.window, pane.pane_id);
        let label: String = Input::with_theme(theme)
            .with_prompt(prompt)
            .allow_empty(true)
            .interact_text()?;

        tracked.push(TrackedPane {
            host: host.clone(),
            session: pane.session.clone(),
            window: pane.window,
            pane_id: pane.pane_id.clone(),
            label: if label.trim().is_empty() {
                None
            } else {
                Some(label)
            },
        });
    }

    Ok(tracked)
}
