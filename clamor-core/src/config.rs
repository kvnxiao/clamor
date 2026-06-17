//! Configuration model, loading, and per-event resolution.
//!
//! Configuration is TOML. The location is resolved in this order, first found
//! wins (no merging):
//!
//! 1. `$CLAMOR_CONFIG` (explicit path)
//! 2. `$CLAUDE_PROJECT_DIR/.clamor.toml`
//! 3. the user config dir (`~/.config/clamor/config.toml`,
//!    `%APPDATA%\clamor\config.toml`, etc.)
//! 4. built-in defaults

use crate::Error;
use crate::Result;
use crate::event::LogicalEvent;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

/// The default config file contents written by `clamor init`.
pub const DEFAULT_CONFIG_TOML: &str = r#"[notifications]
enabled  = true            # master switch
app_name = "Claude Code"   # toast app name / Windows AUMID display label

[events.permission]
enabled = true
title   = "Permission needed"   # body defaults to the hook `message`
sound   = "native"              # "native" | "none" | { file = "/path/chime.wav" }

[events.idle]
enabled = true
title   = "Waiting for you"
sound   = "native"

[events.stop]
enabled = true
title   = "Task complete"
sound   = "native"

[events.subagent_stop]
enabled = false
title   = "Subagent done"
sound   = "none"
"#;

/// Top-level configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Global notification settings.
    #[serde(default)]
    pub notifications: Notifications,

    /// Per-event overrides, keyed by logical event (e.g. `permission`).
    #[serde(default)]
    pub events: BTreeMap<String, EventConfig>,
}

/// Global notification settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Notifications {
    /// Master switch. When `false`, `clamor` shows nothing.
    pub enabled: bool,

    /// Application name shown on the toast and used as the Windows AUMID
    /// display label.
    pub app_name: String,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            enabled: true,
            app_name: "Claude Code".to_owned(),
        }
    }
}

/// Per-event overrides. Omitted fields fall back to the built-in default for
/// that event.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventConfig {
    /// Whether this event fires a notification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// The notification title (summary line).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Which sound to play.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<SoundConfig>,
}

/// How a notification should sound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SoundConfig {
    /// A keyword: `"native"` or `"none"`.
    Keyword(SoundKeyword),

    /// A custom audio file: `{ file = "/path/to/chime.wav" }`.
    File {
        /// Path to the audio file to play.
        file: Utf8PathBuf,
    },
}

/// The sound keyword variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundKeyword {
    /// Play the platform's native notification sound.
    Native,
    /// Play no sound.
    None,
}

/// A fully-resolved event configuration: built-in defaults with any user
/// overrides applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedEvent {
    /// Whether the event fires a notification.
    pub enabled: bool,
    /// The notification title.
    pub title: String,
    /// The resolved sound choice.
    pub sound: SoundConfig,
}

impl Config {
    /// Loads configuration from the first location that exists, falling back
    /// to built-in defaults.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if a located file cannot be read, or
    /// [`Error::ParseConfig`] if it is not valid TOML.
    pub fn load() -> Result<Self> {
        let path = resolve_config_path(
            std::env::var("CLAMOR_CONFIG").ok(),
            std::env::var("CLAUDE_PROJECT_DIR").ok(),
            user_config_path(),
            Utf8Path::exists,
        );
        match path {
            Some(path) => Self::from_file(&path),
            None => Ok(Self::default()),
        }
    }

    /// Loads configuration from a specific TOML file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be read, or
    /// [`Error::ParseConfig`] if it is not valid TOML.
    pub fn from_file(path: &Utf8Path) -> Result<Self> {
        let text = fs_err::read_to_string(path)?;
        toml::from_str(&text).map_err(|source| Error::ParseConfig {
            path: path.to_owned(),
            source,
        })
    }

    /// Resolves the configuration for an event by applying any user override
    /// on top of the built-in default.
    pub(crate) fn resolve_event(&self, event: &LogicalEvent) -> ResolvedEvent {
        let mut resolved = builtin(event);
        if let Some(user) = self.events.get(event.config_key()) {
            if let Some(enabled) = user.enabled {
                resolved.enabled = enabled;
            }
            if let Some(title) = &user.title {
                resolved.title.clone_from(title);
            }
            if let Some(sound) = &user.sound {
                resolved.sound = sound.clone();
            }
        }
        resolved
    }
}

/// The built-in default configuration for an event.
fn builtin(event: &LogicalEvent) -> ResolvedEvent {
    let (enabled, title, sound) = match event {
        LogicalEvent::Permission => (true, "Permission needed", SoundKeyword::Native),
        LogicalEvent::Idle => (true, "Waiting for you", SoundKeyword::Native),
        LogicalEvent::Stop => (true, "Task complete", SoundKeyword::Native),
        LogicalEvent::SubagentStop => (false, "Subagent done", SoundKeyword::None),
        LogicalEvent::Other(_) => (false, "Claude Code", SoundKeyword::None),
    };
    ResolvedEvent {
        enabled,
        title: title.to_owned(),
        sound: SoundConfig::Keyword(sound),
    }
}

/// The path to the user config file (`<user config dir>/clamor/config.toml`),
/// if a user config directory can be determined for this platform.
#[must_use]
pub fn user_config_path() -> Option<Utf8PathBuf> {
    let base = directories::BaseDirs::new()?;
    let dir = Utf8Path::from_path(base.config_dir())?;
    Some(dir.join("clamor").join("config.toml"))
}

/// Returns the first config path that exists, in lookup order.
fn resolve_config_path(
    clamor_config: Option<String>,
    project_dir: Option<String>,
    user_config: Option<Utf8PathBuf>,
    exists: impl Fn(&Utf8Path) -> bool,
) -> Option<Utf8PathBuf> {
    [
        clamor_config.map(Utf8PathBuf::from),
        project_dir.map(|dir| Utf8PathBuf::from(dir).join(".clamor.toml")),
        user_config,
    ]
    .into_iter()
    .flatten()
    .find(|path| exists(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_master_switch_on() {
        let config = Config::default();
        assert!(config.notifications.enabled);
        assert_eq!(config.notifications.app_name, "Claude Code");
        assert!(config.events.is_empty());
    }

    #[test]
    fn defaults_applied_when_sections_omitted() {
        // Empty config: every event resolves to its built-in default.
        let config = Config::default();
        let permission = config.resolve_event(&LogicalEvent::Permission);
        assert!(permission.enabled);
        assert_eq!(permission.title, "Permission needed");
        assert_eq!(permission.sound, SoundConfig::Keyword(SoundKeyword::Native));

        let subagent = config.resolve_event(&LogicalEvent::SubagentStop);
        assert!(!subagent.enabled);
        assert_eq!(subagent.sound, SoundConfig::Keyword(SoundKeyword::None));
    }

    #[test]
    fn partial_event_override_keeps_other_defaults() {
        // Only `title` is set; `enabled` and `sound` keep the built-in default.
        let toml = r#"
            [events.stop]
            title = "Done!"
        "#;
        let config: Config = toml::from_str(toml).expect("valid toml");
        let stop = config.resolve_event(&LogicalEvent::Stop);
        assert_eq!(stop.title, "Done!");
        assert!(stop.enabled, "enabled should keep its built-in default");
        assert_eq!(stop.sound, SoundConfig::Keyword(SoundKeyword::Native));
    }

    #[test]
    fn partial_notifications_section_keeps_app_name_default() {
        let config: Config =
            toml::from_str("[notifications]\nenabled = false\n").expect("valid toml");
        assert!(!config.notifications.enabled);
        assert_eq!(config.notifications.app_name, "Claude Code");
    }

    #[test]
    fn default_template_parses_and_matches_builtins() {
        let config: Config = toml::from_str(DEFAULT_CONFIG_TOML).expect("template is valid toml");
        // The shipped template should resolve identically to the built-ins.
        for event in [
            LogicalEvent::Permission,
            LogicalEvent::Idle,
            LogicalEvent::Stop,
            LogicalEvent::SubagentStop,
        ] {
            assert_eq!(config.resolve_event(&event), builtin(&event));
        }
    }

    #[test]
    fn toml_round_trip_preserves_config() {
        let config: Config = toml::from_str(DEFAULT_CONFIG_TOML).expect("valid toml");
        let serialized = toml::to_string(&config).expect("serializable");
        let reparsed: Config = toml::from_str(&serialized).expect("re-parseable");
        for event in [LogicalEvent::Permission, LogicalEvent::SubagentStop] {
            assert_eq!(config.resolve_event(&event), reparsed.resolve_event(&event));
        }
    }

    #[test]
    fn sound_config_parses_native_keyword() {
        let config: Config =
            toml::from_str("[events.stop]\nsound = \"native\"\n").expect("valid toml");
        assert_eq!(
            config.events.get("stop").and_then(|e| e.sound.clone()),
            Some(SoundConfig::Keyword(SoundKeyword::Native))
        );
    }

    #[test]
    fn sound_config_parses_none_keyword() {
        let config: Config =
            toml::from_str("[events.stop]\nsound = \"none\"\n").expect("valid toml");
        assert_eq!(
            config.events.get("stop").and_then(|e| e.sound.clone()),
            Some(SoundConfig::Keyword(SoundKeyword::None))
        );
    }

    #[test]
    fn sound_config_parses_file_table() {
        let config: Config =
            toml::from_str("[events.stop]\nsound = { file = \"/tmp/chime.wav\" }\n")
                .expect("valid toml");
        assert_eq!(
            config.events.get("stop").and_then(|e| e.sound.clone()),
            Some(SoundConfig::File {
                file: Utf8PathBuf::from("/tmp/chime.wav"),
            })
        );
    }

    #[test]
    fn sound_config_rejects_unknown_keyword() {
        toml::from_str::<Config>("[events.stop]\nsound = \"loud\"\n")
            .expect_err("unknown sound keyword should be rejected");
    }

    #[test]
    fn lookup_prefers_clamor_config_env() {
        let path = resolve_config_path(
            Some("/explicit/clamor.toml".to_owned()),
            Some("/project".to_owned()),
            Some(Utf8PathBuf::from("/home/user/.config/clamor/config.toml")),
            |_| true,
        );
        assert_eq!(
            path.as_deref(),
            Some(Utf8Path::new("/explicit/clamor.toml"))
        );
    }

    #[test]
    fn lookup_falls_through_to_project_dir_when_explicit_missing() {
        let path = resolve_config_path(
            Some("/explicit/clamor.toml".to_owned()),
            Some("/project".to_owned()),
            Some(Utf8PathBuf::from("/home/user/.config/clamor/config.toml")),
            |p| p != Utf8Path::new("/explicit/clamor.toml"),
        );
        assert_eq!(
            path.as_deref(),
            Some(Utf8Path::new("/project/.clamor.toml"))
        );
    }

    #[test]
    fn lookup_falls_through_to_user_dir() {
        let user = Utf8PathBuf::from("/home/user/.config/clamor/config.toml");
        let path = resolve_config_path(None, None, Some(user.clone()), |p| p == user.as_path());
        assert_eq!(path, Some(user));
    }

    #[test]
    fn lookup_returns_none_when_nothing_exists() {
        let path = resolve_config_path(
            Some("/a".to_owned()),
            Some("/b".to_owned()),
            Some(Utf8PathBuf::from("/c")),
            |_| false,
        );
        assert_eq!(path, None);
    }
}
