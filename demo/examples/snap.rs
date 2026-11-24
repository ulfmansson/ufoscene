//! Render a frame from every part of one demo to `snapshots/<demo>-<part>-<name>.png`.
//!
//!     cargo run -p ufo-demo --example snap --release -- [DEMO] [SECONDS]

use std::fs;

use ufo_demo::Demo;
use ufo_demo::gfx::{H, W};
use ufo_demo::png::png;

fn main() {
    let mut args = std::env::args().skip(1);
    let demo: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let secs: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(6.0);
    fs::create_dir_all("snapshots").expect("create snapshots/");
    let mut d = Demo::new();
    let mut t = 10.0f32;
    d.frame(t, false);
    d.set_demo(demo);
    let parts = d.recipes[d.current()].parts.len();
    for p in 0..parts {
        for _ in 0..(secs * 60.0) as usize {
            t += 1.0 / 60.0;
            d.frame(t, false);
        }
        let (di, pi, name, _) = d.state();
        let file = format!("snapshots/{:03}-{}-{}.png", di + 1, pi + 1, name.to_lowercase().replace(' ', "-"));
        fs::write(&file, png(&d.fb.px, W, H, 2)).expect("write png");
        println!("wrote {file}");
        if p + 1 < parts {
            d.next_part();
        }
    }
}
