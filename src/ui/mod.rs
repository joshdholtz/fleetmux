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

pub fn notify_macos(title: &str, message: &str) -> Result<()> {
    notify_macos_impl(title, message)
}

#[cfg(target_os = "macos")]
fn notify_macos_impl(title: &str, message: &str) -> Result<()> {
    let script = format!("display notification \"{}\" with title \"{}\"", message, title);
    let status = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!("osascript failed with status {status}"));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn notify_macos_impl(_title: &str, _message: &str) -> Result<()> {
    Ok(())
}
