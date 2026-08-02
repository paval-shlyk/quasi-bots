use std::time::Duration;

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use crate::config::{ConnectOptions, empty_args_from_schema};
use crate::model::{CallOutcome, LogEntry, LogLevel, ServerStatus, ToolView};
use crate::{McpSession, login_oauth};

use super::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tools,
    Detail,
    Args,
    Result,
    Filter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    Disconnected,
    Authenticating,
    Connecting,
    Ready,
    Calling,
}

pub struct App {
    pub opts: ConnectOptions,
    pub conn: ConnState,
    pub focus: Focus,
    pub server: Option<ServerStatus>,
    pub tools: Vec<ToolView>,
    pub filtered_indices: Vec<usize>,
    pub selected: usize,
    pub filter: String,
    pub args_text: String,
    pub result_text: String,
    pub result_is_error: bool,
    pub log: Vec<LogEntry>,
    pub status_line: String,
    pub should_quit: bool,
    /// Session lives on the async side; we only hold a handle flag.
    pub has_session: bool,
    pub editing_args: bool,
    pub args_cursor: usize,
}

impl App {
    pub fn new(opts: ConnectOptions) -> Self {
        let mut app = Self {
            opts,
            conn: ConnState::Disconnected,
            focus: Focus::Tools,
            server: None,
            tools: Vec::new(),
            filtered_indices: Vec::new(),
            selected: 0,
            filter: String::new(),
            args_text: "{\n}".into(),
            result_text: String::new(),
            result_is_error: false,
            log: Vec::new(),
            status_line: "Disconnected — press c to connect, l for OAuth login"
                .into(),
            should_quit: false,
            has_session: false,
            editing_args: false,
            args_cursor: 0,
        };
        app.push_log(LogEntry::info(format!(
            "MCP URL: {} | protocol: 2025-11-25",
            app.opts.url
        )));
        app
    }

    pub fn push_log(&mut self, entry: LogEntry) {
        self.log.push(entry);
        if self.log.len() > 200 {
            let drain = self.log.len() - 200;
            self.log.drain(0..drain);
        }
    }

    pub fn recompute_filter(&mut self) {
        let q = self.filter.to_lowercase();
        self.filtered_indices = self
            .tools
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                q.is_empty()
                    || t.name.to_lowercase().contains(&q)
                    || t.description.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        if self.selected >= self.filtered_indices.len()
            && !self.filtered_indices.is_empty()
        {
            self.selected = self.filtered_indices.len() - 1;
        }
        if self.filtered_indices.is_empty() {
            self.selected = 0;
        }
    }

    pub fn selected_tool(&self) -> Option<&ToolView> {
        self.filtered_indices
            .get(self.selected)
            .and_then(|&i| self.tools.get(i))
    }

    pub fn select_tool_into_args(&mut self) {
        if let Some(tool) = self.selected_tool().cloned() {
            let schema =
                tool.input_schema.as_object().cloned().unwrap_or_default();
            let args = empty_args_from_schema(&schema);
            self.args_text = serde_json::to_string_pretty(&args)
                .unwrap_or_else(|_| "{}".into());
            self.args_cursor = self.args_text.len();
            self.result_text.clear();
            self.result_is_error = false;
            self.push_log(LogEntry::info(format!(
                "Selected tool {}",
                tool.name
            )));
            self.focus = Focus::Args;
        }
    }
}

enum WorkerCmd {
    Connect,
    Login,
    RefreshTools,
    CallTool {
        name: String,
        args: serde_json::Value,
    },
    Disconnect,
    Shutdown,
}

enum WorkerEvent {
    Log(LogEntry),
    Status(String),
    Conn(ConnState),
    Connected { server: ServerStatus },
    Tools(Vec<ToolView>),
    CallResult(CallOutcome),
    Token(String),
    Disconnected,
    Error(String),
}

/// Run the interactive TUI. Returns when the user quits.
pub async fn run_tui(opts: ConnectOptions) -> anyhow::Result<()> {
    let auto_connect = opts.token.is_some();
    let do_login_first = false; // handled by caller via token after --login

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WorkerCmd>();
    let (evt_tx, mut evt_rx) = mpsc::unbounded_channel::<WorkerEvent>();

    let worker_opts = opts.clone();
    let worker = tokio::spawn(async move {
        session_worker(worker_opts, &mut cmd_rx, evt_tx).await;
    });

    let mut app = App::new(opts);

    if do_login_first {
        let _ = cmd_tx.send(WorkerCmd::Login);
    } else if auto_connect {
        let _ = cmd_tx.send(WorkerCmd::Connect);
    }

    let result = async {
        loop {
            terminal.draw(|frame| ui::draw(frame, &app))?;

            // Drain worker events
            while let Ok(ev) = evt_rx.try_recv() {
                handle_event(&mut app, ev);
            }

            if app.should_quit {
                break;
            }

            // Non-blocking key poll with short timeout so we keep redrawing
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                handle_key(&mut app, key, &cmd_tx);
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    let _ = cmd_tx.send(WorkerCmd::Shutdown);
    let _ = worker.await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn handle_event(app: &mut App, ev: WorkerEvent) {
    match ev {
        WorkerEvent::Log(e) => app.push_log(e),
        WorkerEvent::Status(s) => app.status_line = s,
        WorkerEvent::Conn(c) => {
            app.conn = c;
            app.has_session =
                matches!(c, ConnState::Ready | ConnState::Calling);
        }
        WorkerEvent::Connected { server } => {
            app.server = Some(server.clone());
            app.conn = ConnState::Ready;
            app.has_session = true;
            app.status_line = format!(
                "Connected to {} v{} @ {}",
                server.name, server.version, server.protocol_version
            );
            app.push_log(LogEntry::success(format!(
                "Connected: {} {} (protocol {})",
                server.name, server.version, server.protocol_version
            )));
        }
        WorkerEvent::Tools(tools) => {
            app.tools = tools;
            app.recompute_filter();
            app.push_log(LogEntry::info(format!(
                "Listed {} tools",
                app.tools.len()
            )));
            app.status_line = format!("{} tools", app.tools.len());
        }
        WorkerEvent::CallResult(outcome) => {
            app.conn = ConnState::Ready;
            app.result_text = outcome.text;
            app.result_is_error = outcome.is_error;
            app.focus = Focus::Result;
            if outcome.is_error {
                app.push_log(LogEntry::error("Tool returned isError=true"));
            } else {
                app.push_log(LogEntry::success("Tool call finished"));
            }
        }
        WorkerEvent::Token(token) => {
            // Keep in memory for reconnect; also surface for the user.
            let preview = if token.len() > 12 {
                format!("{}…{}", &token[..6], &token[token.len() - 4..])
            } else {
                "(short token)".into()
            };
            app.opts.token = Some(token.clone());
            app.push_log(LogEntry::success(format!(
                "OAuth access token acquired ({preview}). Export MCP_TOKEN to reuse."
            )));
            app.push_log(LogEntry {
                level: LogLevel::Info,
                message: format!("MCP_TOKEN={token}"),
            });
            app.status_line = "Token acquired — connecting…".into();
        }
        WorkerEvent::Disconnected => {
            app.conn = ConnState::Disconnected;
            app.has_session = false;
            app.server = None;
            app.status_line = "Disconnected".into();
            app.push_log(LogEntry::warn("Disconnected"));
        }
        WorkerEvent::Error(msg) => {
            if matches!(app.conn, ConnState::Calling) {
                app.conn = ConnState::Ready;
            } else if matches!(
                app.conn,
                ConnState::Connecting | ConnState::Authenticating
            ) {
                app.conn = ConnState::Disconnected;
                app.has_session = false;
            }
            app.status_line = format!("Error: {msg}");
            app.push_log(LogEntry::error(msg));
        }
    }
}

fn handle_key(
    app: &mut App,
    key: KeyEvent,
    cmd_tx: &mpsc::UnboundedSender<WorkerCmd>,
) {
    if app.focus == Focus::Filter {
        match key.code {
            KeyCode::Esc => {
                app.focus = Focus::Tools;
            }
            KeyCode::Enter => {
                app.recompute_filter();
                app.focus = Focus::Tools;
            }
            KeyCode::Backspace => {
                app.filter.pop();
                app.recompute_filter();
            }
            KeyCode::Char(c) => {
                app.filter.push(c);
                app.recompute_filter();
            }
            _ => {}
        }
        return;
    }

    if app.editing_args && app.focus == Focus::Args {
        match key.code {
            KeyCode::Esc => {
                app.editing_args = false;
            }
            KeyCode::Backspace => {
                if !app.args_text.is_empty() {
                    app.args_text.pop();
                }
            }
            KeyCode::Enter => {
                app.args_text.push('\n');
            }
            KeyCode::Char(c) => {
                app.args_text.push(c);
            }
            _ => {}
        }
        return;
    }

    // Global / navigation
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            app.should_quit = true;
            let _ = cmd_tx.send(WorkerCmd::Disconnect);
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            let _ = cmd_tx.send(WorkerCmd::Disconnect);
        }
        KeyCode::Tab => {
            app.focus = match app.focus {
                Focus::Tools => Focus::Detail,
                Focus::Detail => Focus::Args,
                Focus::Args => Focus::Result,
                Focus::Result => Focus::Tools,
                Focus::Filter => Focus::Tools,
            };
        }
        KeyCode::BackTab => {
            app.focus = match app.focus {
                Focus::Tools => Focus::Result,
                Focus::Detail => Focus::Tools,
                Focus::Args => Focus::Detail,
                Focus::Result => Focus::Args,
                Focus::Filter => Focus::Tools,
            };
        }
        KeyCode::Char('1') => app.focus = Focus::Tools,
        KeyCode::Char('2') => app.focus = Focus::Detail,
        KeyCode::Char('3') => app.focus = Focus::Args,
        KeyCode::Char('4') => app.focus = Focus::Result,
        KeyCode::Char('c') => {
            let _ = cmd_tx.send(WorkerCmd::Connect);
        }
        KeyCode::Char('l') => {
            let _ = cmd_tx.send(WorkerCmd::Login);
        }
        KeyCode::Char('r') => {
            let _ = cmd_tx.send(WorkerCmd::RefreshTools);
        }
        KeyCode::Char('/') => {
            app.focus = Focus::Filter;
        }
        KeyCode::Char('e') => {
            app.focus = Focus::Args;
            app.editing_args = true;
        }
        KeyCode::Char('i') => {
            invoke_tool(app, cmd_tx);
        }
        KeyCode::Enter if app.focus == Focus::Tools => {
            app.select_tool_into_args();
        }
        KeyCode::Enter if app.focus == Focus::Args => {
            invoke_tool(app, cmd_tx);
        }
        KeyCode::Up | KeyCode::Char('k') if app.focus == Focus::Tools => {
            if app.selected > 0 {
                app.selected -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') if app.focus == Focus::Tools => {
            if !app.filtered_indices.is_empty()
                && app.selected + 1 < app.filtered_indices.len()
            {
                app.selected += 1;
            }
        }
        _ => {}
    }
}

fn invoke_tool(app: &mut App, cmd_tx: &mpsc::UnboundedSender<WorkerCmd>) {
    let Some(tool) = app.selected_tool() else {
        app.push_log(LogEntry::warn("No tool selected"));
        return;
    };
    let name = tool.name.clone();
    let args: serde_json::Value = match serde_json::from_str(&app.args_text) {
        Ok(v) => v,
        Err(e) => {
            app.push_log(LogEntry::error(format!("Invalid JSON args: {e}")));
            return;
        }
    };
    app.editing_args = false;
    app.conn = ConnState::Calling;
    app.status_line = format!("Calling {name}…");
    app.push_log(LogEntry::info(format!("tools/call {name}")));
    let _ = cmd_tx.send(WorkerCmd::CallTool { name, args });
}

async fn session_worker(
    mut opts: ConnectOptions,
    cmd_rx: &mut mpsc::UnboundedReceiver<WorkerCmd>,
    evt_tx: mpsc::UnboundedSender<WorkerEvent>,
) {
    let mut session: Option<McpSession> = None;

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            WorkerCmd::Shutdown => {
                if let Some(s) = session.take() {
                    let _ = s.disconnect().await;
                }
                break;
            }
            WorkerCmd::Disconnect => {
                if let Some(s) = session.take() {
                    let _ = s.disconnect().await;
                }
                let _ = evt_tx.send(WorkerEvent::Disconnected);
            }
            WorkerCmd::Login => {
                let _ =
                    evt_tx.send(WorkerEvent::Conn(ConnState::Authenticating));
                let _ = evt_tx.send(WorkerEvent::Status(
                    "OAuth login — check terminal for URL…".into(),
                ));
                let _ = evt_tx.send(WorkerEvent::Log(LogEntry::info(
                    "Starting OAuth PKCE login (browser)…",
                )));
                match login_oauth(&opts).await {
                    Ok(token) => {
                        opts.token = Some(token.clone());
                        let _ = evt_tx.send(WorkerEvent::Token(token));
                        // Auto-connect after login
                        match connect_session(&opts, &evt_tx).await {
                            Ok(s) => session = Some(s),
                            Err(e) => {
                                let _ = evt_tx.send(WorkerEvent::Error(e));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = evt_tx.send(WorkerEvent::Error(e.to_string()));
                    }
                }
            }
            WorkerCmd::Connect => {
                if let Some(s) = session.take() {
                    let _ = s.disconnect().await;
                }
                match connect_session(&opts, &evt_tx).await {
                    Ok(s) => session = Some(s),
                    Err(e) => {
                        let _ = evt_tx.send(WorkerEvent::Error(e));
                    }
                }
            }
            WorkerCmd::RefreshTools => {
                let Some(s) = session.as_ref() else {
                    let _ =
                        evt_tx.send(WorkerEvent::Error("Not connected".into()));
                    continue;
                };
                match s.list_tools().await {
                    Ok(tools) => {
                        let _ = evt_tx.send(WorkerEvent::Tools(tools));
                    }
                    Err(e) => {
                        let _ = evt_tx.send(WorkerEvent::Error(e.to_string()));
                    }
                }
            }
            WorkerCmd::CallTool { name, args } => {
                let Some(s) = session.as_ref() else {
                    let _ =
                        evt_tx.send(WorkerEvent::Error("Not connected".into()));
                    continue;
                };
                match s.call_tool(&name, args).await {
                    Ok(outcome) => {
                        let _ = evt_tx.send(WorkerEvent::CallResult(outcome));
                    }
                    Err(e) => {
                        let _ = evt_tx.send(WorkerEvent::Error(e.to_string()));
                    }
                }
            }
        }
    }
}

async fn connect_session(
    opts: &ConnectOptions,
    evt_tx: &mpsc::UnboundedSender<WorkerEvent>,
) -> std::result::Result<McpSession, String> {
    let _ = evt_tx.send(WorkerEvent::Conn(ConnState::Connecting));
    let _ = evt_tx
        .send(WorkerEvent::Status(format!("Connecting to {}…", opts.url)));
    let _ = evt_tx.send(WorkerEvent::Log(LogEntry::info(format!(
        "Connecting to {}",
        opts.url
    ))));

    let session = McpSession::connect(opts.clone())
        .await
        .map_err(|e| e.to_string())?;

    let server = session.server_status().clone();
    let _ = evt_tx.send(WorkerEvent::Connected {
        server: server.clone(),
    });

    match session.list_tools().await {
        Ok(tools) => {
            let _ = evt_tx.send(WorkerEvent::Tools(tools));
        }
        Err(e) => {
            let _ = evt_tx.send(WorkerEvent::Log(LogEntry::error(format!(
                "list_tools failed: {e}"
            ))));
        }
    }

    Ok(session)
}
