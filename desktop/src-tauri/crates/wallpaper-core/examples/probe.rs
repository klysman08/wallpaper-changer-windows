//! Exposes the composition stages individually, so they can be diffed against
//! Pillow one at a time.
//!
//! The whole point is to *not* compare the finished collage: a single number mixing
//! resampler noise with effect maths tells you nothing about which is wrong. Each
//! subcommand isolates one stage.
//!
//! ```text
//! cargo run -p wallpaper-core --example probe -- plan '<monitors json>' <count> <same>
//! cargo run -p wallpaper-core --example probe -- effect <in.png> <effect> <out.png>
//! cargo run -p wallpaper-core --example probe -- fit <in.png> <w> <h> <mode> <out.png>
//! ```
//!
//! Disposable: it exists to produce the golden images, and the goldens outlive it.

use wallpaper_core::{collage, effects, Monitor};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: probe <plan|effect|fit> ...");
        std::process::exit(2);
    }

    match args[1].as_str() {
        "plan" => {
            let monitors: Vec<Monitor> = serde_json::from_str::<serde_json::Value>(&args[2])
                .expect("monitors json")
                .as_array()
                .expect("an array")
                .iter()
                .enumerate()
                .map(|(i, m)| Monitor {
                    index: i,
                    x: m["x"].as_i64().unwrap() as i32,
                    y: m["y"].as_i64().unwrap() as i32,
                    width: m["width"].as_i64().unwrap() as i32,
                    height: m["height"].as_i64().unwrap() as i32,
                    name: format!("D{i}"),
                })
                .collect();
            let count: usize = args[3].parse().expect("count");
            let same: bool = args[4] == "true";
            let cells = collage::plan_collage(&monitors, count, same);
            println!("{}", serde_json::to_string(&cells).unwrap());
        }
        "effect" => {
            let source = image::open(&args[2]).expect("open source").to_rgb8();
            let out = effects::apply_effect(&source, &args[3]).expect("apply effect");
            out.save(&args[4]).expect("write output");
        }
        "fit" => {
            let source = image::open(&args[2]).expect("open source").to_rgb8();
            let w: i32 = args[3].parse().expect("width");
            let h: i32 = args[4].parse().expect("height");
            let out = collage::fit_image(&source, w, h, &args[5]);
            out.save(&args[6]).expect("write output");
        }
        // The wallpaper is always written as BMP, because that is what
        // SystemParametersInfoW reliably accepts. Pillow and the `image` crate write
        // their own headers, so "the pixels match" is not the same claim as "the file
        // matches" — this subcommand is what lets the two be compared as bytes.
        "bmp" => {
            let source = image::open(&args[2]).expect("open source").to_rgb8();
            source
                .save_with_format(&args[3], image::ImageFormat::Bmp)
                .expect("write bmp");
        }
        // The scroll hook's two pure functions over their whole input space, for
        // diffing against `scroll_transparency.py`. The unit tests pin the handful of
        // cases the Python suite pinned; this pins every one of them.
        "scroll" => {
            use wallpaper_core::scroll;
            let mut alphas = Vec::new();
            for current in 0..=255i64 {
                for notches in -10..=10i64 {
                    alphas.push(scroll::next_alpha(current, notches));
                }
            }
            let names: Vec<&str> = [
                "alt", "ALT", "  Ctrl  ", "control", "shift", "win", "windows", "super",
                "meta", "", "nonsense", "ctrl+alt", "CONTROL", "Win", "SHIFT", "sHiFt",
                "  ", "altt", "ctrl ", "meta ",
            ]
            .iter()
            .map(|raw| scroll::normalize_modifier(Some(raw)))
            .collect();
            println!(
                "{}",
                serde_json::json!({
                    "next_alpha": alphas,
                    "normalize_modifier": names,
                    "step": scroll::STEP,
                    "min_alpha": scroll::MIN_ALPHA,
                    "max_alpha": scroll::MAX_ALPHA,
                    "modifiers": scroll::SUPPORTED_MODIFIERS,
                })
            );
        }
        other => {
            eprintln!("unknown subcommand: {other}");
            std::process::exit(2);
        }
    }
}
