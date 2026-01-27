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
const QUERY_MAX_LEN: usize = 200;

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
    /// Cached indices of visible events (recomputed only on filter/search change).
    visible_cache: Vec<usize>,
    /// Tracks whether the cache needs recomputation.
    cache_dirty: bool,
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
            .and_then(|i| self.visible_cache.get(i))
            .and_then(|&idx| self.events.get(idx))
    }

    /// Recompute visible event indices if the cache is dirty.
    fn ensure_visible_cache(&mut self) {
        if !self.cache_dirty {
            return;
        }
        let search_lower = self.search_query.to_lowercase();
        self.visible_cache = self
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if !self.search_query.is_empty() {
                    let subject = e.subject.as_deref().unwrap_or("");
                    let sanitized = sanitize_terminal_with_limit(subject, SUBJECT_MAX_LEN);
                    if !sanitized.to_lowercase().contains(&search_lower) {
                        return false;
                    }
                }
                if !self.filter_type.is_empty() && !e.type_.contains(&self.filter_type) {
                    return false;
                }
                true
            })
            .map(|(i, _)| i)
            .collect();
        self.cache_dirty = false;
    }

    fn invalidate_cache(&mut self) {
        self.cache_dirty = true;
    }

    fn visible_event_refs(&self) -> Vec<&EvidenceEvent> {
        self.visible_cache
            .iter()
            .filter_map(|&idx| self.events.get(idx))
            .collect()
    }

    fn move_selection(&mut self, delta: i64) {
        let len = self.visible_cache.len();
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

    let visible_cache: Vec<usize> = (0..events.len()).collect();
    let mut state = AppState {
        events,
        list_state: ListState::default(),
        search_query: String::new(),
        filter_type: String::new(),
        mode: AppMode::Normal,
        run_id,
        event_count,
        verified,
        visible_cache,
        cache_dirty: false,
    };
    state.list_state.select(Some(0));

    run_tui(&mut state)?;
    Ok(0)
}

fn run_tui(state: &mut AppState) -> Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let result = run_tui_inner(state);

    // Always restore terminal state, even if the event loop errored.
    let _ = disable_raw_mode();
    let _ = stdout().execute(LeaveAlternateScreen);

    result
}

fn run_tui_inner(state: &mut AppState) -> Result<()> {
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
                            let len = state.visible_cache.len();
                            if len > 0 {
                                state.list_state.select(Some(len - 1));
                            }
                        }
                        KeyCode::Char('/') => {
                            state.mode = AppMode::Search;
                            state.search_query.clear();
                            state.invalidate_cache();
                        }
                        KeyCode::Char('f') => {
                            state.mode = AppMode::Filter;
                            state.filter_type.clear();
                            state.invalidate_cache();
                        }
                        KeyCode::Esc => {
                            state.search_query.clear();
                            state.filter_type.clear();
                            state.invalidate_cache();
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
                            state.invalidate_cache();
                        }
                        KeyCode::Char(c) => {
                            // Filter control chars and cap length
                            if !c.is_control() && state.search_query.chars().count() < QUERY_MAX_LEN
                            {
                                state.search_query.push(c);
                                state.invalidate_cache();
                            }
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
                            state.invalidate_cache();
                        }
                        KeyCode::Char(c) => {
                            // Filter control chars and cap length
                            if !c.is_control() && state.filter_type.chars().count() < QUERY_MAX_LEN
                            {
                                state.filter_type.push(c);
                                state.invalidate_cache();
                            }
                        }
                        _ => {}
                    },
                }
            }
        }
    }

    Ok(())
}

fn draw_ui(f: &mut ratatui::Frame<'_>, state: &mut AppState) {
    state.ensure_visible_cache();

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
    let visible = state.visible_event_refs();
    let visible_count = visible.len();
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
        format!(
            " Events (search: {}) ",
            sanitize_terminal_with_limit(&state.search_query, QUERY_MAX_LEN)
        )
    } else if !state.filter_type.is_empty() {
        format!(
            " Events (filter: {}) ",
            sanitize_terminal_with_limit(&state.filter_type, QUERY_MAX_LEN)
        )
    } else {
        format!(" Events ({}) ", visible_count)
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
        AppMode::Search => format!(
            " Search: {}_ (Enter/Esc to confirm) ",
            sanitize_terminal_with_limit(&state.search_query, QUERY_MAX_LEN)
        ),
        AppMode::Filter => format!(
            " Filter type: {}_ (Enter/Esc to confirm) ",
            sanitize_terminal_with_limit(&state.filter_type, QUERY_MAX_LEN)
        ),
    };
    let status_bar =
        Paragraph::new(status).style(Style::default().fg(Color::Black).bg(Color::Cyan));
    f.render_widget(status_bar, chunks[2]);
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- truncate_str tests (UTF-8 safety) --

    #[test]
    fn test_truncate_str_ascii() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_str_multibyte_utf8() {
        // 世界 is 2 chars, 6 bytes. Must not panic on byte boundary.
        let input = "Hello 世界 🌍 test";
        let result = truncate_str(input, 10);
        assert!(result.ends_with("..."));
        assert!(result.chars().count() <= 10);
    }

    #[test]
    fn test_truncate_str_exact_boundary() {
        assert_eq!(truncate_str("abcde", 5), "abcde");
        assert_eq!(truncate_str("abcdef", 5), "ab...");
    }

    // -- Input filtering tests --

    #[test]
    fn test_search_input_rejects_control_chars() {
        // Simulate what the input handler does
        let c = '\x1b'; // ESC character
        assert!(c.is_control(), "ESC should be classified as control");

        let c2 = '\x07'; // BEL
        assert!(c2.is_control(), "BEL should be classified as control");

        let c3 = '\x00'; // NUL
        assert!(c3.is_control(), "NUL should be classified as control");

        // Normal chars should pass
        assert!(!'/'.is_control());
        assert!(!'a'.is_control());
        assert!(!'世'.is_control());
    }

    #[test]
    fn test_search_query_length_cap() {
        let mut query = String::new();
        for _ in 0..QUERY_MAX_LEN + 50 {
            let c = 'a';
            if !c.is_control() && query.chars().count() < QUERY_MAX_LEN {
                query.push(c);
            }
        }
        assert_eq!(query.chars().count(), QUERY_MAX_LEN);
    }

    // -- AppState visible cache tests --

    #[test]
    fn test_visible_cache_filters_events() {
        let events = vec![
            make_test_event(0, "assay.net.connect", Some("api.example.com")),
            make_test_event(1, "assay.fs.access", Some("/etc/passwd")),
            make_test_event(2, "assay.net.connect", Some("evil.example.com")),
        ];

        let mut state = AppState {
            events,
            list_state: ListState::default(),
            search_query: "evil".into(),
            filter_type: String::new(),
            mode: AppMode::Normal,
            run_id: "test".into(),
            event_count: 3,
            verified: true,
            visible_cache: Vec::new(),
            cache_dirty: true,
        };
        state.ensure_visible_cache();

        assert_eq!(state.visible_cache.len(), 1);
        assert_eq!(state.visible_cache[0], 2); // only the evil.example.com event
    }

    #[test]
    fn test_visible_cache_filter_type() {
        let events = vec![
            make_test_event(0, "assay.net.connect", Some("host")),
            make_test_event(1, "assay.fs.access", Some("path")),
            make_test_event(2, "assay.net.connect", Some("other")),
        ];

        let mut state = AppState {
            events,
            list_state: ListState::default(),
            search_query: String::new(),
            filter_type: "fs".into(),
            mode: AppMode::Normal,
            run_id: "test".into(),
            event_count: 3,
            verified: true,
            visible_cache: Vec::new(),
            cache_dirty: true,
        };
        state.ensure_visible_cache();

        assert_eq!(state.visible_cache.len(), 1);
        assert_eq!(state.visible_cache[0], 1);
    }

    #[test]
    fn test_invalidate_cache_recomputes() {
        let events = vec![make_test_event(0, "assay.test", Some("hello"))];
        let mut state = AppState {
            events,
            list_state: ListState::default(),
            search_query: String::new(),
            filter_type: String::new(),
            mode: AppMode::Normal,
            run_id: "test".into(),
            event_count: 1,
            verified: true,
            visible_cache: vec![0],
            cache_dirty: false,
        };

        // Initially 1 visible event
        assert_eq!(state.visible_cache.len(), 1);

        // Set search that filters everything, invalidate cache
        state.search_query = "nonexistent".into();
        state.invalidate_cache();
        state.ensure_visible_cache();

        assert_eq!(state.visible_cache.len(), 0);
    }

    fn make_test_event(seq: u64, type_: &str, subject: Option<&str>) -> EvidenceEvent {
        let mut event =
            EvidenceEvent::new(type_, "urn:test", "testrun", seq, serde_json::json!({}));
        if let Some(s) = subject {
            event = event.with_subject(s);
        }
        event
    }
}
