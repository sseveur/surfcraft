//! Editing session shared by the GUI and the automation/MCP interface.

mod asym_commands;
pub mod automation;
mod blank_cmd;
pub mod by_numbers;
pub mod commands;
pub mod compare;
pub mod document;
pub mod edit;
mod fins_cmd;
mod fit;
mod linalg;
pub mod measurements;
pub mod shaping;
mod shaping_commands;

pub use commands::{Context, ToolSpec, execute, tool_specs};
pub use document::{Document, write_with_backup};
