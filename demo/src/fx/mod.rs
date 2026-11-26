//! The effect library. Every part of every demo is an [`Effect`]: something that
//! paints the whole framebuffer once per frame, driven by time, the music and a
//! palette chosen by the demo.
//!
//! Effects are listed in [`all`]. A demo (see `crate::demos`) is a sequence of
//! effects plus a palette, a logo, a scroll text and a transition style.

pub mod abcgames;
pub mod christmas;
pub mod classic;
pub mod concert;
pub mod deform;
pub mod image;
pub mod oldskool;
pub mod pixel;
pub mod vector;
pub mod zxgames;

use std::f32::consts::TAU;

use crate::audio::Audio;
use crate::gfx::*;

/// Everything an effect may read while drawing one frame.
pub struct Ctx<'a> {
    /// Seconds since this part started (starts at 0 for every part).
    pub t: f32,
    /// Seconds since the demo started; keeps running across parts.
    pub time: f32,
    /// Seconds since the previous frame (0..0.1).
    pub dt: f32,
    /// Music: `bass`, `mid`, `treble` in 0..1, `beat` (true on a beat frame),
    /// `pulse` (1 on a beat, decays), `period` (seconds per beat), `bins` (512 FFT bins).
    pub a: &'a Audio,
    /// The demo's colours for this part.
    pub pal: &'a Palette,
    /// Beats since this part started.
    pub beats: u32,
    /// 0..1 position inside the current beat (0 right on the beat).
    pub phase: f32,
    /// The demo's title (font-safe upper case) and its scroll text.
    pub name: &'a str,
    pub text: &'a str,
    /// The current artist picture from Spotify, if any.
    pub image: Option<&'a Image>,
    /// Goes up by one every time the page says a new song started (and on N during the
    /// concert); effects that follow the songs watch it change.
    pub song: u32,
}

/// A 256×256 picture handed over by the page (the artist on Spotify right now).
pub struct Image {
    pub px: Vec<u32>,
    /// Changes every time a new picture arrives.
    pub id: u32,
    /// Who is in the picture, font-safe upper case.
    pub caption: String,
    /// Where the picture comes from and its licence (photographer, CC licence, source),
    /// font-safe upper case. Shown small in a corner; may be empty.
    pub credit: String,
}

/// A visual part. `draw` must paint every pixel of `fb` each frame: the
/// framebuffer holds last frame's output *plus* overlays (logo, scroller, HUD),
/// so an effect that wants trails or feedback keeps its own buffer.
pub trait Effect {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx);
}

/// The effect already draws its own text, so demos put no logo or scroller over it.
pub const OWN_TEXT: u32 = 1;
/// Leaves the top ~40 rows calm enough for a logo.
pub const LOGO_TOP: u32 = 2;
/// Leaves the bottom ~30 rows calm enough for a scroller.
pub const SCROLL_BOTTOM: u32 = 4;

/// Themes demos are built around.
pub const PIXEL: u32 = 1 << 8; // per-pixel 2D fields: plasma, fire, fractals
pub const DEFORM: u32 = 1 << 9; // tunnels, planes, texture mapping
pub const VECTOR: u32 = 1 << 10; // 3D objects, dots, particles
pub const RASTER: u32 = 1 << 11; // copper/raster bars, bobs, text, analysers
pub const RETRO8: u32 = 1 << 12; // 8-bit computer style scenes
pub const IMAGE: u32 = 1 << 13; // shows the artist picture; only used when there is one

pub struct Def {
    /// Upper case, letters/digits/spaces only, at most 14 characters (shown in the HUD).
    pub name: &'static str,
    /// One theme constant (`PIXEL`, `DEFORM`, ...) or'ed with layout flags.
    pub flags: u32,
    /// Build a fresh instance. `seed` differs per demo: use it to pick a variant
    /// (shape, formula, speed, layout) so the effect looks different in different demos.
    pub make: fn(seed: u32) -> Box<dyn Effect>,
}

/// Every effect, in a fixed order (demo recipes depend on it).
pub fn all() -> Vec<Def> {
    let mut v = Vec::new();
    classic::register(&mut v);
    pixel::register(&mut v);
    deform::register(&mut v);
    vector::register(&mut v);
    oldskool::register(&mut v);
    zxgames::register(&mut v);
    abcgames::register(&mut v);
    image::register(&mut v);
    v
}

/// Scenes for special days and the concert, kept out of `all()` so the 100 demos never
/// change. The demo player adds them after the recipes are built.
pub fn seasonal() -> Vec<Def> {
    vec![christmas::def(), concert::def()]
}

// ---------------------------------------------------------------- palettes

/// A demo's colours, as two 256-entry lookup tables.
pub struct Palette {
    /// Cyclic colours: `at(x)` wraps around every 1.0. Good for plasma-like cycling.
    cyc: [u32; 256],
    /// Black → palette colours → near white. Good for intensity: fire, glow, fog, depth.
    ramp: [u32; 256],
    /// A dark background colour that goes with the palette.
    pub bg: u32,
    /// A bright accent colour.
    pub hi: u32,
}

impl Palette {
    /// Cosine palette `a + b * cos(2π(c·x + d))` per channel (Inigo Quilez's form).
    pub fn cosine(a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) -> Self {
        let f = |x: f32| {
            let ch = |i: usize| a[i] + b[i] * (TAU * (c[i] * x + d[i])).cos();
            rgbf(ch(0), ch(1), ch(2))
        };
        let mut cyc = [0; 256];
        let mut ramp = [0; 256];
        for i in 0..256 {
            let x = i as f32 / 256.0;
            cyc[i] = f(x);
            // ramp: rise out of black through the brighter half of the palette, end near white
            let base = f(0.15 + x * 0.6);
            let k = smoothstep(x * 1.6);
            let c = scale(base, k * 1.15);
            ramp[i] = if x > 0.8 { mix(c, rgb(255, 255, 240), (x - 0.8) / 0.2 * 0.8) } else { c };
        }
        let bg = scale(f(0.2), 0.12);
        let hi = mix(f(0.55), rgb(255, 255, 255), 0.35);
        Palette { cyc, ramp, bg, hi }
    }

    #[inline]
    pub fn at(&self, x: f32) -> u32 {
        self.cyc[((x - x.floor()) * 256.0) as usize & 255]
    }
    #[inline]
    pub fn at8(&self, i: u8) -> u32 {
        self.cyc[i as usize]
    }
    #[inline]
    pub fn ramp(&self, x: f32) -> u32 {
        self.ramp[(x.clamp(0.0, 0.999) * 256.0) as usize]
    }
    #[inline]
    pub fn ramp8(&self, i: u8) -> u32 {
        self.ramp[i as usize]
    }
}

pub const PALETTE_NAMES: [&str; 16] = [
    "RAINBOW", "FIRE", "ICE", "TOXIC", "SUNSET", "WORKBENCH", "PURPLE RAIN", "GOLD", "NEON", "OCEAN",
    "CANDY", "FOREST", "BLOOD", "STEEL", "AURORA", "RETRO",
];

/// The 16 palettes demos choose from.
pub fn palette(i: usize) -> Palette {
    let p = Palette::cosine;
    match i % 16 {
        0 => p([0.5; 3], [0.5; 3], [1.0; 3], [0.0, 0.33, 0.67]),
        1 => p([0.5, 0.2, 0.05], [0.5, 0.4, 0.1], [0.5, 0.6, 0.7], [0.0, 0.15, 0.3]),
        2 => p([0.4, 0.6, 0.8], [0.35, 0.35, 0.25], [1.0, 1.0, 1.0], [0.6, 0.62, 0.65]),
        3 => p([0.35, 0.6, 0.2], [0.35, 0.4, 0.25], [1.0, 1.0, 0.5], [0.3, 0.2, 0.25]),
        4 => p([0.6, 0.35, 0.4], [0.4, 0.3, 0.35], [1.0, 1.0, 1.0], [0.0, 0.1, 0.25]),
        5 => p([0.5, 0.45, 0.5], [0.5, 0.35, 0.5], [1.0, 1.0, 1.0], [0.5, 0.65, 0.95]),
        6 => p([0.5, 0.25, 0.6], [0.45, 0.25, 0.4], [1.0, 1.0, 1.0], [0.0, 0.25, 0.25]),
        7 => p([0.6, 0.45, 0.15], [0.4, 0.35, 0.15], [1.0, 1.0, 1.0], [0.0, 0.05, 0.1]),
        8 => p([0.5; 3], [0.5; 3], [2.0, 1.0, 0.0], [0.5, 0.2, 0.25]),
        9 => p([0.15, 0.45, 0.6], [0.15, 0.35, 0.4], [1.0, 1.0, 1.0], [0.3, 0.45, 0.5]),
        10 => p([0.7, 0.5, 0.7], [0.3, 0.4, 0.3], [1.0, 1.0, 1.0], [0.0, 0.33, 0.6]),
        11 => p([0.3, 0.5, 0.25], [0.3, 0.35, 0.2], [1.0, 1.0, 1.0], [0.1, 0.0, 0.2]),
        12 => p([0.5, 0.1, 0.15], [0.5, 0.1, 0.15], [1.0, 1.0, 1.0], [0.0, 0.1, 0.1]),
        13 => p([0.5, 0.52, 0.56], [0.4, 0.4, 0.42], [1.0, 1.0, 1.0], [0.0, 0.02, 0.05]),
        14 => p([0.3, 0.6, 0.5], [0.3, 0.4, 0.5], [1.0, 1.0, 1.0], [0.55, 0.3, 0.1]),
        _ => p([0.5; 3], [0.5; 3], [1.0, 0.7, 0.4], [0.0, 0.15, 0.2]),
    }
}

// ---------------------------------------------------------------- helpers for effects

/// Small deterministic PRNG (xorshift32).
pub struct Rng(pub u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9) | 1)
    }
    pub fn u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    /// 0..1
    pub fn f(&mut self) -> f32 {
        (self.u32() & 0xFF_FFFF) as f32 / 16_777_216.0
    }
    /// lo..hi
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
    /// 0..n
    pub fn below(&mut self, n: u32) -> u32 {
        self.u32() % n.max(1)
    }
}

pub type V3 = [f32; 3];

/// Rotate around x, then y, then z.
pub fn rot3(p: V3, a: V3) -> V3 {
    let (sx, cx) = a[0].sin_cos();
    let (sy, cy) = a[1].sin_cos();
    let (sz, cz) = a[2].sin_cos();
    let (x, y, z) = (p[0], p[1], p[2]);
    let (y, z) = (y * cx - z * sx, y * sx + z * cx);
    let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
    let (x, y) = (x * cz - y * sz, x * sz + y * cz);
    [x, y, z]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-6);
    [a[0] / l, a[1] / l, a[2] / l]
}
/// Perspective projection to screen pixels; the camera looks down +z.
pub fn project(p: V3, fov: f32) -> (f32, f32) {
    let z = p[2].max(0.05);
    (W as f32 / 2.0 + p[0] / z * fov, H as f32 / 2.0 + p[1] / z * fov)
}
#[inline]
pub fn fract(x: f32) -> f32 {
    x - x.floor()
}
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
/// Triangle wave 0..1..0 over one period of 1.0.
#[inline]
pub fn tri(x: f32) -> f32 {
    1.0 - (fract(x) * 2.0 - 1.0).abs()
}

/// Line with additive-free plain colour (Bresenham), clipped.
pub fn line(fb: &mut Fb, x0: f32, y0: f32, x1: f32, y1: f32, c: u32) {
    let (mut x0, mut y0, x1, y1) = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    for _ in 0..2000 {
        fb.put(x0, y0, c);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Fill the screen at reduced resolution: `f(x, y)` is called once per
/// `block`×`block` cell with that cell's top-left pixel position.
pub fn chunky(fb: &mut Fb, block: usize, mut f: impl FnMut(usize, usize) -> u32) {
    for by in (0..H).step_by(block) {
        for bx in (0..W).step_by(block) {
            let c = f(bx, by);
            for y in by..(by + block).min(H) {
                let row = y * W;
                fb.px[row + bx..row + (bx + block).min(W)].fill(c);
            }
        }
    }
}

/// A 256×256 texture: an XOR pattern with grid lines in palette colours.
pub fn xor_texture(pal: &Palette) -> Vec<u32> {
    let mut t = vec![0; 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            let v = (x ^ y) as f32 / 255.0;
            let mut c = pal.at(v * 0.6 + 0.1);
            if x % 32 < 2 || y % 32 < 2 {
                c = mix(c, rgb(255, 255, 255), 0.6);
            }
            t[y * 256 + x] = c;
        }
    }
    t
}

/// Text whose letters dance to the music: every letter bobs on its own sine wave
/// (higher with the bass), squashes and stretches, leans left and right, and
/// cycles through the palette; on each beat the next letter in turn jumps and
/// grows. `x`, `y` is the top-left corner at rest, `sc` the pixel size of the
/// 6×7 bold font. Returns the width at rest.
pub fn dancing_text(fb: &mut Fb, s: &str, x: f32, y: f32, sc: f32, t: f32, a: &Audio, beats: u32, pal: &Palette) -> f32 {
    use crate::font::{GH, bit};
    let chars: Vec<char> = s.chars().collect();
    let letters = chars.iter().filter(|c| **c != ' ').count().max(1) as u32;
    let star = beats % letters; // which letter takes the beat
    let adv = 7.0 * sc;
    let mut li = 0u32;
    for (i, &ch) in chars.iter().enumerate() {
        if ch == ' ' {
            continue;
        }
        let ph = t * 5.0 + i as f32 * 0.55;
        let hop = if li == star { a.pulse } else { 0.0 };
        li += 1;
        let squash = (ph * 2.0).sin() * 0.14;
        let grow = 1.0 + hop * 0.6;
        let (sx, sy) = (sc * (1.0 + squash) * grow, sc * (1.0 - squash) * grow);
        let lean = ph.sin() * 0.22;
        let cx = x + i as f32 * adv + 3.0 * sc;
        let base = y + 3.5 * sc - ph.sin() * sc * (0.8 + a.bass * 2.2) - hop * sc * 5.0;
        let col = pal.at(i as f32 * 0.06 + t * 0.25);
        let (w, h) = (sx.ceil() as i32, sy.ceil() as i32);
        for pass in 0..2 {
            let off = if pass == 0 { sc.max(1.0) } else { 0.0 };
            for gy in 0..GH {
                let fy = gy as f32 - 3.5;
                let top = gy < 2;
                let c = if pass == 0 { rgb(0, 0, 0) } else if top { mix(col, rgb(255, 255, 255), 0.55) } else { col };
                for gx in 0..6 {
                    if !bit(ch, gx, gy, true) {
                        continue;
                    }
                    let fx = gx as f32 - 3.0;
                    let px = cx + fx * sx - fy * sy * lean + off;
                    let py = base + fy * sy + off;
                    fb.fill_rect(px as i32, py as i32, w, h, c);
                }
            }
        }
    }
    chars.len() as f32 * adv - sc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noisy(a: &mut Audio, f: usize, seed: &mut u32) {
        for (i, b) in a.bins.iter_mut().enumerate() {
            *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let kick = if f % 30 < 3 && i < 8 { 200 } else { 0 };
            *b = ((*seed >> 24) as u8 / 3).saturating_add(kick);
        }
    }

    /// Every effect, several seeds and palettes, autopilot and live audio:
    /// no panics, something visible, and (in release builds) fast enough.
    #[test]
    fn every_effect_renders() {
        let defs: Vec<Def> = all().into_iter().chain(seasonal()).collect();
        let mut names = std::collections::HashSet::new();
        for d in &defs {
            assert!(names.insert(d.name), "duplicate effect name {}", d.name);
            assert!(d.name.len() <= 14 && !d.name.is_empty(), "bad name {:?}", d.name);
        }
        let mut seed = 7u32;
        let img = Image { px: xor_texture(&palette(3)), id: 1, caption: "THE ARTIST".into(), credit: "PHOTO: SOMEONE, CC BY-SA 4.0".into() };
        for (k, d) in defs.iter().enumerate() {
            let mut worst = 0.0f64;
            for (variant, live) in [(0u32, false), (1, true), (2, false), (3, true)] {
                let pal = palette(k + variant as usize * 5);
                let mut fx = (d.make)(variant * 7919 + 1);
                let mut fb = Fb::new();
                let mut a = Audio::new();
                let mut beats0 = None;
                let n = 150;
                let start = std::time::Instant::now();
                for f in 0..n {
                    let time = 50.0 + f as f32 / 60.0;
                    if live {
                        noisy(&mut a, f, &mut seed);
                    }
                    a.update(time, 1.0 / 60.0, live);
                    let b0 = *beats0.get_or_insert(a.beats);
                    let c = Ctx {
                        t: f as f32 / 60.0,
                        time,
                        dt: 1.0 / 60.0,
                        a: &a,
                        pal: &pal,
                        beats: a.beats - b0,
                        phase: 0.5,
                        name: "TEST DEMO",
                        text: "HELLO WORLD   ",
                        image: if variant >= 2 { Some(&img) } else { None },
                        song: 0,
                    };
                    fx.draw(&mut fb, &c);
                }
                let ms = start.elapsed().as_secs_f64() * 1000.0 / n as f64;
                worst = worst.max(ms);
                let lit = fb.px.iter().filter(|&&p| p & 0x00F0_F0F0 != 0).count();
                assert!(lit > W * H / 20, "{} (variant {variant}) is mostly black: {lit} lit pixels", d.name);
            }
            #[cfg(not(debug_assertions))]
            assert!(worst < 6.0, "{} takes {worst:.2} ms per frame (budget 6 ms)", d.name);
            let _ = worst;
        }
    }
}
