//! Editing session shared by the GUI and the automation/MCP interface.

pub mod automation;
pub mod commands;
pub mod document;
pub mod edit;
pub mod shaping;
mod shaping_commands;

pub use commands::{Context, ToolSpec, execute, tool_specs};
pub use document::{Document, write_with_backup};
