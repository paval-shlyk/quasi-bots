//! MCP 2025-11-25 Streamable HTTP client library.
//!
//! Use [`McpSession`] to connect, list tools, and call tools against servers
//! such as skill-master. The companion binary adds a ratatui verification TUI.

pub mod client;
pub mod config;
pub mod error;
pub mod model;
pub mod tui;

pub use client::{McpSession, login_oauth};
pub use config::{ConnectOptions, empty_args_from_schema};
pub use error::{Error, Result};
pub use model::{CallOutcome, LogEntry, LogLevel, ServerStatus, ToolView};
