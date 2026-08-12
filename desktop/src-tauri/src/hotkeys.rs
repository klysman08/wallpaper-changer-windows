//! Global hotkeys, owned by Rust.
//!
//! Registration lives here rather than in the Python sidecar on purpose: the
//! shortcuts must keep working across a sidecar restart, and the window-toggle
//! binding has to reach the Tauri window, which the sidecar cannot touch.
//!
//! Bindings are stored in `settings.toml` in the syntax the old GUI used
//! (`ctrl+alt+right`, `alt+a`, `ctrl+alt+.`). [`parse_shortcut`] translates that into
//! the plugin's `Shortcut` type; everything else is a straight mapping from a config
//! key to an engine method.

use std::str::FromStr;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::engine::Engine;

/// Emitted after hotkey-triggered work so the UI can refresh without polling.
pub const HOTKEY_EVENT: &str = "hotkey-fired";

/// What each `[hotkeys]` config key does, as an engine method plus its params.
///
/// `toggle_window` is absent: it is handled in-process because it acts on the Tauri
/// window rather than the engine.
fn action_for(binding: &str) -> Option<(&'static str, Value)> {
    let empty = json!({});
    Some(match binding {
        "next_wallpaper" => ("apply_wallpaper", empty),
        "prev_wallpaper" => ("apply_previous_wallpaper", empty),
        "stop_watch" => ("watch_toggle", empty),
        "default_wallpaper" => ("apply_default_wallpaper", empty),
        "toggle_transparency" => ("toggle_foreground_opacity", empty),
        "effect_normal" => ("set_effect", json!({ "effect": "normal" })),
        "effect_bw" => ("set_effect", json!({ "effect": "bw" })),
        "effect_vintage" => ("set_effect", json!({ "effect": "vintage" })),
        "effect_hdr" => ("set_effect", json!({ "effect": "hdr" })),
        "toggle_video" => ("video_toggle", empty),
        "toggle_video_sound" => ("video_toggle_sound", empty),
        "next_video" => ("video_next", empty),
        "prev_video" => ("video_prev", empty),
        _ => return None,
    })
}

/// Translate a stored binding such as `ctrl+alt+right` into a [`Shortcut`].
///
/// The engine's syntax is lowercase words joined by `+`; the plugin expects
/// `keyboard-types` code names (`ArrowRight`, `KeyA`, `Digit1`, `Period`).
pub fn parse_shortcut(combo: &str) -> Option<Shortcut> {
    let combo = combo.trim().to_lowercase();
    if combo.is_empty() {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();
    for token in combo.split('+') {
        let token = token.trim();
        if token.is_empty() {
            return None;
        }
        parts.push(match token {
            "ctrl" | "control" => "Control".into(),
            "alt" => "Alt".into(),
            "shift" => "Shift".into(),
            "win" | "windows" | "super" | "meta" => "Super".into(),
            other => key_code(other)?,
        });
    }
    Shortcut::from_str(&parts.join("+")).ok()
}

/// Map a single non-modifier key to its `keyboard-types` code name.
fn key_code(key: &str) -> Option<String> {
    let named = match key {
        "right" => "ArrowRight",
        "left" => "ArrowLeft",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "." | "period" => "Period",
        "," | "comma" => "Comma",
        "-" | "minus" => "Minus",
        "=" | "equal" => "Equal",
        "/" | "slash" => "Slash",
        "\\" | "backslash" => "Backslash",
        ";" | "semicolon" => "Semicolon",
        "'" | "quote" => "Quote",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        "`" => "Backquote",
        "space" => "Space",
        "enter" | "return" => "Enter",
        "tab" => "Tab",
        "esc" | "escape" => "Escape",
        "backspace" => "Backspace",
        "delete" | "del" => "Delete",
        "insert" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.to_string());
    }

    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        // Single letter or digit -> KeyA / Digit1
        (Some(c), None) if c.is_ascii_alphabetic() => {
            Some(format!("Key{}", c.to_ascii_uppercase()))
        }
        (Some(c), None) if c.is_ascii_digit() => Some(format!("Digit{c}")),
        // Function keys keep their name.
        _ if key.starts_with('f') && key[1..].chars().all(|c| c.is_ascii_digit()) => {
            Some(format!("F{}", &key[1..]))
        }
        _ => None,
    }
}

/// Register every binding in `hotkeys`, replacing whatever was registered before.
///
/// Returns a human-readable line per binding that could not be registered, saying
/// which of the two reasons applied. "Already in use" is the common one and is not a
/// bug: Windows gives a global hotkey to a single owner, so another application
/// holding it — including an older build of this app — wins.
pub fn register_all(app: &AppHandle, hotkeys: &Value) -> Vec<String> {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();

    let mut failed = Vec::new();
    let Some(map) = hotkeys.as_object() else {
        return failed;
    };

    for (binding, value) in map {
        // Not a shortcut: it selects the modifier for scroll-wheel transparency.
        if binding == "scroll_modifier" {
            continue;
        }
        let Some(combo) = value.as_str().filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        if binding != "toggle_window" && action_for(binding).is_none() {
            continue;
        }
        let Some(shortcut) = parse_shortcut(combo) else {
            failed.push(format!("{binding} ({combo}): not a valid combination"));
            continue;
        };

        let handle = app.clone();
        let name = binding.clone();
        let registered = manager.on_shortcut(shortcut, move |_app, _shortcut, event| {
            // Fire on press only; without this each hotkey runs twice.
            if event.state() != ShortcutState::Pressed {
                return;
            }
            dispatch(handle.clone(), name.clone());
        });
        if registered.is_err() {
            failed.push(format!(
                "{binding} ({combo}): already in use by another app"
            ));
        }
    }
    failed
}

/// Run the action bound to `binding`, off the hotkey callback thread.
fn dispatch(app: AppHandle, binding: String) {
    if binding == "toggle_window" {
        toggle_window(&app);
        return;
    }
    let Some((method, params)) = action_for(&binding) else {
        return;
    };

    tauri::async_runtime::spawn(async move {
        let Some(engine) = app.try_state::<Engine>() else {
            log::warn!("hotkey {binding} fired but the engine is not running");
            return;
        };
        match engine.call(method, params).await {
            Ok(result) => {
                let _ = app.emit(
                    HOTKEY_EVENT,
                    json!({ "binding": binding, "result": result }),
                );
            }
            // A failed hotkey is normal (no previous wallpaper, no videos), so this
            // is a log line and a UI event, not an error dialog.
            Err(e) => {
                log::info!("hotkey {binding} did not run: {e}");
                let _ = app.emit(HOTKEY_EVENT, json!({ "binding": binding, "error": e }));
            }
        }
    });
}

/// Show and focus the main window, or hide it if it is already showing.
pub fn toggle_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_bindings_shipped_in_the_default_config() {
        for combo in [
            "ctrl+alt+right",
            "ctrl+alt+left",
            "ctrl+alt+s",
            "ctrl+alt+d",
            "alt+a",
            "ctrl+alt+w",
            "ctrl+alt+1",
            "ctrl+alt+4",
            "ctrl+alt+v",
            "ctrl+alt+m",
            "ctrl+alt+.",
            "ctrl+alt+,",
        ] {
            assert!(parse_shortcut(combo).is_some(), "failed to parse {combo}");
        }
    }

    #[test]
    fn parsing_is_case_and_space_insensitive() {
        assert!(parse_shortcut("  Ctrl + Alt + Right ").is_some());
    }

    #[test]
    fn rejects_input_that_is_not_a_shortcut() {
        assert!(parse_shortcut("").is_none());
        assert!(parse_shortcut("ctrl+").is_none());
        assert!(parse_shortcut("ctrl+nonsense").is_none());
    }

    #[test]
    fn maps_function_keys() {
        assert!(parse_shortcut("ctrl+f5").is_some());
    }

    #[test]
    fn every_action_binding_resolves_to_a_method() {
        for binding in [
            "next_wallpaper",
            "prev_wallpaper",
            "stop_watch",
            "default_wallpaper",
            "toggle_transparency",
            "effect_normal",
            "effect_bw",
            "effect_vintage",
            "effect_hdr",
            "toggle_video",
            "toggle_video_sound",
            "next_video",
            "prev_video",
        ] {
            assert!(action_for(binding).is_some(), "no action for {binding}");
        }
    }

    #[test]
    fn window_toggle_and_scroll_modifier_are_not_engine_actions() {
        assert!(action_for("toggle_window").is_none());
        assert!(action_for("scroll_modifier").is_none());
    }
}
