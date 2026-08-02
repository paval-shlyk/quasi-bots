use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use super::app::{App, ConnState, Focus};

pub fn draw(frame: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    draw_header(frame, root[0], app);
    draw_main(frame, root[1], app);
    draw_log(frame, root[2], app);
    draw_status(frame, root[3], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let conn = match app.conn {
        ConnState::Disconnected => ("Disconnected", Color::Red),
        ConnState::Authenticating => ("Authenticating", Color::Yellow),
        ConnState::Connecting => ("Connecting", Color::Yellow),
        ConnState::Ready => ("Connected", Color::Green),
        ConnState::Calling => ("Calling…", Color::Cyan),
    };

    let server = app
        .server
        .as_ref()
        .map(|s| format!("{} v{} @ {}", s.name, s.version, s.protocol_version))
        .unwrap_or_else(|| "—".into());

    let title = Line::from(vec![
        Span::styled(
            " mcp-client ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(server, Style::default().fg(Color::White)),
        Span::raw("  "),
        Span::styled(
            format!("[{}]", conn.0),
            Style::default().fg(conn.1).add_modifier(Modifier::BOLD),
        ),
    ]);

    let tabs = Line::from(vec![
        tab_span(
            "1 Tools",
            app.focus == Focus::Tools || app.focus == Focus::Filter,
        ),
        Span::raw("  "),
        tab_span("2 Detail", app.focus == Focus::Detail),
        Span::raw("  "),
        tab_span("3 Call", app.focus == Focus::Args),
        Span::raw("  "),
        tab_span("4 Result", app.focus == Focus::Result),
        Span::raw("   "),
        Span::styled(
            "c connect  l login  r refresh  / filter  e edit  i invoke  q quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);
    frame.render_widget(Paragraph::new(title), rows[0]);
    frame.render_widget(Paragraph::new(tabs), rows[1]);
}

fn tab_span(label: &str, active: bool) -> Span<'_> {
    if active {
        Span::styled(
            format!("[{label}]"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!(" {label} "), Style::default().fg(Color::Gray))
    }
}

fn draw_main(frame: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(35),
            Constraint::Percentage(35),
        ])
        .split(area);

    draw_tools(frame, cols[0], app);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(cols[1]);

    draw_detail(frame, right[0], app);
    draw_args(frame, right[1], app);
    draw_result(frame, cols[2], app);
}

fn draw_tools(frame: &mut Frame, area: Rect, app: &App) {
    let title = if app.focus == Focus::Filter {
        format!("Tools filter: {}_", app.filter)
    } else if app.filter.is_empty() {
        format!("Tools ({})", app.tools.len())
    } else {
        format!(
            "Tools ({}/{}) /{}",
            app.filtered_indices.len(),
            app.tools.len(),
            app.filter
        )
    };

    let border =
        focus_style(app.focus == Focus::Tools || app.focus == Focus::Filter);
    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .enumerate()
        .map(|(vis_i, &tool_i)| {
            let tool = &app.tools[tool_i];
            let selected = vis_i == app.selected;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(tool.name.clone(), style)))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border),
    );
    frame.render_widget(list, area);
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &App) {
    let border = focus_style(app.focus == Focus::Detail);
    let text = if let Some(tool) = app.selected_tool() {
        let schema = serde_json::to_string_pretty(&tool.input_schema)
            .unwrap_or_else(|_| tool.input_schema.to_string());
        format!(
            "name: {}\n\n{}\n\ninput_schema:\n{}",
            tool.name, tool.description, schema
        )
    } else {
        "Select a tool".into()
    };

    let p = Paragraph::new(text).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Detail / Schema")
            .border_style(border),
    );
    frame.render_widget(p, area);
}

fn draw_args(frame: &mut Frame, area: Rect, app: &App) {
    let border = focus_style(app.focus == Focus::Args);
    let title = if app.editing_args {
        "Call args (editing — Esc to stop)"
    } else {
        "Call args (e edit, Enter/i invoke)"
    };
    let p = Paragraph::new(app.args_text.clone())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border),
        );
    frame.render_widget(p, area);
}

fn draw_result(frame: &mut Frame, area: Rect, app: &App) {
    let border = focus_style(app.focus == Focus::Result);
    let style = if app.result_is_error {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };
    let p = Paragraph::new(app.result_text.clone())
        .style(style)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Result")
                .border_style(border),
        );
    frame.render_widget(p, area);
}

fn draw_log(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = app
        .log
        .iter()
        .rev()
        .take(area.height.saturating_sub(2) as usize)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|e| {
            let (tag, color) = match e.level {
                crate::model::LogLevel::Info => ("INF", Color::Gray),
                crate::model::LogLevel::Warn => ("WRN", Color::Yellow),
                crate::model::LogLevel::Error => ("ERR", Color::Red),
                crate::model::LogLevel::Success => ("OK ", Color::Green),
            };
            Line::from(vec![
                Span::styled(format!("{tag} "), Style::default().fg(color)),
                Span::raw(e.message.clone()),
            ])
        })
        .collect();

    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Log"));
    frame.render_widget(p, area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let p = Paragraph::new(app.status_line.clone())
        .block(Block::default().borders(Borders::ALL).title("Status"));
    frame.render_widget(p, area);
}

fn focus_style(active: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}
