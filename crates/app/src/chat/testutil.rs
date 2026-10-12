//! Helpers shared by the unit tests of this module tree.

use pando_client::ChatModel;
use pando_client::chat::{ChatEvent, ToolCallEvent, ToolResultEvent};
use pando_client::model::{ItemKind, ToolItem};

/// Builds a [`ToolItem`] the only way the model allows: by applying events.
pub fn tool(call: ToolCallEvent, result: Option<ToolResultEvent>) -> ToolItem {
    let mut model = ChatModel::default();
    model.apply(&ChatEvent::ToolCall(call));
    if let Some(r) = result {
        model.apply(&ChatEvent::ToolResult(r));
    }
    match model.items().first().map(|i| i.kind.clone()) {
        Some(ItemKind::Tool(t)) => t,
        other => panic!("expected a tool item, got {other:?}"),
    }
}
