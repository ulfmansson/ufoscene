//! Render effects to PNGs without a browser, plus one contact sheet of all of them.
//!
//!     cargo run -p ufo-demo --example fxsnap --release -- [OUT_DIR] [NAME_FILTER] [SECONDS] [SEED]
//!
//! Writes `OUT_DIR/<n>-<name>.png` (2x size) for every effect whose name contains
//! NAME_FILTER (case-insensitive, empty = all), and `OUT_DIR/sheet.png` with all of
//! them as half-size thumbnails, 4 per row.

use std::fs;

use ufo_demo::audio::Audio;
use ufo_demo::fx::{self, Ctx};
use ufo_demo::gfx::{Fb, H, W};
use ufo_demo::png::png;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "snapshots/fx".into());
    let filter = args.next().unwrap_or_default().to_uppercase();
    let secs: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let seed: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    fs::create_dir_all(&out).expect("create output dir");
    let defs: Vec<_> = fx::all().into_iter().chain(fx::seasonal()).filter(|d| d.name.contains(&filter)).collect();
    let cols = 4;
    let (tw, th) = (W / 2, H / 2);
    let rows = defs.len().div_ceil(cols).max(1);
    let mut sheet = vec![0xFF00_0000u32; cols * tw * rows * th];
    for (k, d) in defs.iter().enumerate() {
        let pal = fx::palette(k + seed as usize);
        let mut e = (d.make)(seed);
        let mut fb = Fb::new();
        let mut a = Audio::new();
        let frames = (secs * 60.0) as usize;
        let start = std::time::Instant::now();
        for f in 0..frames {
            let time = 20.0 + f as f32 / 60.0;
            a.update(time, 1.0 / 60.0, false);
            let c = Ctx { t: f as f32 / 60.0, time, dt: 1.0 / 60.0, a: &a, pal: &pal, beats: f as u32 / 29, phase: 0.0, name: "UFO", text: "HEJ HEJ   ", image: None, song: 0 };
            e.draw(&mut fb, &c);
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / frames.max(1) as f64;
        fb.quantize_ocs();
        let name = format!("{out}/{:02}-{}.png", k, d.name.to_lowercase().replace(' ', "-"));
        fs::write(&name, png(&fb.px, W, H, 2)).expect("write png");
        println!("{name}  {ms:.2} ms/frame");
        let (ox, oy) = ((k % cols) * tw, (k / cols) * th);
        for y in 0..th {
            for x in 0..tw {
                sheet[(oy + y) * cols * tw + ox + x] = fb.px[y * 2 * W + x * 2];
            }
        }
    }
    fs::write(format!("{out}/sheet.png"), png(&sheet, cols * tw, rows * th, 1)).expect("write sheet");
    println!("{out}/sheet.png");
}
