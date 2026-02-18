use crate::config::{Config, HostConfig, TrackedPane};
use crate::ssh::HostResolver;
use crate::tmux;
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use std::collections::{BTreeMap, HashMap, HashSet};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

const MAX_PANES: usize = 10;
const ACCENT: Color = Color::Cyan;
const ACCENT_DIM: Color = Color::DarkGray;
const ERROR: Color = Color::Red;
const WARN: Color = Color::Yellow;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct PaneKey {
    host: String,
    session: String,
    window: u32,
    pane_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct WindowKey {
    session: String,
    window: u32,
}

#[derive(Debug, Clone)]
struct PaneSelection {
    session: String,
    window: u32,
    pane_id: String,
    command: String,
    title: String,
}

#[derive(Debug, Clone)]
struct TreeItem {
    session: String,
    window: Option<u32>,
    label: String,
}

#[derive(Debug, Clone)]
struct HostData {
    loading: bool,
    error: Option<String>,
    tree: Vec<TreeItem>,
    panes_by_window: HashMap<WindowKey, Vec<PaneSelection>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Hosts,
    Tree,
    Panes,
}

#[derive(Debug)]
struct HostForm {
    mode: FormMode,
    name: String,
    targets: String,
    color: String,
    field: FormField,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormMode {
    Add,
    Edit(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormField {
    Name,
    Targets,
    Color,
}

#[derive(Debug)]
enum Modal {
    HostForm(HostForm),
    ConfirmDelete(usize),
}

#[derive(Debug)]
enum SetupMsg {
    HostLoaded { host: String, data: HostData },
    HostError { host: String, error: String },
    PreviewLoaded { key: PaneKey, output: String },
    PreviewError { key: PaneKey, error: String },
}

pub enum SetupAction {
    None,
    Save { config: Config, tracked: Vec<TrackedPane> },
    Cancel,
}

pub struct SetupState {
    pub config: Config,
    focus: Focus,
    host_index: usize,
    tree_index: usize,
    pane_index: usize,
    selection: HashSet<PaneKey>,
    bookmarks: HashSet<PaneKey>,
    host_data: HashMap<String, HostData>,
    modal: Option<Modal>,
    status: Option<String>,
    preview: Option<String>,
    preview_key: Option<PaneKey>,
    preview_loading: bool,
    msg_tx: UnboundedSender<SetupMsg>,
    msg_rx: UnboundedReceiver<SetupMsg>,
}

impl SetupState {
    pub fn new(config: Config) -> Self {
        let (msg_tx, msg_rx) = tokio::sync::mpsc::unbounded_channel();
        let selection = config
            .tracked
            .iter()
            .map(|pane| PaneKey {
                host: pane.host.clone(),
                session: pane.session.clone(),
                window: pane.window,
                pane_id: pane.pane_id.clone(),
            })
            .collect();
        let bookmarks = config
            .bookmarks
            .iter()
            .map(|pane| PaneKey {
                host: pane.host.clone(),
                session: pane.session.clone(),
                window: pane.window,
                pane_id: pane.pane_id.clone(),
            })
            .collect();
        let mut state = Self {
            config,
            focus: Focus::Hosts,
            host_index: 0,
            tree_index: 0,
            pane_index: 0,
            selection,
            bookmarks,
            host_data: HashMap::new(),
            modal: None,
            status: None,
            preview: None,
            preview_key: None,
            preview_loading: false,
            msg_tx,
            msg_rx,
        };
        state.ensure_host_loaded();
        state
    }

    pub fn handle_messages(&mut self) {
        while let Ok(msg) = self.msg_rx.try_recv() {
            match msg {
                SetupMsg::HostLoaded { host, data } => {
                    self.host_data.insert(host, data);
                }
                SetupMsg::HostError { host, error } => {
                    self.host_data.insert(
                        host,
                        HostData {
                            loading: false,
                            error: Some(error),
                            tree: Vec::new(),
                            panes_by_window: HashMap::new(),
                        },
                    );
                }
                SetupMsg::PreviewLoaded { key, output } => {
                    if self.preview_key.as_ref() == Some(&key) {
                        self.preview = Some(output);
                        self.preview_loading = false;
                    }
                }
                SetupMsg::PreviewError { key, error } => {
                    if self.preview_key.as_ref() == Some(&key) {
                        self.preview = Some(format!("Preview error: {error}"));
                        self.preview_loading = false;
                    }
                }
            }
        }
    }

    pub fn set_status(&mut self, message: &str) {
        self.status = Some(message.to_string());
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(2)])
            .split(area);

        let body = chunks[0];
        let footer = chunks[1];

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(40),
            ])
            .split(body);

        self.draw_hosts(f, columns[0]);
        self.draw_tree(f, columns[1]);
        self.draw_panes(f, columns[2]);
        self.draw_footer(f, footer);

        if let Some(modal) = &self.modal {
            self.draw_modal(f, modal, body);
        }
    }

    pub fn handle_event(&mut self, event: Event) -> Result<SetupAction> {
        if self.modal.is_some() {
            return self.handle_modal_event(event);
        }

        let Event::Key(key) = event else { return Ok(SetupAction::None); };
        if key.kind != KeyEventKind::Press {
            return Ok(SetupAction::None);
        }

        match key.code {
            KeyCode::Char('q') => return Ok(SetupAction::Cancel),
            KeyCode::Char('s') => return self.save_selection(),
            KeyCode::Tab => self.cycle_focus(),
            KeyCode::Left | KeyCode::Char('h') => self.focus_left(),
            KeyCode::Right | KeyCode::Char('l') => self.focus_right(),
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Char(' ') => self.toggle_pane_selection(),
            KeyCode::Char('m') => self.toggle_bookmark(),
            KeyCode::Char('a') => self.open_add_host(),
            KeyCode::Char('e') => self.open_edit_host(),
            KeyCode::Char('d') => self.confirm_delete_host(),
            _ => {}
        }

        Ok(SetupAction::None)
    }

    fn draw_hosts(&mut self, f: &mut Frame, area: Rect) {
        let mut list_state = ListState::default();
        if !self.config.hosts.is_empty() {
            list_state.select(Some(self.host_index.min(self.config.hosts.len() - 1)));
        }

        let items: Vec<ListItem> = self
            .config
            .hosts
            .iter()
            .map(|host| {
                let mut label = host.name.clone();
                if crate::ssh::is_local_target(host.targets.get(0).map(|s| s.as_str()).unwrap_or(""))
                {
                    label.push_str(" (local)");
                }
                ListItem::new(label)
            })
            .collect();

        let title = match self.focus {
            Focus::Hosts => "Hosts",
            _ => "Hosts",
        };

        let list = List::new(items)
            .block(panel_block(title, self.focus == Focus::Hosts))
            .highlight_style(highlight_style(self.focus == Focus::Hosts))
            .highlight_symbol("▸ ");

        f.render_stateful_widget(list, area, &mut list_state);
    }

    fn draw_tree(&mut self, f: &mut Frame, area: Rect) {
        let title = match self.focus {
            Focus::Tree => "Sessions / Windows",
            _ => "Sessions / Windows",
        };

        let block = panel_block(title, self.focus == Focus::Tree);

        let Some(host) = self.current_host() else {
            f.render_widget(block, area);
            return;
        };

        if let Some(data) = self.host_data.get(host) {
            if data.loading {
                let paragraph = Paragraph::new("Loading panes...").block(block);
                f.render_widget(paragraph, area);
                return;
            }
            if let Some(error) = &data.error {
                let paragraph = Paragraph::new(format!("Error: {error}")).block(block);
                f.render_widget(paragraph, area);
                return;
            }

            let items: Vec<ListItem> = data
                .tree
                .iter()
                .map(|item| {
                    let indent = if item.window.is_some() { "  " } else { "" };
                    let line = format!("{indent}{}", item.label);
                    ListItem::new(line)
                })
                .collect();

            let mut state = ListState::default();
            if !data.tree.is_empty() {
                state.select(Some(self.tree_index.min(data.tree.len() - 1)));
            }

            let list = List::new(items)
                .block(block)
                .highlight_style(highlight_style(self.focus == Focus::Tree))
                .highlight_symbol("▸ ");
            f.render_stateful_widget(list, area, &mut state);
        } else {
            let paragraph = Paragraph::new("Loading panes...").block(block);
            f.render_widget(paragraph, area);
        }
    }

    fn draw_panes(&mut self, f: &mut Frame, area: Rect) {
        let title = match self.focus {
            Focus::Panes => "Panes",
            _ => "Panes",
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        let pane_list_area = chunks[0];
        let preview_area = chunks[1];

        let block = panel_block(
            &format!("{title} ({}/{})", self.selection.len(), MAX_PANES),
            self.focus == Focus::Panes,
        );

        let Some((host, window_key)) = self.current_window() else {
            let paragraph = Paragraph::new("Select a window to see panes.").block(block);
            f.render_widget(paragraph, pane_list_area);
            self.draw_preview(f, preview_area);
            return;
        };

        let panes = self.panes_for_window(host, &window_key);
        let items: Vec<ListItem> = panes
            .iter()
            .map(|pane| {
                let key = PaneKey {
                    host: host.to_string(),
                    session: pane.session.clone(),
                    window: pane.window,
                    pane_id: pane.pane_id.clone(),
                };
                let selected = self.selection.contains(&key);
                let bookmarked = self.bookmarks.contains(&key);
                let marker = match (selected, bookmarked) {
                    (true, true) => "[TB]",
                    (true, false) => "[T]",
                    (false, true) => "[B]",
                    (false, false) => "[ ]",
                };
                let label = if pane.title.is_empty() {
                    pane.command.clone()
                } else {
                    format!("{} — {}", pane.command, pane.title)
                };
                ListItem::new(format!("{marker} {}  {}", pane.pane_id, label))
            })
            .collect();

        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(self.pane_index.min(items.len() - 1)));
        }

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style(self.focus == Focus::Panes))
            .highlight_symbol("▸ ");
        f.render_stateful_widget(list, pane_list_area, &mut state);

        self.draw_preview(f, preview_area);
    }

    fn draw_preview(&self, f: &mut Frame, area: Rect) {
        let title = if self.preview_loading {
            "Preview (loading...)"
        } else {
            "Preview"
        };
        let block = panel_block(title, false);
        let body = match &self.preview {
            Some(text) => Text::from(text.clone()),
            None => Text::from("Select a pane to preview."),
        };
        let paragraph = Paragraph::new(body).block(block).wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        let mut spans = Vec::new();
        spans.extend(hint("Tab", "focus"));
        spans.extend(hint("Arrows", "navigate"));
        spans.extend(hint("Space", "toggle"));
        spans.extend(hint("m", "bookmark"));
        spans.extend(hint("a", "add"));
        spans.extend(hint("e", "edit"));
        spans.extend(hint("d", "delete"));
        spans.extend(hint("s", "save"));
        spans.extend(hint("q", "cancel"));
        if let Some(status) = &self.status {
            spans.push(Span::raw("  |  "));
            spans.push(Span::styled(status.clone(), Style::default().fg(WARN)));
        }
        let paragraph = Paragraph::new(Line::from(spans)).block(Block::default());
        f.render_widget(paragraph, area);
    }

    fn draw_modal(&self, f: &mut Frame, modal: &Modal, area: Rect) {
        let popup = centered_rect(60, 50, area);
        f.render_widget(Clear, popup);
        match modal {
            Modal::HostForm(form) => self.draw_host_form(f, form, popup),
            Modal::ConfirmDelete(index) => self.draw_confirm_delete(f, *index, popup),
        }
    }

    fn draw_host_form(&self, f: &mut Frame, form: &HostForm, area: Rect) {
        let title = match form.mode {
            FormMode::Add => "Add Host",
            FormMode::Edit(_) => "Edit Host",
        };
        let block = panel_block(title, true);

        let mut lines = Vec::new();
        lines.push(input_line("Name", &form.name, form.field == FormField::Name));
        lines.push(input_line(
            "Targets",
            &form.targets,
            form.field == FormField::Targets,
        ));
        lines.push(input_line(
            "Color (optional)",
            &form.color,
            form.field == FormField::Color,
        ));
        lines.push(Line::from(""));
        lines.push(Line::from("Enter: save  Esc: cancel  Tab: next"));
        if let Some(error) = &form.error {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("Error: {error}"),
                Style::default().fg(ERROR).add_modifier(Modifier::BOLD),
            )));
        }

        let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }

    fn draw_confirm_delete(&self, f: &mut Frame, index: usize, area: Rect) {
        let name = self
            .config
            .hosts
            .get(index)
            .map(|host| host.name.as_str())
            .unwrap_or("host");
        let block = panel_block("Delete Host", true);
        let lines = vec![
            Line::from(format!("Delete host '{name}'?")),
            Line::from(""),
            Line::from("y: confirm   n/esc: cancel"),
        ];
        let paragraph = Paragraph::new(lines).block(block);
        f.render_widget(paragraph, area);
    }

    fn handle_modal_event(&mut self, event: Event) -> Result<SetupAction> {
        let Event::Key(key) = event else { return Ok(SetupAction::None); };
        if key.kind != KeyEventKind::Press {
            return Ok(SetupAction::None);
        }

        let mut modal = match self.modal.take() {
            Some(modal) => modal,
            None => return Ok(SetupAction::None),
        };

        let keep_open = match &mut modal {
            Modal::HostForm(form) => self.handle_host_form_event(form, key)?,
            Modal::ConfirmDelete(index) => self.handle_confirm_delete_event(*index, key)?,
        };

        if keep_open {
            self.modal = Some(modal);
        }

        Ok(SetupAction::None)
    }

    fn handle_host_form_event(&mut self, form: &mut HostForm, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => return Ok(false),
            KeyCode::Tab | KeyCode::Down => {
                form.field = match form.field {
                    FormField::Name => FormField::Targets,
                    FormField::Targets => FormField::Color,
                    FormField::Color => FormField::Name,
                };
            }
            KeyCode::Up => {
                form.field = match form.field {
                    FormField::Name => FormField::Color,
                    FormField::Targets => FormField::Name,
                    FormField::Color => FormField::Targets,
                };
            }
            KeyCode::Enter => {
                if let Some(host) = self.build_host_from_form(form) {
                    match form.mode {
                        FormMode::Add => self.config.hosts.push(host),
                        FormMode::Edit(index) => {
                            if let Some(existing) = self.config.hosts.get_mut(index) {
                                *existing = host;
                            }
                        }
                    }
                    self.ensure_host_loaded();
                    return Ok(false);
                }
            }
            KeyCode::Backspace => {
                let target = match form.field {
                    FormField::Name => &mut form.name,
                    FormField::Targets => &mut form.targets,
                    FormField::Color => &mut form.color,
                };
                target.pop();
            }
            KeyCode::Char(ch) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(true);
                }
                let target = match form.field {
                    FormField::Name => &mut form.name,
                    FormField::Targets => &mut form.targets,
                    FormField::Color => &mut form.color,
                };
                target.push(ch);
            }
            _ => {}
        }

        Ok(true)
    }

    fn handle_confirm_delete_event(&mut self, index: usize, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') => {
                self.config.hosts.remove(index);
                if self.host_index >= self.config.hosts.len() {
                    self.host_index = self.config.hosts.len().saturating_sub(1);
                }
                self.ensure_host_loaded();
                Ok(false)
            }
            KeyCode::Char('n') | KeyCode::Esc => Ok(false),
            _ => Ok(true),
        }
    }

    fn build_host_from_form(&mut self, form: &mut HostForm) -> Option<HostConfig> {
        form.error = None;
        let name = form.name.trim().to_string();
        if name.is_empty() {
            form.error = Some("Name is required".to_string());
            return None;
        }
        let targets: Vec<String> = form
            .targets
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if targets.is_empty() {
            form.error = Some("At least one target is required".to_string());
            return None;
        }
        let color = if form.color.trim().is_empty() {
            None
        } else {
            Some(form.color.trim().to_string())
        };

        Some(HostConfig {
            name,
            targets,
            strategy: Some("auto".to_string()),
            color,
            tags: None,
        })
    }

    fn open_add_host(&mut self) {
        self.modal = Some(Modal::HostForm(HostForm {
            mode: FormMode::Add,
            name: String::new(),
            targets: String::new(),
            color: String::new(),
            field: FormField::Name,
            error: None,
        }));
    }

    fn open_edit_host(&mut self) {
        let Some(host) = self.config.hosts.get(self.host_index) else { return; };
        if is_local_host(host) {
            self.status = Some("Local host is managed via [local] config.".to_string());
            return;
        }
        let color = host.color.clone().unwrap_or_default();
        self.modal = Some(Modal::HostForm(HostForm {
            mode: FormMode::Edit(self.host_index),
            name: host.name.clone(),
            targets: host.targets.join(", "),
            color,
            field: FormField::Name,
            error: None,
        }));
    }

    fn confirm_delete_host(&mut self) {
        let Some(host) = self.config.hosts.get(self.host_index) else { return; };
        if is_local_host(host) {
            self.status = Some("Local host cannot be deleted.".to_string());
            return;
        }
        self.modal = Some(Modal::ConfirmDelete(self.host_index));
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Hosts => Focus::Tree,
            Focus::Tree => Focus::Panes,
            Focus::Panes => Focus::Hosts,
        };
    }

    fn focus_left(&mut self) {
        self.focus = match self.focus {
            Focus::Hosts => Focus::Hosts,
            Focus::Tree => Focus::Hosts,
            Focus::Panes => Focus::Tree,
        };
    }

    fn focus_right(&mut self) {
        self.focus = match self.focus {
            Focus::Hosts => Focus::Tree,
            Focus::Tree => Focus::Panes,
            Focus::Panes => Focus::Panes,
        };
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Hosts => {
                if self.host_index > 0 {
                    self.host_index -= 1;
                    self.tree_index = 0;
                    self.pane_index = 0;
                    self.ensure_host_loaded();
                }
            }
            Focus::Tree => {
                if let Some(host) = self.current_host().cloned() {
                    let tree_len = self
                        .host_data
                        .get(&host)
                        .map(|data| data.tree.len())
                        .unwrap_or(0);
                    if self.tree_index > 0 {
                        self.tree_index -= 1;
                    }
                    if self.tree_index >= tree_len {
                        self.tree_index = tree_len.saturating_sub(1);
                    }
                    if let Some((host, window)) =
                        self.current_window().map(|(h, w)| (h.to_string(), w))
                    {
                        self.set_preview_for_window(&host, window);
                    }
                }
            }
            Focus::Panes => {
                if self.pane_index > 0 {
                    self.pane_index -= 1;
                    self.request_preview();
                }
            }
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Hosts => {
                if self.host_index + 1 < self.config.hosts.len() {
                    self.host_index += 1;
                    self.tree_index = 0;
                    self.pane_index = 0;
                    self.ensure_host_loaded();
                }
            }
            Focus::Tree => {
                if let Some(host) = self.current_host().cloned() {
                    let tree_len = self
                        .host_data
                        .get(&host)
                        .map(|data| data.tree.len())
                        .unwrap_or(0);
                    if self.tree_index + 1 < tree_len {
                        self.tree_index += 1;
                    }
                    if let Some((host, window)) =
                        self.current_window().map(|(h, w)| (h.to_string(), w))
                    {
                        self.set_preview_for_window(&host, window);
                    }
                }
            }
            Focus::Panes => {
                if let Some((host, window)) = self.current_window() {
                    let panes = self.panes_for_window(host, &window);
                    if self.pane_index + 1 < panes.len() {
                        self.pane_index += 1;
                        self.request_preview();
                    }
                }
            }
        }
    }

    fn handle_enter(&mut self) {
        match self.focus {
            Focus::Hosts => {
                self.focus = Focus::Tree;
            }
            Focus::Tree => {
                if self.current_window().is_some() {
                    self.focus = Focus::Panes;
                    self.request_preview();
                }
            }
            Focus::Panes => {}
        }
    }

    fn toggle_pane_selection(&mut self) {
        if self.focus != Focus::Panes {
            return;
        }
        let Some((host, window)) = self.current_window() else { return; };
        let panes = self.panes_for_window(host, &window);
        let Some(pane) = panes.get(self.pane_index) else { return; };
        let key = PaneKey {
            host: host.to_string(),
            session: pane.session.clone(),
            window: pane.window,
            pane_id: pane.pane_id.clone(),
        };
        if self.selection.contains(&key) {
            self.selection.remove(&key);
        } else {
            if self.selection.len() >= MAX_PANES {
                self.status = Some(format!("Limit is {MAX_PANES} panes."));
                return;
            }
            self.selection.insert(key);
        }
    }

    fn toggle_bookmark(&mut self) {
        if self.focus != Focus::Panes {
            return;
        }
        let Some((host, window)) = self.current_window() else { return; };
        let panes = self.panes_for_window(host, &window);
        let Some(pane) = panes.get(self.pane_index) else { return; };
        let key = PaneKey {
            host: host.to_string(),
            session: pane.session.clone(),
            window: pane.window,
            pane_id: pane.pane_id.clone(),
        };
        if self.bookmarks.contains(&key) {
            self.bookmarks.remove(&key);
        } else {
            self.bookmarks.insert(key);
        }
    }

    fn save_selection(&mut self) -> Result<SetupAction> {
        if self.selection.is_empty() {
            self.status = Some("Select at least one pane.".to_string());
            return Ok(SetupAction::None);
        }
        if self.selection.len() > MAX_PANES {
            self.status = Some(format!("Limit is {MAX_PANES} panes."));
            return Ok(SetupAction::None);
        }
        let tracked: Vec<TrackedPane> = self
            .selection
            .iter()
            .map(|key| TrackedPane {
                host: key.host.clone(),
                session: key.session.clone(),
                window: key.window,
                pane_id: key.pane_id.clone(),
                label: self.find_label_for(key),
            })
            .collect();
        let bookmarks: Vec<TrackedPane> = self
            .bookmarks
            .iter()
            .map(|key| TrackedPane {
                host: key.host.clone(),
                session: key.session.clone(),
                window: key.window,
                pane_id: key.pane_id.clone(),
                label: self.find_label_for(key),
            })
            .collect();
        self.config.bookmarks = bookmarks;
        Ok(SetupAction::Save {
            config: self.config.clone(),
            tracked,
        })
    }

    fn find_label_for(&self, key: &PaneKey) -> Option<String> {
        for pane in &self.config.tracked {
            if pane.host == key.host
                && pane.session == key.session
                && pane.window == key.window
                && pane.pane_id == key.pane_id
            {
                return pane.label.clone();
            }
        }
        for pane in &self.config.bookmarks {
            if pane.host == key.host
                && pane.session == key.session
                && pane.window == key.window
                && pane.pane_id == key.pane_id
            {
                return pane.label.clone();
            }
        }
        None
    }

    fn ensure_host_loaded(&mut self) {
        let Some(host_name) = self.current_host().cloned() else { return; };
        if let Some(data) = self.host_data.get(&host_name) {
            if data.loading {
                return;
            }
            if data.error.is_none() && !data.tree.is_empty() {
                return;
            }
        }
        let host_cfg = match self.config.hosts.iter().find(|h| h.name == host_name) {
            Some(host) => host.clone(),
            None => return,
        };
        self.host_data.insert(
            host_name.clone(),
            HostData {
                loading: true,
                error: None,
                tree: Vec::new(),
                panes_by_window: HashMap::new(),
            },
        );
        let ssh_cfg = self.config.ssh.clone();
        let tx = self.msg_tx.clone();
        let host_for_error = host_name.clone();
        tokio::spawn(async move {
            match load_host_data(host_cfg, ssh_cfg).await {
                Ok(data) => {
                    let _ = tx.send(SetupMsg::HostLoaded { host: data.0, data: data.1 });
                }
                Err(err) => {
                    let _ = tx.send(SetupMsg::HostError {
                        host: host_for_error,
                        error: err.to_string(),
                    });
                }
            }
        });
    }

    fn current_host(&self) -> Option<&String> {
        self.config.hosts.get(self.host_index).map(|h| &h.name)
    }

    fn current_window(&self) -> Option<(&str, WindowKey)> {
        let host = self.current_host()?;
        let data = self.host_data.get(host)?;
        let item = data.tree.get(self.tree_index)?;
        let window = item.window?;
        Some((host.as_str(), WindowKey { session: item.session.clone(), window }))
    }

    fn panes_for_window(&self, host: &str, window: &WindowKey) -> Vec<PaneSelection> {
        let Some(data) = self.host_data.get(host) else { return Vec::new(); };
        data.panes_by_window
            .get(window)
            .cloned()
            .unwrap_or_default()
    }

    fn request_preview(&mut self) {
        let Some((host, window)) = self
            .current_window()
            .map(|(host, window)| (host.to_string(), window))
        else {
            return;
        };
        let panes = self.panes_for_window(&host, &window);
        let Some(pane) = panes.get(self.pane_index) else { return; };
        let key = PaneKey {
            host: host.clone(),
            session: pane.session.clone(),
            window: pane.window,
            pane_id: pane.pane_id.clone(),
        };
        if self.preview_key.as_ref() == Some(&key) {
            return;
        }
        self.preview_key = Some(key.clone());
        self.preview_loading = true;
        self.preview = None;

        let host_cfg = match self.config.hosts.iter().find(|h| h.name == host) {
            Some(host) => host.clone(),
            None => return,
        };
        let ssh_cfg = self.config.ssh.clone();
        let lines = self.config.ui.lines.min(20);
        let join_lines = self.config.ui.join_lines;
        let ansi = self.config.ui.ansi;
        let tx = self.msg_tx.clone();
        tokio::spawn(async move {
            let mut resolver = HostResolver::new();
            let target = resolver.resolve_target(&host_cfg, &ssh_cfg).await;
            match target {
                Ok(target) => match tmux::capture_pane(&target, &key.pane_id, lines, join_lines, ansi, &ssh_cfg).await {
                    Ok(capture) => {
                        let output = capture.lines.join("\n");
                        let _ = tx.send(SetupMsg::PreviewLoaded { key, output });
                    }
                    Err(err) => {
                        let _ = tx.send(SetupMsg::PreviewError { key, error: err.to_string() });
                    }
                },
                Err(err) => {
                    let _ = tx.send(SetupMsg::PreviewError { key, error: err.to_string() });
                }
            }
        });
    }

    fn set_preview_for_window(&mut self, host: &str, window: WindowKey) {
        let panes = self.panes_for_window(host, &window);
        if self.pane_index >= panes.len() {
            self.pane_index = panes.len().saturating_sub(1);
        }
        self.request_preview();
    }
}

async fn load_host_data(host: HostConfig, ssh_cfg: crate::config::SshConfig) -> Result<(String, HostData)> {
    let mut resolver = HostResolver::new();
    let target = resolver.resolve_target(&host, &ssh_cfg).await?;
    let windows = tmux::list_windows(&target, &ssh_cfg).await.unwrap_or_default();
    let panes = tmux::list_panes(&target, &ssh_cfg).await?;

    let mut window_names: HashMap<(String, u32), String> = HashMap::new();
    for window in windows {
        window_names.insert((window.session.clone(), window.window), window.name.clone());
    }

    let mut panes_by_window: HashMap<WindowKey, Vec<PaneSelection>> = HashMap::new();
    for pane in panes {
        panes_by_window
            .entry(WindowKey { session: pane.session.clone(), window: pane.window })
            .or_default()
            .push(PaneSelection {
                session: pane.session,
                window: pane.window,
                pane_id: pane.pane_id,
                command: pane.command,
                title: pane.title,
            });
    }

    let mut tree = Vec::new();
    let mut grouped: BTreeMap<String, BTreeMap<u32, usize>> = BTreeMap::new();
    for key in panes_by_window.keys() {
        grouped
            .entry(key.session.clone())
            .or_default()
            .insert(key.window, panes_by_window.get(key).map(|v| v.len()).unwrap_or(0));
    }

    for (session, windows) in grouped {
        tree.push(TreeItem {
            session: session.clone(),
            window: None,
            label: format!("{session}"),
        });
        for (window, count) in windows {
            let name = window_names
                .get(&(session.clone(), window))
                .cloned()
                .unwrap_or_default();
            let label = if name.is_empty() {
                format!("{}:{} ({} panes)", session, window, count)
            } else {
                format!("{}:{} {} ({} panes)", session, window, name, count)
            };
            tree.push(TreeItem {
                session: session.clone(),
                window: Some(window),
                label,
            });
        }
    }

    Ok((
        host.name.clone(),
        HostData {
            loading: false,
            error: None,
            tree,
            panes_by_window,
        },
    ))
}

fn is_local_host(host: &HostConfig) -> bool {
    host.targets.len() == 1
        && crate::ssh::is_local_target(host.targets.first().map(|s| s.as_str()).unwrap_or(""))
}

fn input_line(label: &str, value: &str, active: bool) -> Line<'static> {
    let prefix = if active { ">" } else { " " };
    let label_style = if active {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACCENT_DIM)
    };
    Line::from(vec![
        Span::styled(format!("{prefix} {label}: "), label_style),
        Span::styled(
            value.to_string(),
            if active {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ),
    ])
}

fn panel_block(title: &str, focused: bool) -> Block<'static> {
    let color = if focused { ACCENT } else { ACCENT_DIM };
    let title = Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(color)
            .add_modifier(if focused { Modifier::BOLD } else { Modifier::empty() }),
    ));
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(title)
}

fn highlight_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Black)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

fn hint(key: &str, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            key.to_string(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {label}  ")),
    ]
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
