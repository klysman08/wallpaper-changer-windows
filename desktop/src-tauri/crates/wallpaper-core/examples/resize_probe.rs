//! Resizes a PNG with the same filter the engine uses, for comparison against Pillow.
//!
//! Phase 4 pins composition against golden images, and the tolerance it can hold to
//! depends on how far `image`'s Lanczos3 sits from Pillow's LANCZOS. This exists to
//! measure that with no JPEG encoding in the way — `get_image_preview` would add its
//! own loss and blur the answer.
//!
//! ```text
//! cargo run -p wallpaper-core --example resize_probe -- in.png 640 480 out.png
//! ```
//!
//! Disposable: delete it once the phase 4 harness supersedes it.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        eprintln!("usage: resize_probe <in.png> <width> <height> <out.png>");
        std::process::exit(2);
    }
    let width: u32 = args[2].parse().expect("width");
    let height: u32 = args[3].parse().expect("height");

    let source = image::open(&args[1]).expect("open source");
    let resized = source.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    resized.save(&args[4]).expect("write output");
}
