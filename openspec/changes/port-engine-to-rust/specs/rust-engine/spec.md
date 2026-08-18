## Purpose

Provides a native Rust core that owns configuration, image composition, the Windows API surface, and the video wallpaper layer, running in-process inside the Tauri application and replacing the Python sidecar.

## ADDED Requirements

### Requirement: In-process engine with no sidecar

The application SHALL run as a single process. The Rust core SHALL be linked into the Tauri binary, and no Python interpreter, PyInstaller bundle, or child engine process SHALL be shipped.

#### Scenario: Installer size

- **WHEN** the release installer is built via `scripts/build_app.ps1`
- **THEN** it contains no `engine/` resource directory and is approximately 15 MB rather than 145 MB

#### Scenario: Engine teardown on exit

- **WHEN** the application exits, whether via the tray Quit item or a window close
- **THEN** video host windows parented to WORKERW are destroyed by the thread that created them, with no pipe-close handshake and no grace-period poll

#### Scenario: A panic in the core does not kill the app

- **WHEN** a corrupt or unreadable image causes a panic inside `Core::dispatch`
- **THEN** the call returns an error envelope of type `internal` and the application continues running

### Requirement: Preserved RPC contract during and after migration

The Rust core SHALL preserve the existing method names, parameter shapes, result shapes, error `type` vocabulary, and event envelopes. The webview SHALL require no changes.

#### Scenario: Error string format

- **WHEN** any engine call fails, from either the Rust core or the Python sidecar during migration
- **THEN** the error reaching the webview is formatted `"{kind}: {message}"`, identical to what `dispatch_line` produces today

#### Scenario: Event envelope

- **WHEN** the core emits `wallpaper_applied`, `video_status`, `transparency_changed`, `session_restored`, or `error`
- **THEN** it arrives at the webview as the `engine-event` payload `{"event": name, "data": data}`, indistinguishable from a sidecar-produced event

#### Scenario: Method gating

- **WHEN** the webview calls `engine_call` with an unrecognised method name
- **THEN** the call is rejected with `unknown_method`, gated by a single exhaustive match in `Core::dispatch`

### Requirement: Incremental migration with a working application at every commit

The migration SHALL route ported methods to the Rust core and fall through to the Python sidecar for the rest, via a `Dispatch::NotPorted` variant distinct from any error value.

#### Scenario: A partially migrated build

- **WHEN** some methods are ported and others are not
- **THEN** the application launches, applies wallpapers, and responds to hotkeys and the tray exactly as before

#### Scenario: A Rust-side failure does not re-execute in Python

- **WHEN** a ported method fails inside the Rust core
- **THEN** the error is returned to the caller and the sidecar is NOT invoked for that same request

### Requirement: Byte-identical collage geometry

The Rust core SHALL reproduce the collage grid arithmetic exactly, including the fixed column table, the `ceil(sqrt(n))` fallback, integer-truncating cell dimensions, and the centred short last row.

#### Scenario: Grid layout parity

- **WHEN** `plan_collage` is computed for any monitor arrangement and any collage count from 1 to 9
- **THEN** the resulting cell rectangles are byte-identical to the Python implementation's output

#### Scenario: Effect parity

- **WHEN** the `bw`, `vintage`, or `hdr` effect is applied to a fixed input image
- **THEN** every channel value differs from the Pillow output by at most 1, with the 1-pixel border left unprocessed by kernel filters

### Requirement: Existing user data continues to work unmodified

The Rust core SHALL read and write the existing `settings.toml`, `state.json`, `gallery.json`, and `transparency.json` formats in their existing locations, without migration or reset.

#### Scenario: Rotation history survives the upgrade

- **WHEN** a user with an existing `state.json` upgrades to the Rust build
- **THEN** the no-repeat selection cycle continues from where it left off, because the history key is `C:\path\to\folder:random_history` and not the `\\?\`-prefixed form

#### Scenario: Config comments survive a save

- **WHEN** settings are changed and saved from the Settings screen
- **THEN** the explanatory comments and any unrecognised keys in `settings.toml` are preserved rather than destroyed

#### Scenario: Gallery entries resolve across folder moves

- **WHEN** the saved-collage folder has been changed after collages were exported
- **THEN** the gallery still lists previously saved images at their original absolute paths, and removing an entry leaves the image file on disk

### Requirement: Preserved concurrency and rotation semantics

The Rust core SHALL use non-blocking lock acquisition for wallpaper application and SHALL re-arm the rotation timer only after each tick completes.

#### Scenario: Concurrent apply requests

- **WHEN** a second `apply_wallpaper` arrives while one is still compositing
- **THEN** it is rejected immediately with error type `busy` rather than queued behind the first

#### Scenario: Rotation interval

- **WHEN** rotation is active with an interval of N seconds and a tick takes T seconds to composite
- **THEN** the next tick is scheduled N seconds after the previous one finished, not N seconds after it started

### Requirement: Test artifacts that outlive the Python implementation

The change SHALL produce a language-neutral protocol conformance corpus and a set of frozen golden images that remain valid regression tests after the Python is deleted.

#### Scenario: Conformance corpus runs against either implementation

- **WHEN** the corpus runner is pointed at the Python sidecar or at `wallpaper-core-cli`
- **THEN** both pass the same assertions on method existence, error types, event emission, and result field names

#### Scenario: Golden images guard composition

- **WHEN** the composition code is changed after the Python is deleted
- **THEN** the committed golden PNGs detect geometry regressions at zero tolerance and effect regressions at a tolerance of 1

## REMOVED Requirements

### Requirement: Python engine sidecar

**Reason:** Replaced by the in-process Rust core. The stdio JSON-RPC transport, the PyInstaller build pipeline (`scripts/build_engine.ps1`, `wallpaper_changer_rpc.spec`), and the `resources` bundle entry are removed with it.

**Migration:** All 45 RPC methods are reimplemented in `wallpaper-core` with identical contracts. No user action is required; user data files are read in place.

### Requirement: Legacy ttkbootstrap GUI

**Reason:** Superseded by the Tauri + React desktop application. `gui.py`, `transparency_gui.py`, and `hotkeys.py` were already unreachable from the shipping application.

**Migration:** None. The `wallpaper-changer-gui` entry point is removed; the Tauri window is the only GUI.

### Requirement: Python CLI

**Reason:** Removing Python entirely removes the `click`-based CLI.

**Migration:** Reimplemented as a `clap`-based mode of the Tauri binary, preserving the `apply`, `watch`, and `video` commands along with their collage-count range and effect-choice validation.
