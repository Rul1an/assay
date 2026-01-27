use anyhow::{Context, Result};
use assay_evidence::sanitize::sanitize_terminal_with_limit;
use assay_evidence::types::EvidenceEvent;
use clap::Args;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;
use std::io::stdout;

const DEFAULT_MAX_EVENTS: usize = 50_000;
const SUBJECT_MAX_LEN: usize = 200;
const PAYLOAD_MAX_LEN: usize = 4096;

#[derive(Debug, Args, Clone)]
pub struct ExploreArgs {
    /// Bundle to explore
    #[arg(value_name = "BUNDLE")]
    pub bundle: std::path::PathBuf,

    /// Skip bundle verification before opening
    #[arg(long)]
    pub no_verify: bool,

    /// Maximum number of events to load
    #[arg(long, default_value_t = DEFAULT_MAX_EVENTS)]
    pub max_events: usize,
}

struct AppState {
    events: Vec<EvidenceEvent>,
    list_state: ListState,
    search_query: String,
    filter_type: String,
    mode: AppMode,
    run_id: String,
    event_count: usize,
    verified: bool,
}

#[derive(PartialEq)]
enum AppMode {
    Normal,
    Search,
    Filter,
}

impl AppState {
    fn selected_event(&self) -> Option<&EvidenceEvent> {
        self.list_state
            .selected()
            .and_then(|i| self.visible_events().get(i).copied())
    }

    fn visible_events(&self) -> Vec<&EvidenceEvent> {
        self.events
            .iter()
            .filter(|e| {
                if !self.search_query.is_empty() {
                    let subject = e.subject.as_deref().unwrap_or("");
                    let sanitized = sanitize_terminal_with_limit(subject, SUBJECT_MAX_LEN);
                    if !sanitized
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase())
                    {
                        return false;
                    }
                }
                if !self.filter_type.is_empty() && !e.type_.contains(&self.filter_type) {
                    return false;
                }
                true
            })
            .collect()
    }

    fn move_selection(&mut self, delta: i64) {
        let len = self.visible_events().len();
        if len == 0 {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0) as i64;
        let next = (current + delta).clamp(0, len as i64 - 1) as usize;
        self.list_state.select(Some(next));
    }
}

pub fn cmd_explore(args: ExploreArgs) -> Result<i32> {
    let f = std::fs::File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    let br = if args.no_verify {
        assay_evidence::bundle::BundleReader::open_unverified(f)
    } else {
        assay_evidence::bundle::BundleReader::open(f)
    }
    .context("failed to open bundle")?;

    let run_id = br.run_id().to_string();
    let event_count = br.event_count();
    let verified = !args.no_verify;

    let mut events: Vec<EvidenceEvent> = Vec::new();
    for (i, ev_res) in br.events().enumerate() {
        if i >= args.max_events {
            eprintln!(
                "Warning: truncated at {} events (--max-events={})",
                args.max_events, args.max_events
            );
            break;
        }
        events.push(ev_res.context("reading event")?);
    }

    let mut state = AppState {
        events,
        list_state: ListState::default(),
        search_query: String::new(),
        filter_type: String::new(),
        mode: AppMode::Normal,
        run_id,
        event_count,
        verified,
    };
    state.list_state.select(Some(0));

    run_tui(&mut state)?;
    Ok(0)
}

fn run_tui(state: &mut AppState) -> Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| draw_ui(f, state))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match state.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break
                        }
                        KeyCode::Char('j') | KeyCode::Down => state.move_selection(1),
                        KeyCode::Char('k') | KeyCode::Up => state.move_selection(-1),
                        KeyCode::PageDown => state.move_selection(20),
                        KeyCode::PageUp => state.move_selection(-20),
                        KeyCode::Home => state.list_state.select(Some(0)),
                        KeyCode::End => {
                            let len = state.visible_events().len();
                            if len > 0 {
                                state.list_state.select(Some(len - 1));
                            }
                        }
                        KeyCode::Char('/') => {
                            state.mode = AppMode::Search;
                            state.search_query.clear();
                        }
                        KeyCode::Char('f') => {
                            state.mode = AppMode::Filter;
                            state.filter_type.clear();
                        }
                        KeyCode::Esc => {
                            state.search_query.clear();
                            state.filter_type.clear();
                        }
                        _ => {}
                    },
                    AppMode::Search => match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            state.mode = AppMode::Normal;
                            state.list_state.select(Some(0));
                        }
                        KeyCode::Backspace => {
                            state.search_query.pop();
                        }
                        KeyCode::Char(c) => {
                            state.search_query.push(c);
                        }
                        _ => {}
                    },
                    AppMode::Filter => match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            state.mode = AppMode::Normal;
                            state.list_state.select(Some(0));
                        }
                        KeyCode::Backspace => {
                            state.filter_type.pop();
                        }
                        KeyCode::Char(c) => {
                            state.filter_type.push(c);
                        }
                        _ => {}
                    },
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn draw_ui(f: &mut ratatui::Frame<'_>, state: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main area
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    // Header
    let header_text = format!(
        " Assay Evidence Explorer | Run: {} | Events: {} | Verified: {} ",
        sanitize_terminal_with_limit(&state.run_id, 40),
        state.event_count,
        if state.verified { "YES" } else { "NO" }
    );
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::White).bg(Color::DarkGray))
        .block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(header, chunks[0]);

    // Main area: split into event list (left) and detail (right)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Event list (left pane)
    let visible = state.visible_events();
    let items: Vec<ListItem<'_>> = visible
        .iter()
        .map(|e| {
            let subject = e.subject.as_deref().unwrap_or("-");
            let sanitized_subject = sanitize_terminal_with_limit(subject, 50);
            let time_short = e.time.format("%H:%M:%S").to_string();
            let line = format!(
                "{:>4} {} {:<25} {}",
                e.seq,
                time_short,
                truncate_str(&e.type_, 25),
                sanitized_subject
            );
            ListItem::new(Line::from(Span::raw(line)))
        })
        .collect();

    let list_title = if !state.search_query.is_empty() {
        format!(" Events (search: {}) ", state.search_query)
    } else if !state.filter_type.is_empty() {
        format!(" Events (filter: {}) ", state.filter_type)
    } else {
        format!(" Events ({}) ", visible.len())
    };

    let list = List::new(items)
        .block(Block::default().title(list_title).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    f.render_stateful_widget(list, main_chunks[0], &mut state.list_state);

    // Detail view (right pane)
    let detail_text = if let Some(event) = state.selected_event() {
        let subject = event.subject.as_deref().unwrap_or("-");
        let payload_str = serde_json::to_string_pretty(&event.payload).unwrap_or_default();
        let sanitized_payload = sanitize_terminal_with_limit(&payload_str, PAYLOAD_MAX_LEN);
        let sanitized_subject = sanitize_terminal_with_limit(subject, SUBJECT_MAX_LEN);

        format!(
            "Type:        {}\n\
             ID:          {}\n\
             Seq:         {}\n\
             Time:        {}\n\
             Subject:     {}\n\
             Traceparent: {}\n\
             PII:         {}\n\
             Secrets:     {}\n\
             \n\
             Payload:\n{}",
            sanitize_terminal_with_limit(&event.type_, 100),
            sanitize_terminal_with_limit(&event.id, 100),
            event.seq,
            event.time.to_rfc3339(),
            sanitized_subject,
            event
                .trace_parent
                .as_deref()
                .map(|t| sanitize_terminal_with_limit(t, 100))
                .unwrap_or_else(|| "-".into()),
            event.contains_pii,
            event.contains_secrets,
            sanitized_payload,
        )
    } else {
        "No event selected".into()
    };

    let detail = Paragraph::new(detail_text)
        .block(Block::default().title(" Detail ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(detail, main_chunks[1]);

    // Status bar
    let status = match state.mode {
        AppMode::Normal => {
            " j/k: navigate | PgUp/PgDn: page | /: search | f: filter | q: quit ".to_string()
        }
        AppMode::Search => format!(" Search: {}_ (Enter/Esc to confirm) ", state.search_query),
        AppMode::Filter => format!(
            " Filter type: {}_ (Enter/Esc to confirm) ",
            state.filter_type
        ),
    };
    let status_bar =
        Paragraph::new(status).style(Style::default().fg(Color::Black).bg(Color::Cyan));
    f.render_widget(status_bar, chunks[2]);
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
