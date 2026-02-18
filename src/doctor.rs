use crate::config::Config;
use crate::ssh::HostResolver;
use crate::tmux;
use anyhow::Result;

pub async fn run(config: &Config) -> Result<()> {
    println!("FleetMux doctor");
    println!("Hosts: {}", config.hosts.len());

    let mut resolver = HostResolver::new();

    for host in &config.hosts {
        println!();
        println!("Host: {}", host.name);
        println!("Targets: {}", host.targets.join(", "));
        if let Some(color) = &host.color {
            println!("Color: {}", color);
        }

        let target = match resolver.resolve_target(host, &config.ssh).await {
            Ok(target) => {
                println!("Resolved target: {}", target);
                target
            }
            Err(err) => {
                println!("Resolve error: {err}");
                continue;
            }
        };

        match crate::ssh::run_ssh_command(&target, &config.ssh, "tmux -V").await {
            Ok(output) => println!("tmux: {}", output),
            Err(err) => {
                println!("tmux error: {err}");
                continue;
            }
        }

        match tmux::list_windows(&target, &config.ssh).await {
            Ok(windows) => {
                println!("Windows: {}", windows.len());
                for window in &windows {
                    let name = if window.name.is_empty() {
                        "(unnamed)"
                    } else {
                        &window.name
                    };
                    println!("  {}:{} {}", window.session, window.window, name);
                }
            }
            Err(err) => println!("Windows error: {err}"),
        }

        let panes = match tmux::list_panes(&target, &config.ssh).await {
            Ok(panes) => panes,
            Err(err) => {
                println!("Panes error: {err}");
                continue;
            }
        };
        println!("Panes: {}", panes.len());
        for pane in &panes {
            let title = if pane.title.is_empty() {
                ""
            } else {
                &pane.title
            };
            println!(
                "  {}:{} {} {} {}",
                pane.session, pane.window, pane.pane_id, pane.command, title
            );
        }

        if let Some(pane) = panes.first() {
            println!("Capture sample: {}:{} {}", pane.session, pane.window, pane.pane_id);
            match tmux::capture_pane(&target, &pane.pane_id, 10, &config.ssh).await {
                Ok(capture) => {
                    for line in capture.lines {
                        println!("  {line}");
                    }
                }
                Err(err) => println!("Capture error: {err}"),
            }
        }
    }

    Ok(())
}
