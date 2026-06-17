//! Hook input payload and its mapping to a logical event key.
//!
//! Claude Code sends a JSON object on standard input for each hook event. We
//! deserialize only the handful of fields we consume and map the
//! `hook_event_name`/`notification_type` pair to a [`LogicalEvent`], which is
//! the key used to look up per-event configuration.

use crate::Error;
use crate::Result;
use serde::Deserialize;

/// The subset of the Claude Code hook payload that `clamor` consumes.
///
/// Unknown fields in the JSON are ignored, so this stays forward-compatible
/// with new hook fields.
#[derive(Debug, Clone, Deserialize)]
pub struct HookInput {
    /// The hook event name, e.g. `"Notification"`, `"Stop"`, or
    /// `"SubagentStop"`.
    pub hook_event_name: String,

    /// For `Notification` events, the notification subtype, e.g.
    /// `"permission_prompt"` or `"idle_prompt"`.
    #[serde(default)]
    pub notification_type: Option<String>,

    /// For `Notification` events, the human-readable toast body (e.g. the
    /// permission request text).
    #[serde(default)]
    pub message: Option<String>,

    /// For `SubagentStop` events, the subagent type, e.g. `"Explore"`.
    #[serde(default)]
    pub agent_type: Option<String>,
}

impl HookInput {
    /// Parses a hook payload from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ParseInput`] if the input is not valid JSON or is
    /// missing the required `hook_event_name` field.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(Error::ParseInput)
    }

    /// Maps the payload to its [`LogicalEvent`] config key.
    ///
    /// `Notification` events are split by their `notification_type`;
    /// `Stop`/`SubagentStop` map directly. Any other combination becomes
    /// [`LogicalEvent::Other`], which is disabled by default.
    #[must_use]
    pub(crate) fn logical_event(&self) -> LogicalEvent {
        match self.hook_event_name.as_str() {
            "Notification" => match self.notification_type.as_deref() {
                Some("permission_prompt") => LogicalEvent::Permission,
                Some("idle_prompt") => LogicalEvent::Idle,
                Some(other) => LogicalEvent::Other(other.to_owned()),
                None => LogicalEvent::Other("notification".to_owned()),
            },
            "Stop" => LogicalEvent::Stop,
            "SubagentStop" => LogicalEvent::SubagentStop,
            other => LogicalEvent::Other(other.to_owned()),
        }
    }
}

/// A logical event key derived from a [`HookInput`].
///
/// This is the section name used in the configuration file (e.g.
/// `[events.permission]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LogicalEvent {
    /// `Notification` + `permission_prompt`.
    Permission,
    /// `Notification` + `idle_prompt`.
    Idle,
    /// `Stop`.
    Stop,
    /// `SubagentStop`.
    SubagentStop,
    /// Any other event or notification subtype; disabled by default. The
    /// string is the config key (the `notification_type`, or the
    /// `hook_event_name` for unknown events).
    Other(String),
}

impl LogicalEvent {
    /// The configuration section key for this event.
    #[must_use]
    pub(crate) fn config_key(&self) -> &str {
        match self {
            LogicalEvent::Permission => "permission",
            LogicalEvent::Idle => "idle",
            LogicalEvent::Stop => "stop",
            LogicalEvent::SubagentStop => "subagent_stop",
            LogicalEvent::Other(key) => key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_permission_notification() {
        let json = r#"{
            "hook_event_name": "Notification",
            "notification_type": "permission_prompt",
            "message": "Bash(npm test)"
        }"#;
        let input = HookInput::from_json(json).expect("valid payload");
        assert_eq!(input.hook_event_name, "Notification");
        assert_eq!(input.message.as_deref(), Some("Bash(npm test)"));
        assert_eq!(input.logical_event(), LogicalEvent::Permission);
    }

    #[test]
    fn ignores_unknown_fields() {
        let json = r#"{
            "hook_event_name": "Stop",
            "session_id": "abc",
            "transcript_path": "/tmp/x",
            "cwd": "/repo"
        }"#;
        let input = HookInput::from_json(json).expect("valid payload");
        assert_eq!(input.logical_event(), LogicalEvent::Stop);
    }

    #[test]
    fn maps_idle_prompt() {
        let input = HookInput::from_json(
            r#"{"hook_event_name":"Notification","notification_type":"idle_prompt"}"#,
        )
        .expect("valid payload");
        assert_eq!(input.logical_event(), LogicalEvent::Idle);
    }

    #[test]
    fn maps_subagent_stop_with_agent_type() {
        let input =
            HookInput::from_json(r#"{"hook_event_name":"SubagentStop","agent_type":"Explore"}"#)
                .expect("valid payload");
        assert_eq!(input.logical_event(), LogicalEvent::SubagentStop);
        assert_eq!(input.agent_type.as_deref(), Some("Explore"));
    }

    #[test]
    fn unknown_notification_type_becomes_other_with_its_name() {
        let input = HookInput::from_json(
            r#"{"hook_event_name":"Notification","notification_type":"auth_success"}"#,
        )
        .expect("valid payload");
        assert_eq!(
            input.logical_event(),
            LogicalEvent::Other("auth_success".to_owned())
        );
        assert_eq!(input.logical_event().config_key(), "auth_success");
    }

    #[test]
    fn unknown_event_becomes_other() {
        let input =
            HookInput::from_json(r#"{"hook_event_name":"PreToolUse"}"#).expect("valid payload");
        assert_eq!(
            input.logical_event(),
            LogicalEvent::Other("PreToolUse".to_owned())
        );
    }

    #[test]
    fn notification_without_type_is_other() {
        let input =
            HookInput::from_json(r#"{"hook_event_name":"Notification"}"#).expect("valid payload");
        assert_eq!(input.logical_event().config_key(), "notification");
    }

    #[test]
    fn rejects_invalid_json() {
        HookInput::from_json("not json").expect_err("invalid json should be rejected");
    }

    #[test]
    fn config_keys_match_known_events() {
        assert_eq!(LogicalEvent::Permission.config_key(), "permission");
        assert_eq!(LogicalEvent::Idle.config_key(), "idle");
        assert_eq!(LogicalEvent::Stop.config_key(), "stop");
        assert_eq!(LogicalEvent::SubagentStop.config_key(), "subagent_stop");
    }
}
