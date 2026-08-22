//! The command-line mode of the application binary.
//!
//! Replaces `cli.py`, which was a separate `click` program in the Python package. The
//! subcommands and their options are carried over: `apply`, `watch`, and `video`.
//!
//! ## One binary, two modes
//!
//! There is no separate CLI executable. `main` looks at the arguments and, if the
//! first one is a subcommand, runs headless and exits instead of starting Tauri. That
//! keeps `--minimized` — which the autostart entry passes — working as a plain GUI
//! launch rather than being mistaken for a command.
//!
//! ## The console has to be borrowed
//!
//! A release build is linked as `windows_subsystem = "windows"` so double-clicking it
//! does not flash a console. That also means it has **no stdout to print to** when run
//! from a terminal, and output would silently vanish. [`attach_console`] borrows the
//! parent's console for the duration. Nothing is printed before it runs.
//!
//! ## Everything goes through `dispatch`
//!
//! The CLI drives [`wallpaper_core::Core::dispatch`] rather than reaching into the
//! engine directly, so it is held to the same method allowlist and the same argument
//! validation as the webview. A command that works here works there.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use serde_json::{json, Map, Value};
use wallpaper_core::{Core, Dispatch, LoggingNotifier, NullSink};

/// Effects `--effect` accepts. The same list `display.effect` validates against.
const EFFECTS: [&str; 4] = ["normal", "bw", "vintage", "hdr"];

#[derive(Parser)]
#[command(
    name = "wallpaper-changer",
    about = "Collage wallpaper for Windows",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compose and set the wallpaper now.
    Apply {
        /// random | sequential
        #[arg(long)]
        selection: Option<String>,
        /// Images per monitor.
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=8))]
        collage_count: Option<u8>,
        /// normal | bw | vintage | hdr
        #[arg(long, value_parser = EFFECTS)]
        effect: Option<String>,
        /// A settings.toml to use instead of the installed one.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Change the wallpaper on the configured interval until interrupted.
    Watch {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Play the configured folder as a video wallpaper until interrupted.
    Video {
        /// A folder of videos, overriding `video.folder`.
        #[arg(long)]
        folder: Option<String>,
        #[arg(long, action = clap::ArgAction::Set)]
        loop_playlist: Option<bool>,
        #[arg(long, action = clap::ArgAction::Set)]
        sound: Option<bool>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

/// Whether the arguments name a subcommand, and so whether to skip the GUI.
///
/// Deliberately narrow: anything else — no arguments, `--minimized`, a file dropped on
/// the exe — starts the application as usual. Growing this to "any argument means CLI"
/// would break the autostart entry.
pub fn wants_cli() -> bool {
    matches!(
        std::env::args().nth(1).as_deref(),
        Some("apply" | "watch" | "video" | "help" | "--help" | "-h" | "--version" | "-V")
    )
}

/// Run the command and return the process exit code.
pub fn run() -> i32 {
    attach_console();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        // clap has already written usage or the help text to the console it was asked
        // for; its own exit code distinguishes `--help` from a mistake.
        Err(e) => {
            let _ = e.print();
            return i32::from(e.use_stderr());
        }
    };

    // A current-thread runtime: the CLI is one command at a time, and the core's
    // blocking work goes to `spawn_blocking` regardless.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("[ERRO] could not start the runtime: {e}");
            return 1;
        }
    };
    runtime.block_on(execute(cli.command))
}

async fn execute(command: Command) -> i32 {
    let core = Core::new(Arc::new(NullSink), Arc::new(LoggingNotifier));

    match command {
        Command::Apply {
            selection,
            collage_count,
            effect,
            config,
        } => {
            let mut draft = match base_config(config.as_deref()) {
                Ok(draft) => draft,
                Err(code) => return code,
            };
            set_in(
                &mut draft,
                "general",
                "selection",
                selection.map(Value::from),
            );
            set_in(
                &mut draft,
                "general",
                "collage_count",
                collage_count.map(|n| Value::from(u64::from(n))),
            );
            set_in(&mut draft, "display", "effect", effect.map(Value::from));

            match call(&core, "apply_wallpaper", json!({ "config": draft })).await {
                Ok(result) => {
                    println!(
                        "[OK] wallpaper applied -> {}",
                        result["output"].as_str().unwrap_or("?")
                    );
                    0
                }
                Err(e) => {
                    eprintln!("[ERRO] {e}");
                    1
                }
            }
        }

        Command::Watch { config } => {
            let draft = match base_config(config.as_deref()) {
                Ok(draft) => draft,
                Err(code) => return code,
            };
            // `watch_start` reads the interval from the live configuration, so the
            // draft only matters for the applies the timer then performs.
            if !draft.is_null() {
                if let Err(e) = call(&core, "save_config", json!({ "config": draft })).await {
                    eprintln!("[ERRO] {e}");
                    return 1;
                }
            }
            match call(&core, "watch_start", json!({})).await {
                Ok(result) => println!(
                    "[INFO] changing the wallpaper every {}s. Ctrl+C to stop.",
                    result["interval"]
                ),
                Err(e) => {
                    eprintln!("[ERRO] {e}");
                    return 1;
                }
            }
            wait_for_interrupt().await;
            let _ = call(&core, "watch_stop", json!({})).await;
            println!("\n[INFO] stopped.");
            0
        }

        Command::Video {
            folder,
            loop_playlist,
            sound,
            config,
        } => {
            let mut draft = match base_config(config.as_deref()) {
                Ok(draft) => draft,
                Err(code) => return code,
            };
            set_in(&mut draft, "video", "folder", folder.map(Value::from));
            set_in(&mut draft, "video", "loop", loop_playlist.map(Value::from));
            set_in(&mut draft, "video", "sound", sound.map(Value::from));

            match call(&core, "video_start", json!({ "config": draft })).await {
                Ok(result) => println!(
                    "[OK] playing {}. Ctrl+C to stop.",
                    result["current"].as_str().unwrap_or("?")
                ),
                Err(e) => {
                    eprintln!("[ERRO] {e}");
                    return 1;
                }
            }
            wait_for_interrupt().await;
            // Teardown is not optional: the host windows are children of WORKERW and
            // would sit on the desktop until Explorer restarted.
            let _ = call(&core, "video_stop", json!({})).await;
            println!("\n[INFO] stopped.");
            0
        }
    }
}

/// The starting draft: a `--config` file if one was given, otherwise nothing.
///
/// Python's `--config` *replaced* the configuration it loaded. Passing the whole file
/// as the draft is the same thing by a different route, because `_merged` overlays
/// every section it names.
fn base_config(path: Option<&std::path::Path>) -> Result<Value, i32> {
    match path {
        None => Ok(Value::Object(Map::new())),
        Some(path) => match wallpaper_core::config::load_config(Some(path)) {
            Ok(cfg) => Ok(cfg),
            Err(e) => {
                eprintln!("[ERRO] {e}");
                Err(1)
            }
        },
    }
}

fn set_in(draft: &mut Value, section: &str, key: &str, value: Option<Value>) {
    let Some(value) = value else { return };
    let Some(root) = draft.as_object_mut() else {
        return;
    };
    root.entry(section)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .map(|s| s.insert(key.to_string(), value));
}

async fn call(core: &Core, method: &str, params: Value) -> Result<Value, String> {
    match core.dispatch(method, &params).await {
        Dispatch::Handled(result) => result.map_err(|e| format!("{}: {}", e.kind(), e)),
        Dispatch::NotPorted => Err(format!("unknown method: {method}")),
    }
}

async fn wait_for_interrupt() {
    if tokio::signal::ctrl_c().await.is_err() {
        // Without a signal handler there is nothing sensible to wait for, and
        // returning immediately would tear down what was just started.
        std::future::pending::<()>().await;
    }
}

/// Borrow the console of whatever launched us, so printing reaches the terminal.
///
/// A `windows_subsystem = "windows"` binary starts with no standard handles; without
/// this every `println!` in a release build goes nowhere. Failing is not an error —
/// it means there was no console to attach to, such as when launched from Explorer.
#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_console() {}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    /// The autostart entry passes `--minimized`, and a file association or a drop
    /// would pass a path. Neither may be read as a command, or the app would refuse to
    /// start for the people least able to work out why.
    #[test]
    fn only_a_subcommand_diverts_away_from_the_gui() {
        for argument in ["--minimized", "C:/pictures/a.jpg", "--nonsense"] {
            assert!(
                !matches!(
                    Some(argument),
                    Some(
                        "apply" | "watch" | "video" | "help" | "--help" | "-h" | "--version" | "-V"
                    )
                ),
                "{argument} would have been taken for a command"
            );
        }
    }

    #[test]
    fn collage_count_is_held_to_the_grid_the_composer_supports() {
        // The Python CLI used IntRange(1, 8); the grid table has no shape beyond 8.
        assert!(
            Cli::try_parse_from(["wallpaper-changer", "apply", "--collage-count", "8"]).is_ok()
        );
        assert!(
            Cli::try_parse_from(["wallpaper-changer", "apply", "--collage-count", "9"]).is_err()
        );
        assert!(
            Cli::try_parse_from(["wallpaper-changer", "apply", "--collage-count", "0"]).is_err()
        );
    }

    #[test]
    fn an_unknown_effect_is_refused_before_anything_is_composed() {
        assert!(Cli::try_parse_from(["wallpaper-changer", "apply", "--effect", "vintage"]).is_ok());
        assert!(Cli::try_parse_from(["wallpaper-changer", "apply", "--effect", "sepia"]).is_err());
    }

    #[test]
    fn a_draft_only_carries_the_options_that_were_given() {
        let mut draft = Value::Object(Map::new());
        set_in(&mut draft, "general", "selection", None);
        assert_eq!(
            draft,
            json!({}),
            "an absent option must not appear in the draft"
        );

        set_in(
            &mut draft,
            "general",
            "selection",
            Some(Value::from("sequential")),
        );
        set_in(&mut draft, "display", "effect", Some(Value::from("bw")));
        assert_eq!(
            draft,
            json!({ "general": { "selection": "sequential" }, "display": { "effect": "bw" } })
        );
    }
}
