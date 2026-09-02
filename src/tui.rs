use std::{
    fs::File,
    io::{self, IsTerminal},
    panic,
    path::PathBuf,
    time::Duration,
};

use crossterm::{
    cursor::{Hide, Show},
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEvent,
        KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::mpsc;

use crate::{
    app::{AppState, Focus, Message, Overlay, View},
    cache::{CacheStore, DEFAULT_TTL},
    config::{Config, Paths},
    error::{AppError, Result},
    export::{write_report, ExportFormat},
    ga4::{DateRange, Ga4Client},
    ui,
};

type AppTerminal = Terminal<CrosstermBackend<io::Stdout>>;

struct TerminalGuard {
    terminal: AppTerminal,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        if !io::stdout().is_terminal() {
            return Err(AppError::Other(
                "the TUI requires an interactive terminal".into(),
            ));
        }
        enable_raw_mode().map_err(|error| AppError::Other(error.to_string()))?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide) {
            let _ = disable_raw_mode();
            return Err(AppError::Other(error.to_string()));
        }
        let terminal = Terminal::new(CrosstermBackend::new(stdout))
            .map_err(|error| AppError::Other(error.to_string()))?;
        Ok(Self { terminal })
    }

    fn restore(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        );
        let _ = self.terminal.show_cursor();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

pub async fn run(
    paths: Paths,
    config: Config,
    client: Ga4Client,
    authenticated: bool,
) -> Result<()> {
    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(|info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        );
        eprintln!("{info}");
    }));
    let result = run_inner(paths, config, client, authenticated).await;
    panic::set_hook(old_hook);
    result
}

async fn run_inner(
    paths: Paths,
    config: Config,
    client: Ga4Client,
    authenticated: bool,
) -> Result<()> {
    let mut terminal = TerminalGuard::enter()?;
    let mut app = AppState::new(config)?;
    let cache = CacheStore::new(&paths);
    load_cached(&mut app, &cache);
    let (tx, mut rx) = mpsc::unbounded_channel();
    if !authenticated {
        app.status = "setup required".into();
        app.error = Some(AppError::Unauthenticated.to_string());
    } else if app.selected_property().is_some() {
        request_current(&mut app, &client, &cache, tx.clone());
    } else {
        request_properties(&mut app, &client, tx.clone());
    }
    let mut events = EventStream::new();
    let mut realtime_tick = tokio::time::interval(Duration::from_secs(60));
    realtime_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    while !app.quit {
        terminal
            .terminal
            .draw(|frame| ui::render(frame, &mut app))
            .map_err(|error| AppError::Other(error.to_string()))?;
        tokio::select! {
            event = events.next() => if let Some(Ok(Event::Key(key))) = event { if key.kind == KeyEventKind::Press { handle_key(key, &mut app, &paths, &client, &cache, tx.clone())?; } },
            Some(message) = rx.recv() => { app.apply_message(message); if app.config.cache_enabled { if let Some(report) = &app.overview { let key = overview_cache_key(&app); let _ = cache.put(&key, report.fetched_at, report); } } },
            _ = realtime_tick.tick(), if app.view == View::Realtime && app.overlay.is_none() => request_current(&mut app, &client, &cache, tx.clone()),
            _ = tokio::signal::ctrl_c() => app.quit = true,
        }
    }
    app.config.save(&paths)?;
    Ok(())
}

fn handle_key(
    key: KeyEvent,
    app: &mut AppState,
    paths: &Paths,
    client: &Ga4Client,
    cache: &CacheStore,
    tx: mpsc::UnboundedSender<Message>,
) -> Result<()> {
    if app.editing_filter {
        return handle_filter(key, app);
    }
    if let Some(overlay) = app.overlay.clone() {
        return handle_overlay(key, overlay, app, paths, client, cache, tx);
    }
    match key.code {
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Char('1') => {
            app.view = View::Overview;
            request_current(app, client, cache, tx);
        }
        KeyCode::Char('2') => {
            app.view = View::Realtime;
            request_current(app, client, cache, tx);
        }
        KeyCode::Left => {
            app.range = app.range.shifted(-1);
            request_current(app, client, cache, tx);
        }
        KeyCode::Right => {
            app.range = app.range.shifted(1);
            request_current(app, client, cache, tx);
        }
        KeyCode::Char('[') => {
            app.preset = app.preset.shorter();
            app.range = DateRange::last_complete_days(app.preset.days())?;
            request_current(app, client, cache, tx);
        }
        KeyCode::Char(']') => {
            app.preset = app.preset.longer();
            app.range = DateRange::last_complete_days(app.preset.days())?;
            request_current(app, client, cache, tx);
        }
        KeyCode::Tab => {
            app.focus = if app.focus == Focus::Chart {
                Focus::Table
            } else {
                Focus::Chart
            }
        }
        KeyCode::Char('j') | KeyCode::Down => app.selected_row = app.selected_row.saturating_add(1),
        KeyCode::Char('k') | KeyCode::Up => app.selected_row = app.selected_row.saturating_sub(1),
        KeyCode::Char('/') => {
            app.editing_filter = true;
            app.focus = Focus::Table;
        }
        KeyCode::Char('c') => {
            app.config.comparison_enabled = !app.config.comparison_enabled;
            request_current(app, client, cache, tx);
        }
        KeyCode::Char('p') => {
            app.overlay = Some(Overlay::Properties);
            if app.properties.is_empty() {
                request_properties(app, client, tx);
            }
        }
        KeyCode::Char('d') => {
            app.date_input = format!("{}..{}", app.range.start, app.range.end);
            app.overlay = Some(Overlay::DateInput);
        }
        KeyCode::Char('r') => request_current(app, client, cache, tx),
        KeyCode::Char('e') => export_from_tui(app)?,
        KeyCode::Enter if app.view == View::Overview && app.focus == Focus::Table => {
            app.overlay = Some(Overlay::Detail)
        }
        KeyCode::Char('?') => app.overlay = Some(Overlay::Help),
        _ => {}
    }
    Ok(())
}

fn handle_filter(key: KeyEvent, app: &mut AppState) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => app.editing_filter = false,
        KeyCode::Backspace => {
            app.filter.pop();
        }
        KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.filter.push(character)
        }
        _ => {}
    }
    app.selected_row = 0;
    Ok(())
}

fn handle_overlay(
    key: KeyEvent,
    overlay: Overlay,
    app: &mut AppState,
    paths: &Paths,
    client: &Ga4Client,
    cache: &CacheStore,
    tx: mpsc::UnboundedSender<Message>,
) -> Result<()> {
    match overlay {
        Overlay::Help | Overlay::Detail => {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                app.overlay = None;
            }
        }
        Overlay::Properties => match key.code {
            KeyCode::Esc => {
                if app.property_filter.is_empty() {
                    app.overlay = None
                } else {
                    app.property_filter.clear();
                    app.property_row = 0;
                }
            }
            KeyCode::Backspace => {
                app.property_filter.pop();
                app.property_row = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let count = matching_properties(app).count();
                if count > 0 {
                    app.property_row = (app.property_row + 1).min(count - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.property_row = app.property_row.saturating_sub(1)
            }
            KeyCode::Enter => {
                let property = { matching_properties(app).nth(app.property_row).cloned() };
                if let Some(property) = property {
                    app.config.selected_property = Some(property.id().into());
                    app.config.selected_property_name = Some(property.property_name);
                    app.config.save(paths)?;
                    app.overlay = None;
                    app.property_filter.clear();
                    request_current(app, client, cache, tx);
                }
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.property_filter.push(character);
                app.property_row = 0;
            }
            _ => {}
        },
        Overlay::DateInput => match key.code {
            KeyCode::Esc => app.overlay = None,
            KeyCode::Backspace => {
                app.date_input.pop();
            }
            KeyCode::Enter => {
                let parts: Vec<_> = app.date_input.split("..").collect();
                if parts.len() == 2 {
                    match (
                        chrono::NaiveDate::parse_from_str(parts[0], "%Y-%m-%d"),
                        chrono::NaiveDate::parse_from_str(parts[1], "%Y-%m-%d"),
                    ) {
                        (Ok(start), Ok(end)) => match DateRange::new(start, end) {
                            Ok(range) => {
                                app.range = range;
                                app.overlay = None;
                                app.error = None;
                                request_current(app, client, cache, tx);
                            }
                            Err(error) => app.error = Some(error.to_string()),
                        },
                        _ => app.error = Some("dates must use YYYY-MM-DD..YYYY-MM-DD".into()),
                    }
                } else {
                    app.error = Some("dates must use YYYY-MM-DD..YYYY-MM-DD".into());
                }
            }
            KeyCode::Char(character)
                if character.is_ascii_digit() || character == '-' || character == '.' =>
            {
                app.date_input.push(character)
            }
            _ => {}
        },
    }
    Ok(())
}

fn matching_properties(app: &AppState) -> impl Iterator<Item = &crate::ga4::PropertySummary> {
    let filter = app.property_filter.to_ascii_lowercase();
    app.properties.iter().filter(move |property| {
        format!(
            "{} {} {}",
            property.account_name,
            property.property_name,
            property.id()
        )
        .to_ascii_lowercase()
        .contains(&filter)
    })
}

fn request_properties(app: &mut AppState, client: &Ga4Client, tx: mpsc::UnboundedSender<Message>) {
    app.loading = true;
    app.status = "loading properties".into();
    let client = client.clone();
    tokio::spawn(async move {
        let _ = tx.send(Message::PropertiesLoaded(client.properties().await));
    });
}

fn request_current(
    app: &mut AppState,
    client: &Ga4Client,
    _cache: &CacheStore,
    tx: mpsc::UnboundedSender<Message>,
) {
    let Some(property) = app.selected_property().map(str::to_owned) else {
        return;
    };
    let client = client.clone();
    match app.view {
        View::Overview => {
            let range = app.range;
            let comparison = app.config.comparison_enabled;
            let generation = app.begin_request("refreshing");
            tokio::spawn(async move {
                let _ = tx.send(Message::OverviewLoaded {
                    generation,
                    result: client.overview(&property, range, comparison).await,
                });
            });
        }
        View::Realtime => {
            let generation = app.begin_request("refreshing realtime");
            tokio::spawn(async move {
                let _ = tx.send(Message::RealtimeLoaded {
                    generation,
                    result: client.realtime(&property).await,
                });
            });
        }
    }
}

fn load_cached(app: &mut AppState, cache: &CacheStore) {
    if !app.config.cache_enabled {
        return;
    }
    if let Some(hit) = cache.get(&overview_cache_key(app), DEFAULT_TTL) {
        app.overview = Some(hit.value);
        app.stale = !hit.fresh;
        app.status = if hit.fresh {
            "cached".into()
        } else {
            "stale cache".into()
        };
    }
}

fn overview_cache_key(app: &AppState) -> String {
    CacheStore::key(&[
        app.selected_property().unwrap_or("none"),
        &app.range.start.to_string(),
        &app.range.end.to_string(),
        if app.config.comparison_enabled {
            "compare"
        } else {
            "single"
        },
    ])
}

fn export_from_tui(app: &mut AppState) -> Result<()> {
    let Some(report) = &app.overview else {
        app.status = "nothing to export".into();
        return Ok(());
    };
    let path = PathBuf::from(format!("ga4-overview-{}-{}.csv", report.start, report.end));
    let file = File::create(&path).map_err(|error| AppError::Other(error.to_string()))?;
    write_report(file, ExportFormat::Csv, report)?;
    app.status = format!("exported {}", path.display());
    Ok(())
}
