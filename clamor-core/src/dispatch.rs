//! Hook dispatch: resolve a hook event against config and fire the result.

use crate::Result;
use crate::audio;
use crate::config::Config;
use crate::config::ResolvedEvent;
use crate::config::SoundConfig;
use crate::config::SoundKeyword;
use crate::event::HookInput;
use crate::event::LogicalEvent;
use crate::notify;
use crate::notify::NativeSound;
use crate::notify::NotificationSpec;
use camino::Utf8PathBuf;

/// The resolved action for a hook event.
#[derive(Debug, PartialEq, Eq)]
struct DispatchPlan {
    /// The notification to show.
    spec: NotificationSpec,
    /// A custom audio file to play after showing the (silent) notification.
    custom_audio: Option<Utf8PathBuf>,
}

/// Whether a previewed event would also fire on a real hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOutcome {
    /// The event is enabled, so real hooks will fire it too.
    WouldFire,
    /// The event, or the master switch, is disabled. The notification was shown
    /// for preview only; real hooks will not fire it.
    Disabled,
}

/// Dispatches a hook event: load config, decide what to do, and do it.
///
/// # Errors
///
/// Returns an error if config loading, showing the notification, or playing a
/// custom sound fails. Callers in hook mode should swallow the error and exit
/// zero so the notifier never blocks the agent loop.
pub fn dispatch(input: &HookInput) -> Result<()> {
    let config = Config::load()?;
    if let Some(plan) = plan(input, &config) {
        execute(plan)?;
    }
    Ok(())
}

/// Previews the notification for an event, showing it even when the event is
/// disabled, so `clamor test` can verify the toast and sound. Returns whether
/// the event would also fire on a real hook.
///
/// # Errors
///
/// Returns an error if config loading, showing the notification, or playing a
/// custom sound fails.
pub fn dispatch_test(input: &HookInput) -> Result<TestOutcome> {
    let config = Config::load()?;
    let event = input.logical_event();
    let resolved = config.resolve_event(&event);
    let outcome = if config.notifications.enabled && resolved.enabled {
        TestOutcome::WouldFire
    } else {
        TestOutcome::Disabled
    };
    execute(build_plan(
        &event,
        resolved,
        input,
        &config.notifications.app_name,
    ))?;
    Ok(outcome)
}

/// Pure resolution: decide the plan for a hook input, or `None` when nothing
/// should fire (master switch off, or the event is disabled).
fn plan(input: &HookInput, config: &Config) -> Option<DispatchPlan> {
    if !config.notifications.enabled {
        return None;
    }
    let event = input.logical_event();
    let resolved = config.resolve_event(&event);
    if !resolved.enabled {
        return None;
    }
    Some(build_plan(
        &event,
        resolved,
        input,
        &config.notifications.app_name,
    ))
}

/// Builds the notification plan from a resolved event. Does not consider
/// whether the event is enabled; callers gate on that.
fn build_plan(
    event: &LogicalEvent,
    resolved: ResolvedEvent,
    input: &HookInput,
    app_name: &str,
) -> DispatchPlan {
    let (sound, custom_audio) = match resolved.sound {
        SoundConfig::Keyword(SoundKeyword::Native) => (NativeSound::Default, None),
        SoundConfig::Keyword(SoundKeyword::None) => (NativeSound::Silent, None),
        SoundConfig::File { file } => (NativeSound::Silent, Some(file)),
    };
    DispatchPlan {
        spec: NotificationSpec {
            app_name: app_name.to_owned(),
            title: resolved.title,
            body: body_for(event, input),
            sound,
        },
        custom_audio,
    }
}

/// Shows the notification and plays any custom audio.
fn execute(plan: DispatchPlan) -> Result<()> {
    notify::show(&plan.spec)?;
    if let Some(path) = plan.custom_audio {
        audio::play_file(&path)?;
    }
    Ok(())
}

/// The notification body for an event. `Notification` events use the hook
/// `message`; `Stop`/`SubagentStop` use a sensible default.
fn body_for(event: &LogicalEvent, input: &HookInput) -> String {
    match event {
        LogicalEvent::Stop => "Claude Code has finished responding.".to_owned(),
        LogicalEvent::SubagentStop => match input.agent_type.as_deref() {
            Some(agent) => format!("{agent} subagent finished."),
            None => "Subagent finished.".to_owned(),
        },
        LogicalEvent::Permission | LogicalEvent::Idle | LogicalEvent::Other(_) => {
            input.message.clone().unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;

    fn input(json: &str) -> HookInput {
        HookInput::from_json(json).expect("valid payload")
    }

    #[test]
    fn permission_uses_message_as_body() {
        let config = Config::default();
        let payload = input(
            r#"{"hook_event_name":"Notification","notification_type":"permission_prompt","message":"Bash(npm test)"}"#,
        );
        let plan = plan(&payload, &config).expect("permission fires by default");
        assert_eq!(plan.spec.title, "Permission needed");
        assert_eq!(plan.spec.body, "Bash(npm test)");
        assert_eq!(plan.spec.sound, NativeSound::Default);
        assert_eq!(plan.custom_audio, None);
    }

    #[test]
    fn stop_uses_default_body() {
        let config = Config::default();
        let plan = plan(&input(r#"{"hook_event_name":"Stop"}"#), &config).expect("stop fires");
        assert_eq!(plan.spec.title, "Task complete");
        assert_eq!(plan.spec.body, "Claude Code has finished responding.");
    }

    #[test]
    fn subagent_stop_body_includes_agent_type() {
        let config: Config =
            toml::from_str("[events.subagent_stop]\nenabled = true\n").expect("valid toml");
        let plan = plan(
            &input(r#"{"hook_event_name":"SubagentStop","agent_type":"Explore"}"#),
            &config,
        )
        .expect("subagent fires when enabled");
        assert_eq!(plan.spec.body, "Explore subagent finished.");
    }

    #[test]
    fn subagent_stop_without_agent_type_uses_generic_body() {
        let config: Config =
            toml::from_str("[events.subagent_stop]\nenabled = true\n").expect("valid toml");
        let plan = plan(&input(r#"{"hook_event_name":"SubagentStop"}"#), &config)
            .expect("subagent fires when enabled");
        assert_eq!(plan.spec.body, "Subagent finished.");
    }

    #[test]
    fn subagent_stop_disabled_by_default() {
        let config = Config::default();
        assert!(plan(&input(r#"{"hook_event_name":"SubagentStop"}"#), &config).is_none());
    }

    #[test]
    fn build_plan_resolves_disabled_event_for_preview() {
        // subagent_stop is disabled by default, but `clamor test` previews it
        // anyway, so build_plan must still resolve its title/body/sound.
        let config = Config::default();
        let payload = input(r#"{"hook_event_name":"SubagentStop","agent_type":"Explore"}"#);
        let event = payload.logical_event();
        let resolved = config.resolve_event(&event);
        assert!(!resolved.enabled, "subagent_stop is disabled by default");
        let plan = build_plan(&event, resolved, &payload, &config.notifications.app_name);
        assert_eq!(plan.spec.title, "Subagent done");
        assert_eq!(plan.spec.body, "Explore subagent finished.");
        assert_eq!(plan.spec.sound, NativeSound::Silent);
    }

    #[test]
    fn master_switch_off_suppresses_everything() {
        let config: Config =
            toml::from_str("[notifications]\nenabled = false\n").expect("valid toml");
        assert!(plan(&input(r#"{"hook_event_name":"Stop"}"#), &config).is_none());
    }

    #[test]
    fn disabled_event_is_suppressed() {
        let config: Config =
            toml::from_str("[events.stop]\nenabled = false\n").expect("valid toml");
        assert!(plan(&input(r#"{"hook_event_name":"Stop"}"#), &config).is_none());
    }

    #[test]
    fn none_sound_is_silent_without_custom_audio() {
        let config: Config =
            toml::from_str("[events.stop]\nsound = \"none\"\n").expect("valid toml");
        let plan = plan(&input(r#"{"hook_event_name":"Stop"}"#), &config).expect("stop fires");
        assert_eq!(plan.spec.sound, NativeSound::Silent);
        assert_eq!(plan.custom_audio, None);
    }

    #[test]
    fn file_sound_is_silent_with_custom_audio() {
        let config: Config =
            toml::from_str("[events.stop]\nsound = { file = \"/tmp/chime.wav\" }\n")
                .expect("valid toml");
        let plan = plan(&input(r#"{"hook_event_name":"Stop"}"#), &config).expect("stop fires");
        assert_eq!(plan.spec.sound, NativeSound::Silent);
        assert_eq!(
            plan.custom_audio.as_deref(),
            Some(Utf8Path::new("/tmp/chime.wav"))
        );
    }

    #[test]
    fn idle_uses_message_body() {
        let config = Config::default();
        let plan = plan(
            &input(
                r#"{"hook_event_name":"Notification","notification_type":"idle_prompt","message":"waiting"}"#,
            ),
            &config,
        )
        .expect("idle fires by default");
        assert_eq!(plan.spec.title, "Waiting for you");
        assert_eq!(plan.spec.body, "waiting");
    }
}
