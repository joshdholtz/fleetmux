use crate::model::{AppState, PaneStatus};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ansi_to_tui::IntoText as _;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, state: &AppState) {
    let area = f.area();
    if state.panes.is_empty() {
        let block = Block::default().borders(Borders::ALL).title("fleetmux");
        let paragraph = Paragraph::new("No tracked panes. Run the setup wizard.").block(block);
        f.render_widget(paragraph, area);
        return;
    }

    if state.zoomed {
        let index = state.focused.min(state.panes.len().saturating_sub(1));
        draw_tile(f, state, index, area, true);
    } else {
        let tiles = grid_layout(area, state.panes.len());
        for (index, rect) in tiles.into_iter().enumerate() {
            draw_tile(f, state, index, rect, index == state.focused);
        }
    }

    if state.show_help {
        draw_help(f, area);
    }
}

fn draw_tile(f: &mut Frame, state: &AppState, index: usize, area: Rect, focused: bool) {
    let pane = match state.panes.get(index) {
        Some(pane) => pane,
        None => return,
    };

    let colors = state
        .host_colors
        .get(&pane.tracked.host)
        .cloned()
        .unwrap_or_else(crate::model::default_host_colors);

    let border_color = if pane.status == PaneStatus::Down {
        Color::Red
    } else if focused {
        colors.focus
    } else {
        colors.base
    };

    let mut border_style = Style::default().fg(border_color);
    if pane.status == PaneStatus::Down {
        border_style = border_style.add_modifier(Modifier::DIM);
    }
    if pane.status == PaneStatus::Stale {
        border_style = border_style.add_modifier(Modifier::DIM);
    }
    if focused {
        border_style = border_style.add_modifier(Modifier::BOLD);
    }

    let title_color = title_color(border_color, &colors);
    let host_style = Style::default()
        .fg(title_color)
        .add_modifier(Modifier::BOLD);
    let title = build_title(
        pane.tracked.host.as_str(),
        pane,
        host_style,
        title_color,
        state.config.ui.compact,
        focused,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(if focused {
            BorderType::Double
        } else {
            BorderType::Plain
        })
        .title(title);

    let content = build_content(state, index, state.config.ui.compact);
    let inner_height = area.height.saturating_sub(2) as usize;
    let scroll = content
        .lines
        .saturating_sub(inner_height)
        .try_into()
        .unwrap_or(0u16);
    let paragraph = Paragraph::new(content.text)
        .block(block)
        .scroll((scroll, 0))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn build_title(
    host: &str,
    pane: &crate::model::PaneState,
    host_style: Style,
    title_color: Color,
    compact: bool,
    focused: bool,
) -> Line<'static> {
    let session_window = format!("{}:{}", pane.tracked.session, pane.tracked.window);
    let pane_id = format_pane_id(&pane.tracked.pane_id);
    let mut spans = Vec::new();
    if focused {
        spans.push(Span::styled("▶ ", host_style));
    }
    spans.push(Span::styled(host.to_string(), host_style));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(session_window, Style::default().fg(title_color)));

    let label = build_label(pane);
    if let Some(label) = label {
        spans.push(Span::raw(" — "));
        spans.push(Span::styled(
            label,
            Style::default().fg(title_color).add_modifier(Modifier::BOLD),
        ));
    }
    if compact {
        if let Some(age) = last_update_age(pane) {
            spans.push(Span::raw(" · "));
            spans.push(Span::styled(
                age,
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    if compact {
        let status = match pane.status {
            PaneStatus::Down => Some("DOWN"),
            PaneStatus::Stale => Some("STALE"),
            PaneStatus::Ok => None,
        };
        if let Some(status) = status {
            spans.push(Span::raw(" "));
            spans.push(Span::raw(format!("[{status}]")));
        }
    }

    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        format!("({})", pane_id),
        Style::default().fg(Color::DarkGray),
    ));
    Line::from(spans)
}

fn build_label(pane: &crate::model::PaneState) -> Option<String> {
    if let Some(label) = &pane.tracked.label {
        if !label.is_empty() {
            return Some(label.clone());
        }
    }
    if let Some(capture) = &pane.last_capture {
        if !capture.title.is_empty() {
            return Some(capture.title.clone());
        }
        if !capture.command.is_empty() {
            return Some(capture.command.clone());
        }
    }
    None
}

fn format_pane_id(pane_id: &str) -> String {
    match pane_id.strip_prefix('%') {
        Some(id) => format!("pane {id}"),
        None => format!("pane {pane_id}"),
    }
}

fn title_color(border_color: Color, colors: &crate::model::HostColors) -> Color {
    if border_color == colors.base {
        colors.focus
    } else {
        colors.base
    }
}

struct Content {
    text: Text<'static>,
    lines: usize,
}

fn build_content(state: &AppState, index: usize, compact: bool) -> Content {
    let pane = &state.panes[index];
    if state.config.ui.ansi {
        let raw = build_raw_content(state, pane, index, compact);
        let line_count = raw.lines().count().max(1);
        let text = raw.into_text().unwrap_or_else(|_| Text::from(raw));
        return Content {
            text,
            lines: line_count,
        };
    }

    let mut lines = Vec::new();

    let status_label = match pane.status {
        PaneStatus::Ok => "OK",
        PaneStatus::Down => "DOWN",
        PaneStatus::Stale => "STALE",
    };
    if compact {
        if pane.status != PaneStatus::Ok {
            let status_line = format!("Status: {status_label}");
            lines.push(Line::from(status_line));
        }
        if let Some(capture) = &pane.last_capture {
            for line in &capture.lines {
                lines.push(Line::from(line.clone()));
            }
        } else {
            lines.push(Line::from("Waiting for data..."));
        }
        if pane.status == PaneStatus::Down {
            if let Some(err) = &pane.error {
                lines.push(Line::from(""));
                lines.push(Line::from(format!("Error: {err}")));
            }
        }
    } else {
        let activity = if state.is_active(index) { "*" } else { "" };
        let status_line = format!("Status: {status_label} {activity}");
        lines.push(Line::from(status_line));

        if let Some(capture) = &pane.last_capture {
            if !capture.command.is_empty() {
                lines.push(Line::from(format!("Cmd: {}", capture.command)));
            } else if !capture.title.is_empty() {
                lines.push(Line::from(format!("Title: {}", capture.title)));
            }
            if let Some(label) = &pane.tracked.label {
                lines.push(Line::from(format!("Label: {label}")));
            }
            lines.push(Line::from(""));
            for line in &capture.lines {
                lines.push(Line::from(line.clone()));
            }
        } else {
            lines.push(Line::from("Waiting for data..."));
        }

        if pane.status == PaneStatus::Down {
            if let Some(err) = &pane.error {
                lines.push(Line::from(""));
                lines.push(Line::from(format!("Error: {err}")));
            }
        }
    }

    let line_count = lines.len().max(1);
    Content {
        text: Text::from(lines),
        lines: line_count,
    }
}

fn build_raw_content(
    state: &AppState,
    pane: &crate::model::PaneState,
    index: usize,
    compact: bool,
) -> String {
    let mut raw = String::new();
    let status_label = match pane.status {
        PaneStatus::Ok => "OK",
        PaneStatus::Down => "DOWN",
        PaneStatus::Stale => "STALE",
    };

    if compact {
        if pane.status != PaneStatus::Ok {
            raw.push_str(&format!("Status: {status_label}\n"));
        }
        if let Some(capture) = &pane.last_capture {
            raw.push_str(&capture.lines.join("\n"));
        } else {
            raw.push_str("Waiting for data...");
        }
        if pane.status == PaneStatus::Down {
            if let Some(err) = &pane.error {
                raw.push('\n');
                raw.push_str(&format!("Error: {err}"));
            }
        }
        return raw;
    }

    let activity = if state.is_active(index) {
        "*"
    } else {
        ""
    };
    raw.push_str(&format!("Status: {status_label} {activity}\n"));
    if let Some(age) = last_update_age(pane) {
        raw.push_str(&format!("Updated: {age} ago\n"));
    }

    if let Some(capture) = &pane.last_capture {
        if !capture.command.is_empty() {
            raw.push_str(&format!("Cmd: {}\n", capture.command));
        } else if !capture.title.is_empty() {
            raw.push_str(&format!("Title: {}\n", capture.title));
        }
        if let Some(label) = &pane.tracked.label {
            raw.push_str(&format!("Label: {label}\n"));
        }
        raw.push('\n');
        raw.push_str(&capture.lines.join("\n"));
    } else {
        raw.push_str("Waiting for data...");
    }

    if pane.status == PaneStatus::Down {
        if let Some(err) = &pane.error {
            raw.push('\n');
            raw.push_str(&format!("Error: {err}"));
        }
    }

    raw
}

fn last_update_age(pane: &crate::model::PaneState) -> Option<String> {
    pane.last_update.map(|instant| format_duration(instant.elapsed()))
}

fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 3600 {
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        format!("{hours}h {minutes}m")
    } else if secs >= 60 {
        let minutes = secs / 60;
        let seconds = secs % 60;
        format!("{minutes}m {seconds}s")
    } else if secs >= 1 {
        format!("{secs}s")
    } else {
        "0s".to_string()
    }
}

fn grid_layout(area: Rect, count: usize) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }

    let cols = (count as f64).sqrt().ceil() as usize;
    let rows = (count + cols - 1) / cols;

    let mut row_constraints = Vec::new();
    for _ in 0..rows {
        row_constraints.push(Constraint::Ratio(1, rows as u32));
    }

    let row_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    let mut tiles = Vec::new();
    for row in row_chunks.iter() {
        let mut col_constraints = Vec::new();
        for _ in 0..cols {
            col_constraints.push(Constraint::Ratio(1, cols as u32));
        }
        let cols_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row);
        tiles.extend(cols_chunks.iter().copied());
    }

    tiles.into_iter().take(count).collect()
}

fn draw_help(f: &mut Frame, area: Rect) {
    let help = vec![
        Line::from("Keys:"),
        Line::from("  q   Quit"),
        Line::from("  h/j/k/l or arrows   Move focus"),
        Line::from("  Tab   Next tile"),
        Line::from("  Enter   Take control"),
        Line::from("  r   Reload config"),
        Line::from("  e   Edit config"),
        Line::from("  n   Set pane label"),
        Line::from("  s   Setup"),
        Line::from("  c   Toggle compact mode"),
        Line::from("  z   Zoom focused tile"),
        Line::from("  ?   Toggle help"),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Help")
        .border_style(Style::default().fg(Color::White));
    let paragraph = Paragraph::new(help).block(block).wrap(Wrap { trim: true });

    let popup_area = centered_rect(60, 60, area);
    f.render_widget(paragraph, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
