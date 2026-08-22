//! The translation tables, served to the webview.
//!
//! `i18n.py` is 1,006 lines of which every one is data — three tables of 267 strings
//! and a `t()` helper the Python UI used. The Tauri UI does its own formatting, so
//! only the data crosses the wire, and it lives here as JSON generated from the
//! Python source and embedded at compile time.
//!
//! Regenerate with:
//!
//! ```text
//! uv run python -c "import json;from wallpaper_changer import i18n;\
//!   print(json.dumps({'supported': i18n.SUPPORTED_LANGUAGES,\
//!   'default': i18n.DEFAULT_LANGUAGE, 'translations': i18n.get_translations()},\
//!   ensure_ascii=False, indent=2))"
//! ```
//!
//! Once `i18n.py` is deleted the JSON becomes the source of truth and is edited
//! directly.

use serde_json::Value;

use crate::CoreError;

/// The tables, parsed once on first use.
fn tables() -> &'static Value {
    use std::sync::OnceLock;
    static TABLES: OnceLock<Value> = OnceLock::new();
    TABLES.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/translations.json"))
            .expect("translations.json is generated and must parse")
    })
}

/// The language code the engine reports as current.
///
/// `general.language` if it names a language we actually have, otherwise the
/// default — `i18n.set_language` ignores an unknown code the same way, so a hand-
/// edited config cannot leave the UI with no strings.
pub fn current_language(cfg: &Value) -> String {
    let requested = cfg
        .pointer("/general/language")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if tables()["supported"].get(requested).is_some() {
        requested.to_string()
    } else {
        default_language().to_string()
    }
}

pub fn default_language() -> &'static str {
    tables()["default"].as_str().unwrap_or("en")
}

/// The `get_translations` RPC result.
pub fn get_translations_result(cfg: &Value) -> Result<Value, CoreError> {
    let tables = tables();
    Ok(serde_json::json!({
        "translations": tables["translations"].clone(),
        "supported": tables["supported"].clone(),
        "default": tables["default"].clone(),
        "current": current_language(cfg),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_supported_language_has_a_table() {
        let tables = tables();
        let supported = tables["supported"].as_object().expect("supported map");
        assert_eq!(supported.len(), 3);
        for code in supported.keys() {
            assert!(
                tables["translations"].get(code).is_some(),
                "{code} is offered but has no strings"
            );
        }
    }

    /// A missing key in one language falls back to English at render time, but a
    /// wholesale drift means someone edited one table and forgot the others.
    #[test]
    fn the_tables_all_carry_the_same_keys() {
        let translations = tables()["translations"].as_object().unwrap();
        let english: std::collections::BTreeSet<&String> =
            translations["en"].as_object().unwrap().keys().collect();
        // A tripwire, not a fact about the UI: bump it deliberately when adding a
        // string, so a key that appears in one table by accident cannot pass.
        assert_eq!(english.len(), 268);
        for (code, table) in translations {
            let keys: std::collections::BTreeSet<&String> =
                table.as_object().unwrap().keys().collect();
            assert_eq!(keys, english, "{code} has drifted from en");
        }
    }

    #[test]
    fn the_supported_names_are_the_ones_the_ui_shows() {
        let supported = &tables()["supported"];
        assert_eq!(supported["en"], "English");
        assert_eq!(supported["pt_BR"], "Português (Brasil)");
        assert_eq!(supported["ja"], "日本語");
    }

    #[test]
    fn current_follows_the_configured_language() {
        assert_eq!(
            current_language(&json!({ "general": { "language": "ja" } })),
            "ja"
        );
        assert_eq!(
            current_language(&json!({ "general": { "language": "pt_BR" } })),
            "pt_BR"
        );
    }

    /// A hand-edited config naming a language we do not ship must not leave the UI
    /// with an empty table.
    #[test]
    fn an_unknown_language_falls_back_to_the_default() {
        assert_eq!(
            current_language(&json!({ "general": { "language": "xx" } })),
            "en"
        );
        assert_eq!(current_language(&json!({})), "en");
    }

    #[test]
    fn the_result_carries_the_four_documented_fields() {
        let result = get_translations_result(&json!({ "general": { "language": "ja" } })).unwrap();
        assert_eq!(result["default"], "en");
        assert_eq!(result["current"], "ja");
        assert!(result["supported"].is_object());
        assert!(result["translations"]["ja"].is_object());
    }
}
