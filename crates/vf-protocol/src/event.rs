//! One normalized event of a running task — the same shape for every provider
//! (Claude Code, Codex, GitHub Copilot, the OpenAI-compatible loop).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Token usage reported by a provider.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Usage {
    /// Input tokens (including cached).
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
}

/// A normalized event, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// Session started.
    Init {
        /// Provider's own session id (for native resume), if any.
        session: Option<String>,
        /// Model actually used.
        model: Option<String>,
        /// Which CLI / loop runs it.
        cli: String,
        /// MCP servers loaded by the CLI (Claude Code only).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mcp: Option<u32>,
        /// Skills loaded by the CLI (Claude Code only).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        skills: Option<u32>,
    },
    /// Text from the model.
    Text {
        /// The text.
        text: String,
    },
    /// The model calls a tool (shell, edit, read…).
    Tool {
        /// Tool name.
        tool: String,
        /// Short human-readable detail (command, file path).
        detail: String,
    },
    /// Result of a tool call.
    ToolResult {
        /// Whether the tool failed.
        error: bool,
        /// Output, truncated.
        text: String,
    },
    /// The model finished.
    Result {
        /// Success flag.
        ok: bool,
        /// Final message.
        text: String,
        /// Model turns.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turns: Option<u32>,
        /// Duration, milliseconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<u64>,
        /// Cost reported by the provider (API prices), USD.
        #[serde(default, rename = "costUsd", skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
        /// Token usage.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
    },
    /// An error.
    Error {
        /// Message.
        text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_tagged_by_kind() -> Result<(), serde_json::Error> {
        let json = serde_json::to_string(&Event::Text { text: "hi".into() })?;
        assert_eq!(json, r#"{"kind":"text","text":"hi"}"#);
        let back: Event = serde_json::from_str(r#"{"kind":"tool","tool":"shell","detail":"ls"}"#)?;
        assert_eq!(
            back,
            Event::Tool {
                tool: "shell".into(),
                detail: "ls".into()
            }
        );
        let result = Event::Result {
            ok: true,
            text: String::new(),
            turns: None,
            ms: Some(5),
            cost_usd: Some(0.5),
            usage: None,
        };
        assert_eq!(
            serde_json::to_string(&result)?,
            r#"{"kind":"result","ok":true,"text":"","ms":5,"costUsd":0.5}"#
        );
        Ok(())
    }
}
