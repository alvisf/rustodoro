use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};

use crate::app::{App, Outcome, Phase, Screen, format_duration};
use crate::store;

fn phase_color(phase: Phase) -> Color {
    match phase {
        Phase::Work => Color::Red,
        Phase::Break => Color::Green,
        Phase::LongBreak => Color::Cyan,
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Setup => draw_setup(frame, app),
        Screen::TaskInput => draw_task_input(frame, app),
        Screen::NotesInput => draw_notes_input(frame, app),
        Screen::Timer => draw_timer_screen(frame, app),
        Screen::DailyLog => draw_daily_log(frame, app),
    }
}

// ── Setup screen ──────────────────────────────────────────

fn draw_setup(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    let block = Block::default()
        .title(" 🍅 Pomodoro Timer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    let fields: [(&str, u64, &str); 4] = [
        ("Work", app.work_secs / 60, "min"),
        ("Break", app.break_secs / 60, "min"),
        ("Long Break", app.long_break_secs / 60, "min"),
        ("Cycle", app.sessions_before_long as u64, "sessions"),
    ];

    let content_height: u16 = 7;
    let v_pad = inner.height.saturating_sub(content_height) / 2;

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..v_pad {
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "Configure your session",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    for (i, (label, value, unit)) in fields.iter().enumerate() {
        let selected = i == app.selected_field;
        let marker = if selected { "▸ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        lines.push(Line::from(Span::styled(
            format!("{marker}{label:<16}{value:>3} {unit}"),
            style,
        )));
    }

    let para = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(para, inner);

    draw_controls(
        frame,
        chunks[1],
        &[
            ("↑↓", "navigate"),
            ("←→", "adjust"),
            ("Enter", "start"),
            ("q", "quit"),
        ],
    );
}

// ── Task input screen ─────────────────────────────────────

fn draw_task_input(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    let block = Block::default()
        .title(" 🍅 Pomodoro Timer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    let content_height: u16 = 5;
    let v_pad = inner.height.saturating_sub(content_height) / 2;

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..v_pad {
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "What are you working on?",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    let input_text = format!("> {}▏", app.task_input_buffer);
    lines.push(Line::from(Span::styled(
        input_text,
        Style::default().fg(Color::Yellow),
    )));

    let para = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(para, inner);

    draw_controls(frame, chunks[1], &[("Enter", "start"), ("Esc", "skip")]);
}

// ── Notes input screen ───────────────────────────────────

fn draw_notes_input(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    let block = Block::default()
        .title(" 🍅 Pomodoro Timer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    let content_height: u16 = 7;
    let v_pad = inner.height.saturating_sub(content_height) / 2;

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..v_pad {
        lines.push(Line::from(""));
    }

    if !app.current_task.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("Task: {}", app.current_task),
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        "Any notes on this task?",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    let input_text = format!("> {}▏", app.notes_input_buffer);
    lines.push(Line::from(Span::styled(
        input_text,
        Style::default().fg(Color::Yellow),
    )));

    let para = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(para, inner);

    draw_controls(frame, chunks[1], &[("Enter", "save"), ("Esc", "skip")]);
}

// ── Timer screen ──────────────────────────────────────────

fn draw_timer_screen(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),
            Constraint::Length(4),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(frame.area());

    draw_timer(frame, chunks[0], app);
    draw_stats(frame, chunks[1], app);
    draw_log(frame, chunks[2], app);

    let pause_label: &str = if app.paused { "resume" } else { "pause" };
    let mut bindings: Vec<(&str, &str)> = vec![("space", pause_label)];
    if app.phase == Phase::Work {
        bindings.push(("Enter", "break"));
        bindings.push(("e", "end task"));
    }
    bindings.extend([("s", "skip"), ("d", "daily log"), ("q", "quit")]);
    draw_controls(frame, chunks[3], &bindings);
}

fn draw_timer(frame: &mut Frame, area: Rect, app: &App) {
    let overtime = app.is_overtime();
    let color = if app.paused || overtime {
        Color::Yellow
    } else {
        phase_color(app.phase)
    };

    let block = Block::default()
        .title(" 🍅 Pomodoro Timer ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let phase_label = format!("{} {}", app.phase.icon(), app.phase.label());
    let session_label = format!("Session #{}", app.session);
    let time_str = if overtime {
        format!("+{}", format_duration(app.overtime_secs()))
    } else {
        format_duration(app.remaining_secs())
    };

    let task_line = if app.phase == Phase::Work && !app.current_task.is_empty() {
        Line::from(Span::styled(
            app.current_task.clone(),
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from("")
    };

    let status_line = if app.paused {
        Line::from(Span::styled(
            "⏸  PAUSED",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
    } else if overtime {
        Line::from(Span::styled(
            "⏰ Time's up! Press Enter for break",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from("")
    };

    let text = Text::from(vec![
        Line::from(""),
        Line::from(Span::styled(
            phase_label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            session_label,
            Style::default().fg(Color::Gray),
        )),
        task_line,
        Line::from(Span::styled(
            time_str,
            Style::default()
                .fg(if overtime { Color::Yellow } else { color })
                .add_modifier(Modifier::BOLD),
        )),
        status_line,
    ]);

    let para = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(para, rows[0]);

    let pct = (app.progress() * 100.0) as u16;
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
        .percent(pct)
        .label(format!("{pct}%"));
    frame.render_widget(gauge, rows[1]);
}

fn draw_stats(frame: &mut Frame, area: Rect, app: &App) {
    let work_m = app.work_secs / 60;
    let break_m = app.break_secs / 60;
    let long_m = app.long_break_secs / 60;
    let today_hours = store::format_hours(app.today_work_secs());
    let today_sessions = app.today_sessions();
    let cycle = format!("{}/{}", app.sessions_in_cycle(), app.sessions_before_long);

    let lines = vec![
        Line::from(vec![
            Span::styled("  📊 Today: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                today_hours,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "  ({today_sessions} session{})",
                if today_sessions == 1 { "" } else { "s" }
            )),
            Span::raw(format!("  │  Cycle: {cycle}")),
        ]),
        Line::from(vec![
            Span::styled("  Work: ", Style::default().fg(Color::Red)),
            Span::raw(format!("{work_m}m")),
            Span::raw("  │  "),
            Span::styled("Break: ", Style::default().fg(Color::Green)),
            Span::raw(format!("{break_m}m")),
            Span::raw("  │  "),
            Span::styled("Long: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{long_m}m")),
        ]),
    ];

    let widget = Paragraph::new(lines).block(
        Block::default()
            .title(" Stats ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(widget, area);
}

fn draw_log(frame: &mut Frame, area: Rect, app: &App) {
    let overtime = app.is_overtime();
    let color = if app.paused || overtime {
        Color::Yellow
    } else {
        phase_color(app.phase)
    };
    let (status, icon) = if app.paused {
        ("paused", "⏸")
    } else if overtime {
        ("overtime", "⏰")
    } else {
        ("running", "▶")
    };

    let mut spans = vec![
        Span::styled(format!("  {icon} "), Style::default().fg(color)),
        Span::styled(
            format!("#{:<3} ", app.session),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!("{:<11}", app.phase.label()),
            Style::default().fg(color),
        ),
        Span::raw(if overtime {
            format!(
                "+{} / {}  ",
                format_duration(app.overtime_secs()),
                format_duration(app.phase_total_secs()),
            )
        } else {
            format!(
                "{} / {}  ",
                format_duration(app.remaining_secs()),
                format_duration(app.phase_total_secs()),
            )
        }),
        Span::styled(status, Style::default().fg(color)),
    ];
    if app.phase == Phase::Work && !app.current_task.is_empty() {
        spans.push(Span::styled(
            format!("  {}", app.current_task),
            Style::default().fg(Color::DarkGray),
        ));
    }
    let current_item = ListItem::new(Line::from(spans));

    let history_items: Vec<ListItem> = app
        .history
        .iter()
        .rev()
        .map(|entry| {
            let icon = match entry.outcome {
                Outcome::Completed => "✓",
                Outcome::Skipped => "⏭",
            };
            let outcome_str = match entry.outcome {
                Outcome::Completed => "completed",
                Outcome::Skipped => "skipped",
            };
            let entry_color = phase_color(entry.phase);

            let mut spans = vec![
                Span::styled(format!("  {icon} "), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("#{:<3} ", entry.session),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!("{:<11}", entry.phase.label()),
                    Style::default().fg(entry_color),
                ),
                Span::raw(format!(
                    "{} / {}  ",
                    format_duration(entry.elapsed_secs),
                    format_duration(entry.total_secs),
                )),
                Span::styled(outcome_str, Style::default().fg(Color::DarkGray)),
            ];
            if entry.phase == Phase::Work && !entry.start_time.is_empty() {
                spans.push(Span::styled(
                    format!("  {}–{}", entry.start_time, entry.end_time),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            if !entry.task.is_empty() {
                spans.push(Span::styled(
                    format!("  {}", entry.task),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut items = vec![current_item];
    items.extend(history_items);

    let widget = List::new(items).block(
        Block::default()
            .title(" Log ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(widget, area);
}

// ── Daily log screen ─────────────────────────────────────

fn draw_daily_log(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    let block = Block::default()
        .title(" 📊 Daily Work Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    let today = store::local_date_str();
    let yesterday = store::yesterday_str();

    let mut items: Vec<ListItem> = Vec::new();

    if app.daily_stats.is_empty() && app.today_work_secs() == 0 {
        items.push(ListItem::new(Line::from(Span::styled(
            "  No work logged yet. Complete a work session to see stats!",
            Style::default().fg(Color::DarkGray),
        ))));
    } else {
        for (date, stats) in app.daily_stats.iter().rev() {
            let label = if *date == today {
                "today    "
            } else if *date == yesterday {
                "yesterday"
            } else {
                "         "
            };

            let work_secs = if *date == today {
                app.today_work_secs()
            } else {
                stats.work_secs
            };
            let hours = store::format_hours(work_secs);
            let sessions = stats.sessions;

            let style = if *date == today {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {date}  "), style),
                Span::styled(format!("{label}  "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{hours:>8}"), style),
                Span::styled(
                    format!(
                        "   {sessions} session{}",
                        if sessions == 1 { "" } else { "s" }
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ])));
        }
    }

    let list = List::new(items);
    frame.render_widget(list, inner);

    draw_controls(frame, chunks[1], &[("Esc", "back"), ("q", "quit")]);
}

// ── Shared controls bar ──────────────────────────────────

fn draw_controls(frame: &mut Frame, area: Rect, bindings: &[(&str, &str)]) {
    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in bindings.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("    "));
        }
        spans.push(Span::styled(format!(" {key} "), key_style));
        spans.push(Span::raw(*desc));
    }

    let widget = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(widget, area);
}
