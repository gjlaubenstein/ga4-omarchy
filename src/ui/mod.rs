use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Sparkline, Table, TableState,
        Tabs, Wrap,
    },
    Frame,
};

use crate::{
    app::{AppState, Focus, Overlay, View},
    ga4::{format_number, MetricValue},
};

pub const MIN_WIDTH: u16 = 48;
pub const MIN_HEIGHT: u16 = 16;

pub fn render(frame: &mut Frame<'_>, app: &mut AppState) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(
            Paragraph::new(format!(
                "Terminal too small\nNeed at least {MIN_WIDTH}×{MIN_HEIGHT}\nCurrent: {}×{}",
                area.width, area.height
            ))
            .alignment(Alignment::Center)
            .block(Block::bordered().title(" ga4 ")),
            area,
        );
        return;
    }
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_header(frame, app, layout[0]);
    render_tabs(frame, app, layout[1]);
    match app.view {
        View::Overview => render_overview(frame, app, layout[2]),
        View::Realtime => render_realtime(frame, app, layout[2]),
    }
    render_footer(frame, app, layout[3]);
    if let Some(overlay) = app.overlay.clone() {
        render_overlay(frame, app, overlay, area);
    }
}

fn accent(app: &AppState) -> Color {
    if app.config.no_color {
        Color::White
    } else {
        Color::Blue
    }
}
fn good(app: &AppState) -> Color {
    if app.config.no_color {
        Color::White
    } else {
        Color::Green
    }
}
fn bad(app: &AppState) -> Color {
    if app.config.no_color {
        Color::White
    } else {
        Color::Red
    }
}
fn muted() -> Color {
    Color::DarkGray
}

fn render_header(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let report = match app.view {
        View::Overview => "Overview",
        View::Realtime => "Realtime",
    };
    let property = app
        .config
        .selected_property_name
        .as_deref()
        .or(app.config.selected_property.as_deref())
        .unwrap_or("No property selected");
    let comparison = if app.config.comparison_enabled {
        " · compare on"
    } else {
        ""
    };
    let left = Line::from(vec![
        Span::styled(
            format!(" ga4 · {report}"),
            Style::default()
                .fg(accent(app))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" · {property}")),
    ]);
    let right = format!(
        "{} → {}{comparison} · {} ",
        app.range.start, app.range.end, app.status
    );
    let columns =
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)]).split(area);
    frame.render_widget(Paragraph::new(left), columns[0]);
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right),
        columns[1],
    );
}

fn render_tabs(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let selected = if app.view == View::Overview { 0 } else { 1 };
    let tabs = Tabs::new(["1 Overview", "2 Realtime"])
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(accent(app))
                .add_modifier(Modifier::BOLD),
        )
        .divider(" │ ")
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(tabs, area);
}

fn render_overview(frame: &mut Frame<'_>, app: &mut AppState, area: Rect) {
    let Some(report) = app.overview.as_ref() else {
        render_empty(
            frame,
            app,
            area,
            "Overview",
            "Authenticate and select a property to load analytics.",
        );
        return;
    };
    let vertical = Layout::vertical([Constraint::Length(5), Constraint::Min(5)]).split(area);
    let metrics = Layout::horizontal([Constraint::Percentage(25); 4]).split(vertical[0]);
    let values = [
        ("Active users", &report.summary.active_users, false),
        ("Sessions", &report.summary.sessions, false),
        ("Engagement", &report.summary.engagement_rate, true),
        ("Key events", &report.summary.key_events, false),
    ];
    for (index, (label, value, percent)) in values.into_iter().enumerate() {
        render_metric(frame, app, metrics[index], label, value, percent);
    }
    if vertical[1].width >= 80 {
        let columns = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(vertical[1]);
        render_trend(frame, app, columns[0]);
        render_channels(frame, app, columns[1]);
    } else {
        let rows = Layout::vertical([Constraint::Percentage(52), Constraint::Percentage(48)])
            .split(vertical[1]);
        render_trend(frame, app, rows[0]);
        render_channels(frame, app, rows[1]);
    }
}

fn render_metric(
    frame: &mut Frame<'_>,
    app: &AppState,
    area: Rect,
    label: &str,
    metric: &MetricValue,
    percent: bool,
) {
    let value = if percent {
        format!("{:.1}%", metric.current * 100.0)
    } else {
        format_number(metric.current)
    };
    let delta = metric
        .delta_percent()
        .map(|v| format!(" {v:+.1}%"))
        .unwrap_or_else(|| " —".into());
    let color = if metric.delta_percent().unwrap_or(0.0) >= 0.0 {
        good(app)
    } else {
        bad(app)
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(label, muted()),
            Line::from(vec![
                Span::styled(value, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(delta, Style::default().fg(color)),
            ]),
        ])
        .block(Block::bordered()),
        area,
    );
}

fn render_trend(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let current: Vec<_> = app
        .overview
        .as_ref()
        .into_iter()
        .flat_map(|r| r.trend.iter())
        .filter(|p| !p.comparison)
        .map(|p| p.active_users as u64)
        .collect();
    let title = if app.focus == Focus::Chart {
        " Users over time · focused "
    } else {
        " Users over time "
    };
    let block = Block::bordered()
        .title(title)
        .border_style(if app.focus == Focus::Chart {
            Style::default().fg(accent(app))
        } else {
            Style::default()
        });
    frame.render_widget(
        Sparkline::default()
            .data(&current)
            .style(Style::default().fg(accent(app)))
            .block(block),
        area,
    );
}

fn render_channels(frame: &mut Frame<'_>, app: &mut AppState, area: Rect) {
    let Some(report) = app.overview.as_ref() else {
        return;
    };
    let filter = app.filter.to_ascii_lowercase();
    let rows: Vec<_> = report
        .channels
        .iter()
        .filter(|row| row.channel.to_ascii_lowercase().contains(&filter))
        .map(|row| {
            Row::new(vec![
                Cell::from(row.channel.clone()),
                Cell::from(format_number(row.sessions)),
                Cell::from(format!("{:.1}%", row.engagement_rate * 100.0)),
            ])
        })
        .collect();
    let title = if app.editing_filter {
        format!(" Channels · /{} ", app.filter)
    } else if app.focus == Focus::Table {
        " Channels · focused ".into()
    } else {
        " Channels ".into()
    };
    let table = Table::new(
        rows,
        [
            Constraint::Min(14),
            Constraint::Length(9),
            Constraint::Length(8),
        ],
    )
    .header(Row::new(["Channel", "Sessions", "Engage"]).style(Style::default().fg(muted())))
    .row_highlight_style(
        Style::default()
            .fg(accent(app))
            .add_modifier(Modifier::BOLD),
    )
    .block(
        Block::bordered()
            .title(title)
            .border_style(if app.focus == Focus::Table {
                Style::default().fg(accent(app))
            } else {
                Style::default()
            }),
    );
    let mut state = TableState::default().with_selected(Some(app.selected_row));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_realtime(frame: &mut Frame<'_>, app: &mut AppState, area: Rect) {
    let Some(report) = app.realtime.as_ref() else {
        render_empty(
            frame,
            app,
            area,
            "Realtime",
            "Press r to load the last 30 minutes.",
        );
        return;
    };
    let rows = Layout::vertical([Constraint::Length(5), Constraint::Min(6)]).split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("Active users · 30 min", muted()),
            Line::styled(
                format_number(report.active_users),
                Style::default().fg(good(app)).add_modifier(Modifier::BOLD),
            ),
        ])
        .block(Block::bordered()),
        rows[0],
    );
    let columns = if rows[1].width >= 80 {
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1])
    } else {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1])
    };
    render_ranked(frame, app, columns[0], "Top pages", &report.pages);
    render_ranked(frame, app, columns[1], "Top events", &report.events);
}

fn render_ranked(
    frame: &mut Frame<'_>,
    app: &AppState,
    area: Rect,
    title: &str,
    rows: &[crate::ga4::RankedRow],
) {
    let items = rows
        .iter()
        .take(area.height.saturating_sub(2) as usize)
        .map(|row| {
            ListItem::new(Line::from(vec![
                Span::raw(truncate(&row.name, area.width.saturating_sub(14) as usize)),
                Span::styled(
                    format!("  {:>8}", format_number(row.primary)),
                    Style::default().fg(accent(app)),
                ),
            ]))
        });
    frame.render_widget(
        List::new(items).block(Block::bordered().title(format!(" {title} "))),
        area,
    );
}

fn render_empty(frame: &mut Frame<'_>, app: &AppState, area: Rect, title: &str, message: &str) {
    let detail = app.error.as_deref().unwrap_or(message);
    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center)
            .block(Block::bordered().title(format!(" {title} ")).border_style(
                if app.error.is_some() {
                    Style::default().fg(bad(app))
                } else {
                    Style::default()
                },
            )),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    let error = app
        .error
        .as_deref()
        .map(|error| format!(" · {error}"))
        .unwrap_or_default();
    let text = if app.editing_filter {
        "Type to filter · Enter apply · Esc cancel".into()
    } else {
        format!("←/→ date  [/] range  Tab focus  p property  d custom  r refresh  e export  ? help  q quit · {}{error}", app.age_label())
    };
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(if app.error.is_some() {
                bad(app)
            } else {
                muted()
            }))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_overlay(frame: &mut Frame<'_>, app: &mut AppState, overlay: Overlay, area: Rect) {
    let popup = centered(area, 72, 70);
    frame.render_widget(Clear, popup);
    match overlay {
        Overlay::Help => {
            let help = "1/2 reports        ←/→ move date range\n[/] change preset  Tab focus pane\nj/k select row     Enter confirm\nEsc close/back     / filter table\nc comparison       p choose property\nd custom dates     r refresh\ne export CSV       ? help · q quit";
            frame.render_widget(
                Paragraph::new(help)
                    .block(Block::bordered().title(" Help "))
                    .wrap(Wrap { trim: false }),
                popup,
            );
        }
        Overlay::Properties => {
            let filter = app.property_filter.to_ascii_lowercase();
            let items: Vec<_> = app
                .properties
                .iter()
                .filter(|property| {
                    format!(
                        "{} {} {}",
                        property.account_name,
                        property.property_name,
                        property.id()
                    )
                    .to_ascii_lowercase()
                    .contains(&filter)
                })
                .map(|property| {
                    ListItem::new(format!(
                        "{} · {}",
                        property.account_name, property.property_name
                    ))
                })
                .collect();
            let title = if app.property_filter.is_empty() {
                " Choose property · type to search · j/k · Enter ".into()
            } else {
                format!(" Choose property · /{} ", app.property_filter)
            };
            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .fg(accent(app))
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> ")
                .block(Block::bordered().title(title));
            let mut state =
                ratatui::widgets::ListState::default().with_selected(Some(app.property_row));
            frame.render_stateful_widget(list, popup, &mut state);
        }
        Overlay::DateInput => {
            let text = vec![
                Line::raw("Enter an inclusive range:"),
                Line::raw(""),
                Line::styled(&app.date_input, Style::default().fg(accent(app))),
                Line::raw(""),
                Line::styled(
                    "Format: YYYY-MM-DD..YYYY-MM-DD · Enter apply · Esc cancel",
                    muted(),
                ),
            ];
            frame.render_widget(
                Paragraph::new(text)
                    .block(Block::bordered().title(" Custom date range "))
                    .wrap(Wrap { trim: true }),
                popup,
            );
        }
        Overlay::Detail => {
            let detail = app
                .overview
                .as_ref()
                .and_then(|report| report.channels.get(app.selected_row));
            let text = detail
                .map(|row| {
                    vec![
                        Line::styled(
                            &row.channel,
                            Style::default()
                                .fg(accent(app))
                                .add_modifier(Modifier::BOLD),
                        ),
                        Line::raw(""),
                        Line::raw(format!(
                            "Active users   {}",
                            format_number(row.active_users)
                        )),
                        Line::raw(format!("Sessions       {}", format_number(row.sessions))),
                        Line::raw(format!(
                            "Engagement     {:.1}%",
                            row.engagement_rate * 100.0
                        )),
                        Line::raw(""),
                        Line::styled("Esc close", muted()),
                    ]
                })
                .unwrap_or_else(|| vec![Line::raw("No row selected")]);
            frame.render_widget(
                Paragraph::new(text).block(Block::bordered().title(" Channel detail ")),
                popup,
            );
        }
    }
}

fn centered(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.into();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::AppState, config::Config};
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn layouts_render_at_supported_sizes() {
        for (width, height) in [(120, 36), (80, 24), (60, 20), (40, 10)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = AppState::new(Config::default()).unwrap();
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
        }
    }

    #[test]
    fn small_terminal_explains_minimum_size() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = AppState::new(Config::default()).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Terminal too small"));
    }
}
