//! The error vocabulary the engine speaks on the wire.
//!
//! These `kind` strings are not decoration: `desktop/src/lib/engine.ts` reads the
//! `"{kind}: {message}"` prefix, and `rpc.py` has emitted exactly this set since the
//! protocol was written. A new variant here is a protocol change.

use std::fmt;

/// The `error.type` values `rpc.py` puts on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Generic failure — `rpc.py`'s default when nothing more specific fits.
    Error,
    /// Another apply is already in flight. Never queued; see `Core`'s try-lock rule.
    Busy,
    /// A parameter was outside the accepted set (an unknown effect, say).
    Invalid,
    /// `apply_previous_wallpaper` with nothing behind the cursor.
    NoHistory,
    /// A required setting is empty, e.g. no default wallpaper is configured.
    NotConfigured,
    /// A path that should exist does not.
    NotFound,
    /// No displays were enumerated.
    NoMonitors,
    /// libmpv is unavailable, so video cannot start.
    NoMpv,
    /// A filesystem operation failed.
    Io,
    /// The method name is not in the allowlist.
    UnknownMethod,
    /// The parameters did not fit the method's signature.
    BadParams,
    /// The request line was not valid JSON.
    Parse,
    /// An unexpected panic or bug. See [`crate::guard`].
    Internal,
}

impl ErrorKind {
    /// The exact string that goes into `error.type` on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::Error => "error",
            ErrorKind::Busy => "busy",
            ErrorKind::Invalid => "invalid",
            ErrorKind::NoHistory => "no_history",
            ErrorKind::NotConfigured => "not_configured",
            ErrorKind::NotFound => "not_found",
            ErrorKind::NoMonitors => "no_monitors",
            ErrorKind::NoMpv => "no_mpv",
            ErrorKind::Io => "io",
            ErrorKind::UnknownMethod => "unknown_method",
            ErrorKind::BadParams => "bad_params",
            ErrorKind::Parse => "parse",
            ErrorKind::Internal => "internal",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An engine failure, carrying the wire `type` and the human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError {
    kind: ErrorKind,
    message: String,
}

impl CoreError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// The `{"type": ..., "message": ...}` half of a failure envelope.
    pub fn to_payload(&self) -> serde_json::Value {
        serde_json::json!({ "type": self.kind.as_str(), "message": self.message })
    }
}

/// Only the message. The seam prepends the kind itself, matching `dispatch_line`.
impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CoreError {}

macro_rules! ctor {
    ($name:ident, $kind:expr, $doc:literal) => {
        impl CoreError {
            #[doc = $doc]
            pub fn $name(message: impl Into<String>) -> Self {
                Self::new($kind, message)
            }
        }
    };
}

ctor!(error, ErrorKind::Error, "A generic failure.");
ctor!(busy, ErrorKind::Busy, "Another apply is in flight.");
ctor!(invalid, ErrorKind::Invalid, "A parameter was out of range.");
ctor!(
    no_history,
    ErrorKind::NoHistory,
    "Nothing behind the history cursor."
);
ctor!(
    not_configured,
    ErrorKind::NotConfigured,
    "A required setting is empty."
);
ctor!(not_found, ErrorKind::NotFound, "A path does not exist.");
ctor!(
    no_monitors,
    ErrorKind::NoMonitors,
    "No displays were enumerated."
);
ctor!(no_mpv, ErrorKind::NoMpv, "libmpv is unavailable.");
ctor!(io, ErrorKind::Io, "A filesystem operation failed.");
ctor!(
    bad_params,
    ErrorKind::BadParams,
    "Parameters did not fit the signature."
);
ctor!(internal, ErrorKind::Internal, "An unexpected panic or bug.");

impl CoreError {
    /// The method name is not in the allowlist. Worded exactly as `rpc.py` words it,
    /// because the string reaches the webview.
    pub fn unknown_method(method: &str) -> Self {
        Self::new(
            ErrorKind::UnknownMethod,
            format!("Unknown method: {method}"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire vocabulary is a contract with `rpc.py` and with `engine.ts`.
    #[test]
    fn kind_strings_match_the_python_vocabulary() {
        let expected = [
            "error",
            "busy",
            "invalid",
            "no_history",
            "not_configured",
            "not_found",
            "no_monitors",
            "no_mpv",
            "io",
            "unknown_method",
            "bad_params",
            "parse",
            "internal",
        ];
        let actual = [
            ErrorKind::Error,
            ErrorKind::Busy,
            ErrorKind::Invalid,
            ErrorKind::NoHistory,
            ErrorKind::NotConfigured,
            ErrorKind::NotFound,
            ErrorKind::NoMonitors,
            ErrorKind::NoMpv,
            ErrorKind::Io,
            ErrorKind::UnknownMethod,
            ErrorKind::BadParams,
            ErrorKind::Parse,
            ErrorKind::Internal,
        ]
        .map(ErrorKind::as_str);
        assert_eq!(actual, expected);
    }

    /// `Display` must yield the bare message: the seam builds "{kind}: {message}"
    /// itself, and a Display that repeated the kind would double it.
    #[test]
    fn display_is_the_message_alone() {
        let err = CoreError::busy("An apply is already running.");
        assert_eq!(err.to_string(), "An apply is already running.");
        assert_eq!(
            format!("{}: {}", err.kind(), err),
            "busy: An apply is already running."
        );
    }

    #[test]
    fn unknown_method_is_worded_like_the_python() {
        let err = CoreError::unknown_method("nope");
        assert_eq!(err.kind(), ErrorKind::UnknownMethod);
        assert_eq!(err.to_string(), "Unknown method: nope");
    }

    #[test]
    fn payload_carries_type_and_message() {
        let payload = CoreError::not_found("gone.png").to_payload();
        assert_eq!(payload["type"], "not_found");
        assert_eq!(payload["message"], "gone.png");
    }
}
