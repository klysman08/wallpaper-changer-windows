//! Tests that touch the real desktop, and are therefore opt-in.
//!
//! Every test here is `#[ignore]`. They embed windows into the WORKERW layer and, in
//! one case, actually play video on it — visible on screen while they run. `cargo test`
//! must never do that to someone who only wanted to check their build, so they are run
//! deliberately:
//!
//! ```text
//! cargo test -p wallpaper-core --test desktop_layer -- --ignored --nocapture
//! ```
//!
//! The playback test needs a folder of videos:
//!
//! ```text
//! WALLPAPER_TEST_VIDEOS=C:/some/folder cargo test -p wallpaper-core \
//!   --test desktop_layer -- --ignored --nocapture
//! ```
//!
//! What they exist to catch is the thing no headless test can: `DestroyWindow` may only
//! be called by the thread that created the window, and when it is called from the
//! wrong one it **fails silently**. The Python implementation had exactly that bug, and
//! its only symptom was host windows left on the desktop layer until Explorer
//! restarted. "We asked the window to close" and "the window closed" are different
//! claims; these make the second one.

use std::sync::Arc;
use std::time::Duration;

use wallpaper_core::{monitor, video, workerw, NullSink};

/// How many start/stop cycles the window soak runs.
const CYCLES: usize = 200;

fn parent() -> isize {
    workerw::desktop_parent().expect("no desktop layer — is Explorer running?")
}

#[test]
#[ignore = "embeds windows in the real desktop layer"]
fn the_desktop_layer_is_discoverable_and_has_a_sane_origin() {
    let parent = parent();
    let origin = workerw::window_origin(parent);
    println!("desktop parent {parent:#x}, origin {origin:?}");

    let monitors = monitor::get_monitors().expect("monitors");
    assert!(!monitors.is_empty());
    for monitor in &monitors {
        // What `create_host_window` computes. A monitor above or left of the primary
        // gives the parent a negative origin, and dropping that offset is the classic
        // way to put every video off-screen on a multi-monitor desktop.
        println!(
            "  monitor {} at ({}, {}) {}x{} -> child ({}, {})",
            monitor.index,
            monitor.x,
            monitor.y,
            monitor.width,
            monitor.height,
            monitor.x - origin.0,
            monitor.y - origin.1,
        );
    }
}

/// Start and stop the player over and over, then count what is left behind.
///
/// One host window per screen is created and destroyed on every cycle, so this is
/// where a `DestroyWindow` that silently does nothing shows up: the count grows by the
/// number of screens each time round and the assertion at the end catches it. It also
/// puts several hundred mpv instances through creation and teardown, which is the
/// shape of load the `dxgi.dll` access violations appeared under.
///
/// `WALLPAPER_TEST_CYCLES` overrides the count for a quicker pass.
#[test]
#[ignore = "embeds windows in the real desktop layer"]
fn host_windows_never_outlive_their_player() {
    let Ok(folder) = std::env::var("WALLPAPER_TEST_VIDEOS") else {
        println!("set WALLPAPER_TEST_VIDEOS to a folder of videos to run this");
        return;
    };
    let videos = video::scan_video_folder(&folder);
    assert!(!videos.is_empty(), "no videos in {folder}");

    let cycles: usize = std::env::var("WALLPAPER_TEST_CYCLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(CYCLES);

    let parent = parent();
    let before = workerw::children(parent).len();
    let monitors = monitor::get_monitors().expect("monitors");
    println!(
        "desktop layer has {before} children before; {cycles} cycles over {} screen(s)",
        monitors.len()
    );

    let player = video::VideoPlayer::new(Arc::new(NullSink));
    for cycle in 0..cycles {
        player
            .start(videos.clone(), true, false, monitors.clone())
            .unwrap_or_else(|e| panic!("cycle {cycle}: {e}"));
        player.stop();
        if cycle % 25 == 24 {
            println!(
                "  {} cycles, {} children",
                cycle + 1,
                workerw::children(parent).len()
            );
        }
    }

    let after = workerw::children(parent).len();
    println!("desktop layer has {after} children after {cycles} cycles");
    assert_eq!(
        before,
        after,
        "{} host window(s) were stranded on the desktop layer",
        after.saturating_sub(before)
    );
}

/// An empty playlist is refused before anything is embedded in the desktop.
#[test]
#[ignore = "touches the real desktop layer"]
fn an_empty_playlist_never_reaches_the_desktop() {
    let parent = parent();
    let before = workerw::children(parent).len();
    let monitors = monitor::get_monitors().expect("monitors");
    let player = video::VideoPlayer::new(Arc::new(NullSink));

    assert!(player.start(Vec::new(), true, false, monitors).is_err());
    assert_eq!(
        workerw::children(parent).len(),
        before,
        "a refused start still put windows on the desktop layer"
    );
}

/// The whole path, for real: mpv into WORKERW, on every screen.
///
/// Skips rather than fails without a video folder, so the soak above can be run on a
/// machine with nothing to play.
#[test]
#[ignore = "plays video on the real desktop"]
fn video_really_plays_and_really_stops() {
    let Ok(folder) = std::env::var("WALLPAPER_TEST_VIDEOS") else {
        println!("set WALLPAPER_TEST_VIDEOS to a folder of videos to run this");
        return;
    };
    let videos = video::scan_video_folder(&folder);
    assert!(!videos.is_empty(), "no videos in {folder}");
    assert!(video::has_mpv(), "libmpv did not load");

    let parent = parent();
    let before = workerw::children(parent).len();
    let monitors = monitor::get_monitors().expect("monitors");
    let player = video::VideoPlayer::new(Arc::new(NullSink));

    player
        .start(videos.clone(), true, false, monitors)
        .expect("video did not start");

    // mpv loads asynchronously; asking immediately can beat the first file.
    std::thread::sleep(Duration::from_secs(3));
    let (running, current) = player.status();
    println!("playing: {running} — {current}");
    assert!(running, "the player reports nothing playing");
    assert!(!current.is_empty(), "no current file");

    if videos.len() > 1 {
        let next = player.step(1);
        println!("stepped to: {next}");
        assert_ne!(next, current, "stepping did not move the playlist");
        std::thread::sleep(Duration::from_secs(2));
        let back = player.step(-1);
        assert_eq!(back, current, "stepping back did not return");
    }

    player.set_sound(false);
    player.stop();

    let (running, _) = player.status();
    assert!(!running, "the player still reports playing after stop");
    let after = workerw::children(parent).len();
    assert_eq!(
        before,
        after,
        "{} host window(s) survived the stop",
        after.saturating_sub(before)
    );
}
