//! The hook payload read from standard input.
//!
//! Claude Code sends a JSON object on standard input for each hook event. The
//! only field `clamor` consumes is the human-readable `message`, used as the
//! notification body when the caller does not pass an explicit one. Routing
//! (which event, which subtype) is handled entirely by `settings.json` hook
//! matchers, so it is not parsed here.

use crate::Error;
use crate::Result;
use serde::Deserialize;

/// The subset of the Claude Code hook payload that `clamor` consumes.
///
/// Unknown fields in the JSON are ignored, so this stays forward-compatible
/// with new hook fields.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HookInput {
    /// The human-readable notification text (e.g. the permission request),
    /// present on `Notification` events. Used as the toast body unless the
    /// caller supplies its own.
    #[serde(default)]
    pub message: Option<String>,
}

impl HookInput {
    /// Parses a hook payload from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ParseInput`] if the input is not valid JSON.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(Error::ParseInput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_message() {
        let input = HookInput::from_json(
            r#"{"hook_event_name":"Notification","notification_type":"permission_prompt","message":"Bash(npm test)"}"#,
        )
        .expect("valid payload");
        assert_eq!(input.message.as_deref(), Some("Bash(npm test)"));
    }

    #[test]
    fn ignores_unknown_fields() {
        let input = HookInput::from_json(
            r#"{"hook_event_name":"Stop","session_id":"abc","transcript_path":"/tmp/x","cwd":"/repo"}"#,
        )
        .expect("valid payload");
        assert_eq!(input.message, None);
    }

    #[test]
    fn missing_message_is_none() {
        let input = HookInput::from_json("{}").expect("valid payload");
        assert_eq!(input.message, None);
    }

    #[test]
    fn rejects_invalid_json() {
        HookInput::from_json("not json").expect_err("invalid json should be rejected");
    }
}
