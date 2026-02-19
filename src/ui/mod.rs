pub mod dashboard;

use anyhow::Result;
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout, Write};

pub type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

pub fn enter_terminal() -> Result<AppTerminal> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub fn exit_terminal(terminal: &mut AppTerminal) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn bell() -> Result<()> {
    let mut stdout = io::stdout();
    stdout.write_all(b"\x07")?;
    stdout.flush()?;
    Ok(())
}

pub fn notify_macos(title: &str, message: &str, sender: Option<&str>) -> Result<()> {
    notify_macos_impl(title, message, sender)
}

pub fn macos_frontmost_app() -> Result<Option<String>> {
    macos_frontmost_app_impl()
}

#[cfg(target_os = "macos")]
fn notify_macos_impl(title: &str, message: &str, sender: Option<&str>) -> Result<()> {
    if terminal_notifier_available() {
        let mut cmd = std::process::Command::new("terminal-notifier");
        cmd.arg("-title").arg(title);
        cmd.arg("-message").arg(message);
        if let Some(sender) = sender {
            let sender = sender.trim();
            if !sender.is_empty() {
                cmd.arg("-sender").arg(sender);
            }
        }
        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("terminal-notifier failed with status {status}"));
        }
        return Ok(());
    }

    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escape_applescript(message),
        escape_applescript(title)
    );
    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("osascript failed with status {status}"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_frontmost_app_impl() -> Result<Option<String>> {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first application process whose frontmost is true")
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

#[cfg(not(target_os = "macos"))]
fn notify_macos_impl(_title: &str, _message: &str, _sender: Option<&str>) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn terminal_notifier_available() -> bool {
    std::process::Command::new("sh")
        .arg("-lc")
        .arg("command -v terminal-notifier")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn terminal_notifier_available() -> bool {
    false
}

fn escape_applescript(input: &str) -> String {
    input.replace('\\', "\\\\").replace('\"', "\\\"")
}

#[cfg(not(target_os = "macos"))]
fn macos_frontmost_app_impl() -> Result<Option<String>> {
    Ok(None)
}
