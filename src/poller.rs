use crate::config::Config;
use crate::model::{PaneStatus, PaneUpdate};
use crate::ssh::HostResolver;
use crate::tmux;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task::JoinHandle;

pub struct PollerHandle {
    shutdown: broadcast::Sender<()>,
    tasks: Vec<JoinHandle<()>>,
}

impl PollerHandle {
    pub async fn stop(&mut self) {
        let _ = self.shutdown.send(());
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
    }
}

pub fn start_pollers(
    config: &Config,
    resolver: Arc<Mutex<HostResolver>>,
    tx: mpsc::Sender<PaneUpdate>,
) -> PollerHandle {
    let (shutdown, _) = broadcast::channel(1);
    let mut tasks = Vec::new();

    for (index, tracked) in config.tracked.iter().cloned().enumerate() {
        let host = config.hosts.iter().find(|h| h.name == tracked.host).cloned();
        let ssh_cfg = config.ssh.clone();
        let refresh = Duration::from_millis(config.ui.refresh_ms);
        let lines = config.ui.lines;
        let mut shutdown_rx = shutdown.subscribe();
        let tx = tx.clone();
        let resolver = Arc::clone(&resolver);

        let handle = tokio::spawn(async move {
            loop {
                let now = Instant::now();
                let update = match &host {
                    Some(host_cfg) => {
                        let target = {
                            let mut resolver = resolver.lock().await;
                            resolver.resolve_target(host_cfg, &ssh_cfg).await
                        };

                        match target {
                            Ok(target) => match tmux::capture_pane(&target, &tracked.pane_id, lines, &ssh_cfg).await {
                                Ok(capture) => PaneUpdate {
                                    index,
                                    capture: Some(capture),
                                    status: PaneStatus::Ok,
                                    error: None,
                                    at: now,
                                },
                                Err(err) => PaneUpdate {
                                    index,
                                    capture: None,
                                    status: PaneStatus::Down,
                                    error: Some(err.to_string()),
                                    at: now,
                                },
                            },
                            Err(err) => PaneUpdate {
                                index,
                                capture: None,
                                status: PaneStatus::Down,
                                error: Some(err.to_string()),
                                at: now,
                            },
                        }
                    }
                    None => PaneUpdate {
                        index,
                        capture: None,
                        status: PaneStatus::Down,
                        error: Some("Unknown host".to_string()),
                        at: now,
                    },
                };

                let _ = tx.send(update).await;

                tokio::select! {
                    _ = tokio::time::sleep(refresh) => {},
                    _ = shutdown_rx.recv() => break,
                }
            }
        });

        tasks.push(handle);
    }

    PollerHandle { shutdown, tasks }
}
