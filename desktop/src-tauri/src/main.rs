// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
//
// It also means a release build has no stdout of its own, which is why the CLI mode
// borrows the parent's console before it prints anything. See `cli::run`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `apply`, `watch` and `video` run headless and exit; everything else — no
    // arguments, or the `--minimized` the autostart entry passes — starts the app.
    if tauri_native_lib::cli::wants_cli() {
        std::process::exit(tauri_native_lib::cli::run());
    }
    tauri_native_lib::run()
}
