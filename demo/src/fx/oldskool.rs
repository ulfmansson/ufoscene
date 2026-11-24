//! Effects: oldskool — the raster and text tricks of the late 80s / early 90s
//! scene: raster and vertical copper bars, shadebobs, a sine dot plotter,
//! parallax scrolling, wavy checkerboards, two spectrum analysers, zooming and
//! exploding words, a perspective text crawl and two 8-bit pixel scenes.

use std::f32::consts::{PI, TAU};

use super::*;
use crate::audio::BINS;
use crate::font;

pub fn register(v: &mut Vec<Def>) {
    v.push(Def { name: "RASTER BARS", flags: RASTER | LOGO_TOP | SCROLL_BOTTOM, make: |s| Box::new(RasterBars::new(s)) });
    v.push(Def { name: "SHADEBOBS", flags: RASTER, make: |s| Box::new(Shadebobs::new(s)) });
    v.push(Def { name: "SINE DOTS", flags: RASTER, make: |s| Box::new(SineDots::new(s)) });
    v.push(Def { name: "PARALLAX", flags: RASTER | LOGO_TOP | SCROLL_BOTTOM, make: |s| Box::new(Parallax::new(s)) });
    v.push(Def { name: "CHECKER WAVE", flags: RASTER, make: |s| Box::new(CheckerWave::new(s)) });
    v.push(Def { name: "RADIAL EQ", flags: RASTER, make: |s| Box::new(RadialEq::new(s)) });
    v.push(Def { name: "ANALYZER", flags: RASTER | LOGO_TOP, make: |s| Box::new(Analyzer::new(s)) });
    v.push(Def { name: "TEXT ZOOM", flags: RASTER | OWN_TEXT, make: |s| Box::new(TextZoom::new(s)) });
    v.push(Def { name: "DOT LOGO", flags: RASTER | OWN_TEXT, make: |s| Box::new(DotLogo::new(s)) });
    v.push(Def { name: "VERTICAL BARS", flags: RASTER | LOGO_TOP | SCROLL_BOTTOM, make: |s| Box::new(VerticalBars::new(s)) });
    v.push(Def { name: "CRAWL", flags: RASTER | OWN_TEXT, make: |s| Box::new(Crawl::new(s)) });
    v.push(Def { name: "NIGHT CITY", flags: RETRO8 | LOGO_TOP | SCROLL_BOTTOM, make: |s| Box::new(NightCity::new(s)) });
    v.push(Def { name: "SNOWFALL", flags: RETRO8 | LOGO_TOP | SCROLL_BOTTOM, make: |s| Box::new(Snowfall::new(s)) });
}

// ---------------------------------------------------------------- shared helpers

#[inline]
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

#[inline]
fn h2(a: u32, b: u32) -> u32 {
    hash(a ^ hash(b).wrapping_add(0x9E37_79B9))
}

#[inline]
fn h3(a: u32, b: u32, c: u32) -> u32 {
    h2(h2(a, b), c)
}

/// Copper shading across a bar: `d` runs -1..1 from edge to edge.
fn bar_shade(c: u32, d: f32, glint: f32) -> u32 {
    let k = (d.clamp(-1.0, 1.0) * PI * 0.5).cos();
    add(scale(c, 0.12 + 0.88 * k), scale(rgb(255, 255, 255), k.powi(14) * glint))
}

/// 50% blend of two colours with plain bit arithmetic.
#[inline]
fn half_mix(a: u32, b: u32) -> u32 {
    0xFF00_0000 | (((a >> 1) & 0x7F7F7F) + ((b >> 1) & 0x7F7F7F))
}

/// `pal.at(x)`, pulled towards the accent colour where the palette runs dark,
/// so backgrounds built from it never drop to black.
fn lift(pal: &Palette, x: f32) -> u32 {
    let c = pal.at(x);
    let (r, g, b) = unpack(c);
    let lum = r.max(g).max(b);
    if lum < 0.6 { mix(c, pal.hi, (0.6 - lum) / 0.6) } else { c }
}

/// `out.len()` log-spaced spectrum bands in 0..1, bass first. Without music a
/// plausible moving spectrum is faked from the autopilot's bass/mid/treble.
fn spectrum(c: &Ctx, out: &mut [f32]) {
    let a = c.a;
    let n = out.len() as f32;
    for (i, o) in out.iter_mut().enumerate() {
        let f0 = i as f32 / n;
        if a.autopilot {
            let fi = i as f32;
            let wig = 0.65 + 0.35 * (fi * 1.7 + c.time * 5.3).sin() * (fi * 0.43 - c.time * 2.9).cos();
            let bell = (-((f0 - 0.35) * 4.0).powi(2)).exp();
            let v = a.bass * (1.0 - f0).powi(3) * 1.1 + a.mid * 0.75 * bell + a.treble * 0.55 * f0;
            *o = (v * wig).clamp(0.0, 1.0);
        } else {
            let f1 = (i + 1) as f32 / n;
            let hi = ((300f32.powf(f1) as usize) + 2).clamp(3, BINS);
            let lo = ((300f32.powf(f0) as usize) + 1).min(hi - 1);
            let (mut sum, mut max) = (0u32, 0u8);
            for &b in &a.bins[lo..hi] {
                sum += b as u32;
                max = max.max(b);
            }
            let avg = sum as f32 / (hi - lo) as f32;
            let v = (avg + max as f32) * 0.5 / 255.0;
            *o = (v * (1.0 + f0 * 1.2)).clamp(0.0, 1.0);
        }
    }
}

/// Per-pixel angle (0..255, 0 = up, clockwise) and radius around the screen centre.
struct Polar {
    ang: Vec<u8>,
    rad: Vec<u8>,
}

impl Polar {
    fn new() -> Self {
        let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
        let mut ang = vec![0; W * H];
        let mut rad = vec![0; W * H];
        for y in 0..H {
            for x in 0..W {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                ang[y * W + x] = (fract(dx.atan2(-dy) / TAU) * 256.0) as u8;
                rad[y * W + x] = dx.hypot(dy).min(255.0) as u8;
            }
        }
        Polar { ang, rad }
    }
}

/// Fill every row with `rows[y]`.
fn fill_rows(fb: &mut Fb, rows: &[u32]) {
    for (y, &c) in rows.iter().enumerate().take(H) {
        fb.px[y * W..(y + 1) * W].fill(c);
    }
}

/// Lit pixels of a word in the 5×7 font (bold or not), as font-pixel coordinates.
fn word_pixels(s: &str, bold: bool) -> Vec<(i32, i32)> {
    let adv = font::advance(bold);
    let mut out = Vec::new();
    for (i, ch) in s.chars().enumerate() {
        for gy in 0..font::GH {
            for gx in 0..adv - 1 {
                if font::bit(ch, gx, gy, bold) {
                    out.push((i as i32 * adv + gx, gy));
                }
            }
        }
    }
    out
}

const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Ordered dither: is `v` (0..1) above the threshold at this pixel?
#[inline]
fn dith(v: f32, x: usize, y: usize) -> bool {
    v * 16.0 > BAYER[y & 3][x & 3] as f32 + 0.5
}

/// Snap a colour to 3 bits per channel, for the 8-bit scenes.
fn q8(c: u32) -> u32 {
    let q = |v: u32| ((v & 0xFF) * 7 + 127) / 255 * 255 / 7;
    rgb(q(c) as u8, q(c >> 8) as u8, q(c >> 16) as u8)
}

/// Logical resolution of the 8-bit scenes (each pixel becomes 2×2).
const LW: usize = W / 2;
const LH: usize = H / 2;

/// Blow a 192×108 index buffer up to the framebuffer through a 16-colour palette.
fn blit2x(fb: &mut Fb, idx: &[u8], cols: &[u32; 16]) {
    for ly in 0..LH {
        let row = ly * 2 * W;
        for lx in 0..LW {
            let c = cols[(idx[ly * LW + lx] & 15) as usize];
            fb.px[row + lx * 2] = c;
            fb.px[row + lx * 2 + 1] = c;
        }
        fb.px.copy_within(row..row + W, row + W);
    }
}

// ---------------------------------------------------------------- RASTER BARS

/// Raster bars: horizontal copper bars weaving on sine paths (woven, mirrored,
/// interlaced or snake layouts) between two calm gradient bands.
struct RasterBars {
    mode: u32,
    n: usize,
    speed: f32,
    ph: f32,
    rows: Vec<u32>,
    alt: Vec<u32>,
    order: Vec<(f32, f32, f32)>,
}

const ZONE_TOP: usize = 42;
const ZONE_BOT: usize = 184;

impl RasterBars {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let mode = r.below(4);
        RasterBars {
            mode,
            n: [14, 8, 10, 22][mode as usize],
            speed: r.range(0.8, 1.25),
            ph: r.range(0.0, 20.0),
            rows: vec![0; H],
            alt: vec![0; H],
            order: Vec::with_capacity(32),
        }
    }
}

/// Paint `n` bars into `rows[top..bot]`, back to front. `snake` makes every bar
/// follow the same wave a little later instead of circling in depth.
#[allow(clippy::too_many_arguments)]
fn weave(rows: &mut [u32], order: &mut Vec<(f32, f32, f32)>, c: &Ctx, ph: f32, n: usize, top: f32, bot: f32, hue: f32, snake: bool) {
    let a = c.a;
    let (mid, amp) = ((top + bot) * 0.5, (bot - top) * 0.5);
    order.clear();
    for i in 0..n {
        let fi = i as f32;
        let (y, z) = if snake {
            let p = ph * 1.7 - fi * 0.27;
            (p.sin() * 0.7 + (p * 0.37 + fi * 0.05).sin() * 0.3, fi / n as f32 * 2.0 - 1.0)
        } else {
            let p = ph * 1.2 + fi * TAU / n as f32;
            (p.sin() * 0.72 + (ph * 0.53 + fi * 1.9).sin() * 0.28, p.cos())
        };
        order.push((z, y, hue + fi / n as f32 * 0.7));
    }
    order.sort_by(|p, q| p.0.total_cmp(&q.0));
    for &(z, y, h) in order.iter() {
        let depth = (z + 1.0) * 0.5;
        let half = if snake { 3.0 + a.bass * 3.0 } else { 3.5 + 4.5 * depth + a.bass * 4.0 };
        let y = mid + y * (amp - half);
        let col = add(scale(c.pal.at(h), 0.4 + 0.6 * depth), scale(c.pal.hi, a.pulse * 0.45 * depth));
        let y0 = ((y - half).floor() as i32).max(top as i32);
        let y1 = ((y + half).ceil() as i32).min(bot as i32 - 1);
        for yy in y0..=y1 {
            let d = (yy as f32 + 0.5 - y) / half;
            if d.abs() < 1.0 {
                rows[yy as usize] = bar_shade(col, d, 0.4 + 0.5 * depth);
            }
        }
    }
}

impl Effect for RasterBars {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.ph += c.dt * self.speed * (0.7 + 0.8 * a.mid);
        let hue = c.time * 0.03;
        let line = add(scale(c.pal.hi, 0.75), scale(rgb(255, 255, 255), a.pulse * 0.4));
        for y in 0..H {
            self.rows[y] = if y + 2 >= ZONE_TOP && y < ZONE_TOP || (ZONE_BOT..ZONE_BOT + 2).contains(&y) {
                line
            } else if !(ZONE_TOP..ZONE_BOT).contains(&y) {
                // calm stepped gradient, darkest towards the screen edge
                let d = if y < ZONE_TOP { ZONE_TOP - y } else { y - ZONE_BOT };
                scale(c.pal.at(0.6), (0.55 - (d / 5) as f32 * 0.06).max(0.14))
            } else {
                c.pal.bg
            };
        }
        let (top, bot) = (ZONE_TOP as f32, ZONE_BOT as f32 - 2.0);
        let (n, ph) = (self.n, self.ph);
        match self.mode {
            0 => weave(&mut self.rows, &mut self.order, c, ph, n, top, bot, hue, false),
            1 => {
                // bars in the upper half, a rippling darker reflection below
                let mid = ((top + bot) * 0.5) as usize;
                weave(&mut self.rows, &mut self.order, c, ph, n, top, mid as f32, hue, false);
                for y in mid..bot as usize {
                    let d = (y - mid) as f32;
                    let wob = ((d * 0.7 - ph * 9.0).sin() * (1.0 + d * 0.03)) as i32;
                    let src = (2 * mid as i32 - 1 - y as i32 + wob).clamp(top as i32, mid as i32 - 1);
                    self.rows[y] = half_mix(self.rows[src as usize], c.pal.bg);
                }
            }
            2 => {
                // two sets moving against each other on alternate scanlines
                self.alt.copy_from_slice(&self.rows);
                weave(&mut self.rows, &mut self.order, c, ph, n, top, bot, hue, false);
                weave(&mut self.alt, &mut self.order, c, -ph * 0.8 + 5.0, n, top, bot, hue + 0.5, false);
                for y in (ZONE_TOP + 1..ZONE_BOT).step_by(2) {
                    self.rows[y] = self.alt[y];
                }
            }
            _ => weave(&mut self.rows, &mut self.order, c, ph, n, top, bot, hue, true),
        }
        fill_rows(fb, &self.rows);
    }
}

// ---------------------------------------------------------------- SHADEBOBS

/// Shadebobs: discs stamped into a palette-index buffer, each pass adding to the
/// index so overlaps climb through the palette and slowly wrap around.
struct Shadebobs {
    buf: Vec<u8>,
    /// Half-width per row of a disc, for radii 6, 8, .. 28.
    discs: Vec<Vec<i32>>,
    bobs: Vec<[f32; 4]>,
    path: u32,
    step: [u8; 256],
    lut: [u32; 256],
    rng: Rng,
    last: u32,
    centre: f32,
}

const BOB_RAMP: usize = 14;

impl Shadebobs {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let path = rng.below(3);
        let n = [6, 8, 5][path as usize];
        let amt = 2 + rng.below(2) as usize;
        let discs = (0..12)
            .map(|k| {
                let r = 6 + k * 2;
                (-r..=r).map(|dy| (((r * r - dy * dy) as f32).sqrt()) as i32).collect()
            })
            .collect();
        let bobs = (0..n).map(|_| [rng.range(0.0, TAU), rng.range(0.0, TAU), rng.range(0.6, 1.5), rng.range(0.8, 1.9)]).collect();
        // index 0 is the empty background; first passes fade up, then cycle
        let mut step = [0u8; 256];
        for (v, s) in step.iter_mut().enumerate() {
            let nv = v + amt;
            *s = if nv > 255 { (BOB_RAMP + nv - 256) as u8 } else { nv as u8 };
        }
        Shadebobs { buf: vec![0; W * H], discs, bobs, path, step, lut: [0; 256], rng, last: 0, centre: 0.0 }
    }
}

impl Effect for Shadebobs {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        if c.beats != self.last {
            self.last = c.beats;
            // every 8 beats the bobs pick new speeds (positions stay continuous)
            if c.beats % 8 == 0 {
                for b in self.bobs.iter_mut() {
                    b[2] = self.rng.range(0.6, 1.5);
                    b[3] = self.rng.range(0.8, 1.9);
                }
            }
        }
        let sp = c.dt * (0.7 + 0.9 * a.mid);
        self.centre += sp * 0.6;
        let n = self.bobs.len();
        let ri = ((0.25 + a.bass * 0.75) * (self.discs.len() - 1) as f32) as usize;
        let disc = &self.discs[ri.min(self.discs.len() - 1)];
        let r = (disc.len() / 2) as i32;
        let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
        for (j, b) in self.bobs.iter_mut().enumerate() {
            b[0] += sp * b[2];
            b[1] += sp * b[3];
            let o = j as f32 * TAU / n as f32;
            let (x, y) = match self.path {
                0 => (cx + 160.0 * b[0].sin(), cy + 82.0 * b[1].sin()),
                1 => {
                    let rr = 40.0 + 45.0 * (self.centre * 0.9).sin();
                    let an = self.centre * 1.8 + o;
                    (cx + 90.0 * (self.centre * 0.7).sin() + rr * an.cos(), cy + 40.0 * (self.centre * 1.1).sin() + rr * an.sin())
                }
                _ => (
                    cx + 105.0 * (b[0] + o).cos() + 50.0 * (b[1] * 2.0 - o).cos(),
                    cy + 55.0 * (b[1] + o).sin() + 28.0 * (b[0] * 1.7).sin(),
                ),
            };
            let (bx, by) = (x as i32, y as i32);
            for (k, &hw) in disc.iter().enumerate() {
                let yy = by - r + k as i32;
                if !(0..HI).contains(&yy) {
                    continue;
                }
                let row = yy as usize * W;
                for xx in (bx - hw).max(0)..(bx + hw + 1).min(WI) {
                    let i = row + xx as usize;
                    self.buf[i] = self.step[self.buf[i] as usize];
                }
            }
        }
        // palette: background, a fade-up ramp, then one seamless cycle
        let off = c.time * 0.02;
        let flash = scale(rgb(255, 255, 255), a.pulse * 0.25);
        self.lut[0] = c.pal.bg;
        for i in 1..256 {
            self.lut[i] = if i < BOB_RAMP {
                scale(c.pal.at(off), i as f32 / BOB_RAMP as f32)
            } else {
                add(c.pal.at(off + (i - BOB_RAMP) as f32 / (256 - BOB_RAMP) as f32), flash)
            };
        }
        for (p, &v) in fb.px.iter_mut().zip(&self.buf) {
            *p = self.lut[v as usize];
        }
    }
}

// ---------------------------------------------------------------- SINE DOTS

/// Sine dots: a big matrix of dots displaced by combined sine waves — a flat
/// plotter grid, a rolling dot landscape or a flag-like sheet turning in 3D.
struct SineDots {
    mode: u32,
    ph: f32,
    k: [f32; 4],
    pts: Vec<(f32, f32, f32, f32, f32)>,
}

impl SineDots {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let mode = r.below(3);
        let k = [r.range(0.3, 0.5), r.range(0.18, 0.3), r.range(0.25, 0.42), r.range(0.08, 0.16)];
        SineDots { mode, ph: r.range(0.0, 10.0), k, pts: Vec::with_capacity(1600) }
    }
}

/// A small round dot with a bright core, radius 1..3.
fn dot(fb: &mut Fb, x: f32, y: f32, r: i32, col: u32) {
    let (xi, yi) = (x as i32, y as i32);
    match r {
        0 | 1 => {
            let edge = scale(col, 0.75);
            fb.put(xi, yi, col);
            fb.put(xi + 1, yi, edge);
            fb.put(xi, yi + 1, edge);
            fb.put(xi + 1, yi + 1, scale(col, 0.5));
        }
        _ => {
            let edge = scale(col, 0.55);
            let core = mix(col, rgb(255, 255, 255), 0.45);
            for dy in -r..=r {
                for dx in -r..=r {
                    let d2 = dx * dx + dy * dy;
                    if d2 <= r * r {
                        let c = if d2 <= 1 && dx <= 0 && dy <= 0 { core } else if d2 * 2 > r * r { edge } else { col };
                        fb.put(xi + dx, yi + dy, c);
                    }
                }
            }
        }
    }
}

impl Effect for SineDots {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.ph += c.dt * (0.8 + 0.7 * a.mid);
        let (ph, k) = (self.ph, self.k);
        fb.clear(c.pal.bg);
        if self.mode == 1 {
            // a banded glow over the horizon behind the landscape
            let glow = scale(lift(c.pal, 0.6 + c.time * 0.02), 0.45 + 0.2 * a.pulse);
            for y in 20..96i32 {
                fb.row(y, mix(c.pal.bg, glow, ((y - 20) / 8) as f32 / 9.0));
            }
        }
        // a beat sends a ring of displacement out from the centre
        let ring = (1.0 - a.pulse) * 300.0;
        let amp = 0.6 + 0.8 * a.bass;
        self.pts.clear();
        match self.mode {
            0 => {
                let (nx, ny, sp) = (44, 25, 8.6);
                let (ox, oy) = ((W as f32 - (nx - 1) as f32 * sp) / 2.0, (H as f32 - (ny - 1) as f32 * sp) / 2.0);
                for gy in 0..ny {
                    for gx in 0..nx {
                        let (fx, fy) = (gx as f32, gy as f32);
                        let (x0, y0) = (ox + fx * sp, oy + fy * sp);
                        let w1 = (fy * k[0] + ph * 1.7).sin() + 0.6 * ((fx + fy) * k[1] - ph * 1.1).sin();
                        let w2 = (fx * k[2] + ph * 1.3).cos() + 0.5 * (fx * k[3] - fy * k[0] + ph).sin();
                        let (rx, ry) = (x0 - W as f32 / 2.0, y0 - H as f32 / 2.0);
                        let rd = rx.hypot(ry).max(1.0);
                        let push = a.pulse * 12.0 * (-((rd - ring) / 22.0).powi(2)).exp();
                        let x = x0 + w1 * 7.0 * amp + rx / rd * push;
                        let y = y0 + w2 * 6.0 * amp + ry / rd * push;
                        let z = (w1 + w2) * 0.16 + 0.5;
                        self.pts.push((0.0, x, y, z, fx / nx as f32));
                    }
                }
            }
            1 => {
                // a dot landscape rolling towards the camera
                let (nx, nz) = (48, 30);
                let scroll = ph * 0.6;
                let (z0, dz) = (1.3, 0.2);
                for gz in (0..nz).rev() {
                    let z = z0 + (gz as f32 - fract(scroll)) * dz;
                    let wz = (gz as f32 + scroll.floor()) * dz;
                    for gx in 0..nx {
                        let x = (gx as f32 / (nx - 1) as f32 - 0.5) * 4.4;
                        let h = 0.22
                            * amp
                            * ((x * 2.1 + ph * 1.1).sin() + (wz * 1.9 - ph * 0.7).sin() + 0.5 * ((x + wz) * 3.3 + ph).sin());
                        let rd = (x * x + (z - 2.5).powi(2)).sqrt() * 70.0;
                        let h = h - a.pulse * 0.25 * (-((rd - ring * 0.6) / 25.0).powi(2)).exp();
                        let (sx, sy) = project([x, 0.55 + h, z], 150.0);
                        self.pts.push((z, sx, sy - 30.0, 0.5 - h * 1.2, gx as f32 / nx as f32));
                    }
                }
            }
            _ => {
                // a flag of dots turning in 3D
                let (nx, ny) = (36, 20);
                let rot = [0.5 + (ph * 0.37).sin() * 0.4, (ph * 0.29).sin() * 0.8, (ph * 0.21).sin() * 0.25];
                for gy in 0..ny {
                    for gx in 0..nx {
                        let (u, v) = (gx as f32 / (nx - 1) as f32 - 0.5, gy as f32 / (ny - 1) as f32 - 0.5);
                        let (x, y) = (u * 3.4, v * 1.9);
                        let rd = (x * x + y * y).sqrt() * 80.0;
                        let zw = 0.28 * amp * ((x * 2.4 + ph * 2.0).sin() * (y * 2.8 - ph * 1.3).cos())
                            + a.pulse * 0.3 * (-((rd - ring * 0.5) / 20.0).powi(2)).exp();
                        let p = rot3([x, y, zw], rot);
                        let z = p[2] + 3.3;
                        let (sx, sy) = project([p[0], p[1], z], 190.0);
                        self.pts.push((-z, sx, sy, zw * 1.6 + 0.5, u + 0.5));
                    }
                }
                self.pts.sort_unstable_by(|p, q| p.0.total_cmp(&q.0));
            }
        }
        let hue = c.time * 0.05;
        let sparkle = (a.treble * a.treble * 40.0) as u32;
        let tick = (c.time * 12.0) as u32;
        for (i, &(depth, x, y, z, u)) in self.pts.iter().enumerate() {
            let near = if self.mode == 1 { (1.0 - (depth - 1.3) / 6.0).clamp(0.25, 1.0) } else { 1.0 };
            let zc = z.clamp(0.0, 1.0);
            let mut col = scale(lift(c.pal, hue + u * 0.5 + z * 0.35), (0.7 + 0.5 * zc) * near);
            if zc > 0.8 {
                col = mix(col, rgb(255, 255, 255), (zc - 0.8) * 2.0);
            }
            if h2(i as u32, tick) % 1000 < sparkle {
                col = rgb(255, 255, 255);
            }
            let r = match self.mode {
                0 => (z * 2.6) as i32,
                1 => (4.2 / depth) as i32,
                _ => ((z + 0.4) * 1.9) as i32,
            };
            dot(fb, x, y, r.clamp(1, 3), col);
        }
    }
}

// ---------------------------------------------------------------- PARALLAX

/// Parallax: a side-scrolling landscape of layers at different speeds — stars,
/// a moon or striped sun, far mountains, clouds, hills and a fast ground strip.
struct Parallax {
    sunset: bool,
    dir: f32,
    pos: f32,
    /// Ridge heights (screen y) of three layers, 1024 entries each, seamless.
    ridge: [Vec<i16>; 3],
    stars: Vec<(f32, f32, u32)>,
    clouds: Vec<(f32, f32, f32)>,
    sky: Vec<u32>,
    tabs: [Vec<u32>; 3],
}

const SKY_HOR: usize = 150;

impl Parallax {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let sunset = r.below(2) == 1;
        let dir = if r.below(3) == 0 { -1.0 } else { 1.0 };
        let mut layer = |base: f32, amp: f32, ridged: bool, freqs: [f32; 4]| -> Vec<i16> {
            let ph: Vec<f32> = (0..4).map(|_| r.range(0.0, TAU)).collect();
            (0..1024)
                .map(|i| {
                    let x = i as f32 / 1024.0 * TAU;
                    let mut v = 0.0;
                    for k in 0..4 {
                        let s = (x * freqs[k] + ph[k]).sin();
                        v += if ridged { (1.0 - s.abs()) * 2.0 - 1.0 } else { s } / (k + 1) as f32;
                    }
                    (base - v * amp / 1.6) as i16
                })
                .collect()
        };
        let ridge = [
            layer(112.0, 46.0, true, [3.0, 7.0, 13.0, 29.0]),
            layer(150.0, 22.0, false, [4.0, 9.0, 17.0, 33.0]),
            layer(178.0, 12.0, false, [6.0, 11.0, 23.0, 41.0]),
        ];
        let stars = (0..90).map(|i| (r.range(0.0, W as f32), r.range(0.0, 120.0), 1 + i % 3)).collect();
        let clouds = (0..7).map(|_| (r.range(0.0, 1024.0), r.range(38.0, 100.0), r.range(0.7, 1.3))).collect();
        Parallax { sunset, dir, pos: 0.0, ridge, stars, clouds, sky: vec![0; H], tabs: [vec![0; H], vec![0; H], vec![0; H]] }
    }
}

impl Effect for Parallax {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.pos += c.dt * (45.0 + 90.0 * a.bass) * self.dir;
        let pal = c.pal;
        // stepped sky: a dozen bands from deep blue-ish to a bright horizon
        let (top, hor) = if self.sunset {
            (scale(lift(pal, 0.7), 0.3), mix(lift(pal, 0.05), rgb(255, 230, 180), 0.3))
        } else {
            (scale(lift(pal, 0.65), 0.14), scale(lift(pal, 0.55), 0.6))
        };
        for y in 0..H {
            let band = (y * 12 / SKY_HOR).min(11) as f32 / 11.0;
            self.sky[y] = mix(top, hor, band * band);
        }
        fill_rows(fb, &self.sky);
        // stars, three speeds
        if !self.sunset {
            for (i, &(x, y, l)) in self.stars.iter().enumerate() {
                let sx = (x - self.pos * 0.02 * l as f32).rem_euclid(W as f32) as i32;
                let tw = 0.4 + 0.6 * ((c.time * 3.0 + i as f32 * 1.3).sin() * 0.5 + 0.5) * (0.5 + a.treble);
                fb.put(sx, y as i32, scale(rgb(255, 255, 255), tw * l as f32 / 3.0 + 0.2));
            }
        }
        // moon or striped sun, far away so it stays put
        let (mx, my, mr) = (W as f32 * 0.72, if self.sunset { 108.0 } else { 52.0 }, if self.sunset { 40.0 } else { 20.0 });
        let glow = mr + 4.0 + a.pulse * 8.0;
        for y in (my - glow) as i32..=(my + glow) as i32 {
            let dy = y as f32 - my;
            let half = (glow * glow - dy * dy).max(0.0).sqrt();
            fb.hspan_blend((mx - half) as i32, (mx + half) as i32, y, pal.hi, 0.25);
            if dy.abs() < mr {
                let half = (mr * mr - dy * dy).sqrt();
                let cut = self.sunset && dy > 0.0 && ((dy as i32 + (c.time * 8.0) as i32) % 9) < 2 + (dy / 8.0) as i32;
                if !cut {
                    let col = if self.sunset {
                        mix(mix(pal.hi, rgb(255, 255, 220), 0.5), lift(pal, 0.02), (dy / mr + 1.0) * 0.5)
                    } else {
                        mix(pal.hi, rgb(255, 255, 255), 0.5)
                    };
                    fb.hspan((mx - half) as i32, (mx + half) as i32, y, col);
                }
            }
        }
        // per-layer colour by depth below the ridge: lit edge, then banded body
        let haze = self.sky[SKY_HOR - 20];
        for (l, tab) in self.tabs.iter_mut().enumerate() {
            let base = scale(lift(pal, 0.3 + l as f32 * 0.12), [0.4, 0.6, 0.85][l]);
            let base = mix(base, haze, [0.55, 0.25, 0.0][l]);
            let edge = add(mix(base, rgb(255, 255, 255), 0.45), scale(pal.hi, a.pulse * 0.3));
            for (d, t) in tab.iter_mut().enumerate() {
                *t = if d < 2 { edge } else { scale(base, 1.0 - (d / 8).min(6) as f32 * 0.07) };
            }
        }
        let speeds = [0.2, 0.5, 0.8];
        for l in 0..3 {
            if l == 1 {
                // clouds drift between the mountains and the hills
                for &(x, y, s) in &self.clouds {
                    let cx = (x - self.pos * 0.32).rem_euclid(1024.0) - 120.0;
                    if cx > W as f32 + 60.0 {
                        continue;
                    }
                    let col = mix(self.sky[y as usize], rgb(255, 255, 255), 0.55);
                    let shade = mix(self.sky[y as usize], rgb(255, 255, 255), 0.3);
                    for (px, py, pr) in [(0.0, 0.0, 14.0), (16.0, -6.0, 12.0), (30.0, 0.0, 15.0), (-14.0, 2.0, 9.0), (46.0, 3.0, 9.0)] {
                        let (ccx, ccy, r) = (cx + px * s, y + py * s, pr * s);
                        for yy in (ccy - r * 0.6) as i32..(ccy + r * 0.6) as i32 {
                            let dy = (yy as f32 - ccy) / (r * 0.6);
                            let half = (1.0 - dy * dy).max(0.0).sqrt() * r;
                            fb.hspan((ccx - half) as i32, (ccx + half) as i32, yy, if dy > 0.45 { shade } else { col });
                        }
                    }
                }
            }
            let off = (self.pos * speeds[l]) as i32;
            let tab = &self.tabs[l];
            for x in 0..W {
                let top = self.ridge[l][((x as i32 + off) & 1023) as usize].max(0) as usize;
                for y in top.min(H)..H {
                    fb.px[y * W + x] = tab[y - top];
                }
            }
        }
        // fast ground strip with fence posts
        let off = self.pos as i32;
        let g0 = scale(lift(pal, 0.35), 0.55);
        let g1 = scale(lift(pal, 0.35), 0.38);
        for y in 198..H {
            for x in 0..W {
                let stripe = ((x as i32 + off).rem_euclid(24) < 12) ^ (y > 206);
                fb.px[y * W + x] = if y == 198 { scale(pal.hi, 0.8) } else if stripe { g0 } else { g1 };
            }
        }
        let poff = (self.pos * 1.5) as i32;
        for k in 0..9 {
            let x = k * 56 - poff.rem_euclid(56);
            fb.fill_rect(x, 180, 4, 18, scale(pal.bg, 0.5));
            fb.fill_rect(x - 1, 178, 6, 2, scale(pal.bg, 0.5));
        }
        fb.fill_rect(0, 185, WI, 2, scale(pal.bg, 0.6));
    }
}

// ---------------------------------------------------------------- CHECKER WAVE

/// Checker wave: an Amiga checkerboard whose rows and columns bob in sine
/// waves — flat and wobbling, as a rolling floor and ceiling, or rotozoomed.
struct CheckerWave {
    mode: u32,
    ph: f32,
    rowoff: Vec<f32>,
    rowl: Vec<u8>,
    coloff: Vec<f32>,
    coll: Vec<u8>,
    cols: [[u32; 16]; 2],
    cols2: [[u32; 16]; 2],
}

impl CheckerWave {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        CheckerWave {
            mode: r.below(3),
            ph: r.range(0.0, 10.0),
            rowoff: vec![0.0; H],
            rowl: vec![0; H],
            coloff: vec![0.0; W],
            coll: vec![0; W],
            cols: [[0; 16]; 2],
            cols2: [[0; 16]; 2],
        }
    }
}

fn shade_table(out: &mut [[u32; 16]; 2], a: u32, b: u32) {
    for l in 0..16 {
        let k = 0.3 + 0.7 * l as f32 / 15.0;
        let glint = if l >= 14 { 0.25 } else { 0.0 };
        out[0][l] = add(scale(a, k), scale(rgb(255, 255, 255), glint));
        out[1][l] = add(scale(b, k), scale(rgb(255, 255, 255), glint * 0.5));
    }
}

impl Effect for CheckerWave {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.ph += c.dt * (0.8 + 0.8 * a.mid);
        let ph = self.ph;
        let hue = c.time * 0.04;
        let light = add(c.pal.at(hue + 0.1), scale(rgb(255, 255, 255), a.pulse * 0.3));
        shade_table(&mut self.cols, light, scale(c.pal.at(hue + 0.6), 0.35));
        shade_table(&mut self.cols2, c.pal.at(hue + 0.45), scale(c.pal.at(hue + 0.9), 0.3));
        let size = 22.0 - 5.0 * a.pulse;
        let inv = 1.0 / size;
        match self.mode {
            0 => {
                let (ax, ay) = (14.0 + 28.0 * a.bass, 8.0 + 16.0 * a.mid);
                for y in 0..H {
                    let p = y as f32 * 0.045 + ph * 2.0;
                    self.rowoff[y] = ax * p.sin() + ax * 0.4 * (y as f32 * 0.017 - ph * 1.3).sin() + ph * 30.0;
                    self.rowl[y] = (4.0 + 3.5 * p.cos()) as u8;
                }
                for x in 0..W {
                    let p = x as f32 * 0.031 + ph * 1.6;
                    self.coloff[x] = ay * p.sin() + ph * 12.0;
                    self.coll[x] = (4.0 + 3.5 * p.cos()) as u8;
                }
                for y in 0..H {
                    let (ro, rl) = (self.rowoff[y] + 4000.0, self.rowl[y]);
                    let row = &mut fb.px[y * W..(y + 1) * W];
                    for (x, p) in row.iter_mut().enumerate() {
                        let u = ((x as f32 + ro) * inv) as i32;
                        let v = ((y as f32 + self.coloff[x] + 4000.0) * inv) as i32;
                        *p = self.cols[((u ^ v) & 1) as usize][(rl + self.coll[x]) as usize];
                    }
                }
            }
            1 => {
                // floor and ceiling in perspective, rows rolling sideways
                let hy = H as f32 / 2.0;
                let glow = add(c.pal.hi, scale(rgb(255, 255, 255), a.pulse * 0.5));
                for y in 0..H {
                    let d = (y as f32 + 0.5 - hy).abs();
                    let row = &mut fb.px[y * W..(y + 1) * W];
                    if d < 3.0 {
                        row.fill(scale(glow, 1.0 - d / 4.0));
                        continue;
                    }
                    let z = 1400.0 / d;
                    let roll = (z * 0.012 + ph * 2.2).sin() * (20.0 + 40.0 * a.bass);
                    let v = ((z + ph * 160.0) * inv) as i32;
                    let du = z / 160.0;
                    let mut u = (-(W as f32) / 2.0) * du + roll + ph * 25.0 + 8000.0;
                    let fog = (15.0 - z / 70.0).clamp(0.0, 15.0) as usize;
                    let tab = if y as f32 > hy { &self.cols } else { &self.cols2 };
                    for p in row.iter_mut() {
                        let ui = (u * inv) as i32;
                        *p = tab[((ui ^ v) & 1) as usize][fog];
                        u += du;
                    }
                }
            }
            _ => {
                // rotating, zooming board with sine-bent rows
                let ang = ph * 0.3 + (ph * 0.7).sin() * 0.5;
                let zoom = 1.0 + 0.3 * (ph * 0.5).sin() + 0.25 * a.pulse;
                let (s, co) = (ang.sin() / zoom, ang.cos() / zoom);
                let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
                let amp = 10.0 + 24.0 * a.bass;
                for y in 0..H {
                    let p = y as f32 * 0.05 + ph * 2.0;
                    let bend = p.sin() * amp;
                    let l = (8.0 + 7.0 * p.cos()) as usize;
                    let fy = y as f32 - cy;
                    let mut u = (-cx) * co - fy * s + bend + 8000.0;
                    let mut v = (-cx) * s + fy * co + 8000.0;
                    let row = &mut fb.px[y * W..(y + 1) * W];
                    for p in row.iter_mut() {
                        let (ui, vi) = ((u * inv) as i32, (v * inv) as i32);
                        *p = self.cols[((ui ^ vi) & 1) as usize][l];
                        u += co;
                        v += s;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- RADIAL EQ

/// Radial EQ: a circular spectrum analyser — rays or ring arcs around a pulsing
/// centre disc, with falling peak markers.
struct RadialEq {
    mode: u32,
    polar: Polar,
    nb: usize,
    lvl: [f32; 64],
    peak: [f32; 64],
    pvel: [f32; 64],
    tmp: [f32; 64],
    rot: f32,
    spin: f32,
}

impl RadialEq {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let mode = r.below(3);
        RadialEq {
            mode,
            polar: Polar::new(),
            nb: [32, 64, 18][mode as usize],
            lvl: [0.0; 64],
            peak: [0.0; 64],
            pvel: [0.0; 64],
            tmp: [0.0; 64],
            rot: r.range(0.0, 256.0),
            spin: if r.below(2) == 0 { 1.0 } else { -1.0 },
        }
    }
}

impl Effect for RadialEq {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let nb = self.nb;
        spectrum(c, &mut self.tmp[..nb]);
        for b in 0..nb {
            self.lvl[b] = self.tmp[b].max(self.lvl[b] - c.dt * 1.8);
            if self.lvl[b] >= self.peak[b] {
                self.peak[b] = self.lvl[b];
                self.pvel[b] = 0.0;
            } else {
                self.pvel[b] += c.dt * 1.2;
                self.peak[b] = (self.peak[b] - self.pvel[b] * c.dt).max(self.lvl[b]);
            }
        }
        self.rot = (self.rot + c.dt * self.spin * (6.0 + 30.0 * a.mid)).rem_euclid(256.0);
        let rot = self.rot as u8;
        let r0 = (30.0 + a.bass * 8.0 + a.pulse * 5.0) as i32;
        let hue = c.time * 0.04;
        // per-radius background and centre disc colours
        let mut bg = [0u32; 256];
        let mut disc = [0u32; 256];
        let ring = ((1.0 - a.pulse) * 200.0) as i32 + r0;
        for r in 0..256 {
            let ri = r as i32;
            let k = (0.22 - r as f32 * 0.0008).max(0.06);
            bg[r] = if (ri - ring).abs() < 2 && a.pulse > 0.05 {
                scale(c.pal.hi, a.pulse * 0.7)
            } else if r % 24 == 0 {
                scale(c.pal.at(hue + 0.5), k * 1.8)
            } else {
                scale(c.pal.at(hue + 0.5), k)
            };
            disc[r] = c.pal.ramp(0.95 - r as f32 / r0.max(1) as f32 * (0.55 - a.pulse * 0.3));
        }
        let rim = add(c.pal.hi, scale(rgb(255, 255, 255), a.pulse * 0.5));
        let mut shade = [[0u32; 4]; 64];
        let mut tipc = [0u32; 64];
        let mut tip = [0i32; 64];
        let mut pk = [0i32; 64];
        let lmax = 130.0;
        for b in 0..nb {
            let col = c.pal.at(hue + b as f32 / nb as f32 * 0.6);
            for (s, sh) in shade[b].iter_mut().enumerate() {
                *sh = scale(col, 0.45 + 0.2 * s as f32);
            }
            tipc[b] = mix(col, rgb(255, 255, 255), 0.6);
            if self.mode == 2 {
                tip[b] = (self.lvl[b] * 128.0) as i32;
                pk[b] = (self.peak[b] * 128.0) as i32;
            } else {
                tip[b] = r0 + 3 + (self.lvl[b] * lmax) as i32;
                pk[b] = r0 + 5 + (self.peak[b] * lmax) as i32;
            }
        }
        let mode = self.mode;
        for ((p, &an), &rd) in fb.px.iter_mut().zip(&self.polar.ang).zip(&self.polar.rad) {
            let r = rd as i32;
            if r < r0 - 3 {
                *p = disc[rd as usize];
                continue;
            }
            if r < r0 {
                *p = rim;
                continue;
            }
            let a8 = an.wrapping_add(rot);
            let m = if a8 < 128 { a8 } else { 255 - a8 };
            *p = if mode == 2 {
                // concentric arcs growing from the top down both sides
                let k = (r - r0 - 4).max(0);
                let band = (k / 7) as usize;
                if r < r0 + 4 || band >= nb || k % 7 == 6 {
                    bg[rd as usize]
                } else if (m as i32) < tip[band] {
                    shade[band][((m as i32) * 4 / 128).min(3) as usize]
                } else if (m as i32 - pk[band]).abs() <= 1 {
                    rim
                } else {
                    bg[rd as usize]
                }
            } else {
                let (band, gap) = if mode == 0 { ((m >> 2) as usize, m & 3 == 3) } else { ((a8 >> 2) as usize, a8 & 3 == 3) };
                if gap {
                    bg[rd as usize]
                } else if r <= tip[band] {
                    if r > tip[band] - 3 { tipc[band] } else { shade[band][((r - r0) >> 5).min(3) as usize] }
                } else if (r - pk[band]).abs() <= 1 {
                    rim
                } else {
                    bg[rd as usize]
                }
            };
        }
    }
}

// ---------------------------------------------------------------- ANALYZER

/// Analyzer: a classic bar spectrum analyser with falling peak caps, standing
/// on a floor line with a rippling mirrored reflection underneath.
struct Analyzer {
    nb: usize,
    seg: bool,
    centred: bool,
    by_band: bool,
    lvl: Vec<f32>,
    peak: Vec<f32>,
    hold: Vec<f32>,
    pvel: Vec<f32>,
    tmp: Vec<f32>,
}

const FLOOR: i32 = 154;
const BAR_MAX: f32 = 110.0;

impl Analyzer {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let nb = [24, 32, 44][r.below(3) as usize];
        Analyzer {
            nb,
            seg: r.below(2) == 0,
            centred: r.below(3) == 0,
            by_band: r.below(2) == 0,
            lvl: vec![0.0; nb],
            peak: vec![0.0; nb],
            hold: vec![0.0; nb],
            pvel: vec![0.0; nb],
            tmp: vec![0.0; nb],
        }
    }
}

impl Effect for Analyzer {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let nb = self.nb;
        spectrum(c, &mut self.tmp);
        for b in 0..nb {
            self.lvl[b] = self.tmp[b].max(self.lvl[b] - c.dt * 2.2);
            if self.lvl[b] >= self.peak[b] {
                self.peak[b] = self.lvl[b];
                self.pvel[b] = 0.0;
                self.hold[b] = 0.35;
            } else if self.hold[b] > 0.0 {
                self.hold[b] -= c.dt;
            } else {
                self.pvel[b] += c.dt * 2.5;
                self.peak[b] = (self.peak[b] - self.pvel[b] * c.dt).max(self.lvl[b]);
            }
        }
        let hue = c.time * 0.03;
        let bg = c.pal.bg;
        let grid = mix(bg, c.pal.at(hue + 0.5), 0.18);
        for y in 0..FLOOR {
            let col = if (FLOOR - y) % 12 == 0 { grid } else { bg };
            fb.px[y as usize * W..(y as usize + 1) * W].fill(col);
        }
        let floor = add(scale(c.pal.hi, 0.7), scale(rgb(255, 255, 255), a.pulse * 0.5));
        fb.row(FLOOR, floor);
        let bw = 352 / nb as i32;
        let x0 = (WI - bw * nb as i32) / 2;
        let gap = if bw > 9 { 2 } else { 1 };
        let peakc = mix(c.pal.hi, rgb(255, 255, 255), 0.6);
        for s in 0..nb {
            // centred: bass in the middle, highs out to both sides
            let b = if self.centred {
                let d = (2 * s as i32 - nb as i32 + 1).unsigned_abs() as usize / 2;
                (d * 2).min(nb - 1)
            } else {
                s
            };
            let h = (self.lvl[b] * BAR_MAX) as i32;
            let xs = x0 + s as i32 * bw;
            let band_col = c.pal.at(hue + b as f32 / nb as f32 * 0.7);
            for k in 0..h {
                if self.seg && k % 4 == 3 {
                    continue;
                }
                let f = k as f32 / BAR_MAX;
                let col = if self.by_band {
                    scale(band_col, 0.55 + 0.6 * f)
                } else {
                    c.pal.at(0.1 + f * 0.6)
                };
                let col = if k >= h - 2 { mix(col, rgb(255, 255, 255), 0.5) } else { col };
                fb.hspan(xs, xs + bw - gap, FLOOR - 1 - k, col);
            }
            let py = FLOOR - 4 - (self.peak[b] * BAR_MAX) as i32;
            fb.fill_rect(xs, py, bw - gap, 2, peakc);
        }
        // reflection: squashed, rippling, half-blended into the background
        for y in FLOOR + 1..HI {
            let d = (y - FLOOR) as f32;
            let row = y as usize * W;
            if d > 44.0 {
                fb.px[row..row + W].fill(bg);
                continue;
            }
            let sy = (FLOOR as f32 - d * 1.5).max(0.0) as usize;
            let wob = ((d * 0.55 - c.time * 7.0).sin() * (1.0 + d * 0.05)) as i32;
            let dark = d > 22.0;
            for x in 0..W {
                let sx = (x as i32 + wob).clamp(0, WI - 1) as usize;
                let m = half_mix(fb.px[sy * W + sx], bg);
                fb.px[row + x] = if dark { half_mix(m, bg) } else { m };
            }
        }
    }
}

// ---------------------------------------------------------------- TEXT ZOOM

const ZOOM_WORDS: [&[&str]; 3] = [
    &["HEJ!", "DANSA", "TAKTEN", "BASEN", "LJUSET", "MER!", "HÖGRE", "KOM IGEN", "SKÅL!", "NATT"],
    &["HELLO", "JUMP", "LOUDER", "BASS", "PIXELS", "MORE!", "GROOVE", "NO SLEEP", "YEAH!", "AGAIN"],
    &["UFO", "BEAT", "TAKT", "COPPER", "RASTER", "SINUS", "LJUD", "PARTY", "NU!", "BOOM"],
];

/// Text zoom: one big pixel-font word per beat, punching in or flying in over
/// a sunburst, ring or polar-checker background, with echo copies behind it.
struct TextZoom {
    words: &'static [&'static str],
    mode: u32,
    bgmode: u32,
    every: u32,
    polar: Polar,
    ph: f32,
}

impl TextZoom {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        TextZoom {
            words: ZOOM_WORDS[r.below(3) as usize],
            mode: r.below(3),
            bgmode: r.below(3),
            every: 1 + r.below(2),
            polar: Polar::new(),
            ph: 0.0,
        }
    }
}

/// Draw `s` in the bold font at `sc` pixels per font pixel, centred on (cx, cy),
/// each font row shifted by `wob(row)`, coloured per row by `col(row)`.
#[allow(clippy::too_many_arguments)]
fn big_word(fb: &mut Fb, s: &str, cx: f32, cy: f32, sc: f32, grow: f32, wob: impl Fn(i32) -> f32, col: impl Fn(i32) -> u32) {
    let n = s.chars().count() as f32;
    let x0 = cx - (n * 7.0 - 1.0) * sc / 2.0;
    let y0 = cy - 3.5 * sc;
    for (i, ch) in s.chars().enumerate() {
        for gy in 0..font::GH {
            let ya = (y0 + gy as f32 * sc - grow).floor() as i32;
            let yb = (y0 + (gy + 1) as f32 * sc + grow).floor() as i32;
            let c = col(gy);
            let ox = x0 + wob(gy);
            for gx in 0..6 {
                if font::bit(ch, gx, gy, true) {
                    let cx = (i as i32 * 7 + gx) as f32;
                    let xa = (ox + cx * sc - grow).floor() as i32;
                    let xb = (ox + (cx + 1.0) * sc + grow).floor() as i32;
                    fb.fill_rect(xa, ya, xb - xa, yb - ya, c);
                }
            }
        }
    }
}

impl Effect for TextZoom {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.ph += c.dt * (0.6 + 0.8 * a.mid);
        let hue = c.time * 0.05 + (c.beats / self.every) as f32 * 0.13;
        // background: two dark palette shades in a rotating pattern, dimmer outwards
        let mut tab = [[0u32; 8]; 2];
        let (ca, cb) = (lift(c.pal, hue), lift(c.pal, hue + 0.15));
        for (l, k) in (0..8).map(|l| (l, 1.0 - l as f32 * 0.09)) {
            tab[0][l] = scale(ca, (0.34 + 0.25 * a.pulse) * k);
            tab[1][l] = scale(cb, (0.14 + 0.1 * a.pulse) * k);
        }
        let rot = (self.ph * 40.0) as i32 as u8;
        let roff = (self.ph * 30.0) as i32 as u8;
        let bgmode = self.bgmode;
        for ((p, &an), &rd) in fb.px.iter_mut().zip(&self.polar.ang).zip(&self.polar.rad) {
            let s = match bgmode {
                0 => (an.wrapping_add(rot) >> 4) & 1,
                1 => (rd.wrapping_sub(roff) >> 3) & 1,
                _ => ((an.wrapping_add(rot) >> 4) ^ (rd.wrapping_sub(roff) >> 4)) & 1,
            };
            *p = tab[s as usize][(rd >> 5) as usize];
        }
        let word = self.words[(c.beats / self.every) as usize % self.words.len()];
        let n = word.chars().count() as f32;
        let rest = ((W as f32 - 40.0) / (n * 7.0 - 1.0)).min(13.0);
        let p = 1.0 - a.pulse;
        let sc = match self.mode {
            0 => rest * (1.0 + 1.2 * a.pulse * a.pulse),
            1 => rest * (0.12 + 0.88 * smoothstep(p * 1.6)),
            _ => rest * (0.55 + 0.6 * p),
        };
        let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
        let wob = |gy: i32| (gy as f32 * 0.9 + self.ph * 5.0).sin() * a.mid * sc * 0.35;
        // echoes trailing the zoom, thin and dim so the word stays readable
        for k in (1..=2).rev() {
            let sk = sc * (1.0 + k as f32 * (0.05 + 0.12 * a.pulse));
            let ck = scale(lift(c.pal, hue + 0.3 + k as f32 * 0.1), 0.5 - k as f32 * 0.15);
            big_word(fb, word, cx, cy, sk, 0.0, wob, |_| ck);
        }
        let out = (sc * 0.14).max(1.0);
        big_word(fb, word, cx + sc * 0.3, cy + sc * 0.3, sc, out, wob, |_| rgb(0, 0, 0));
        big_word(fb, word, cx, cy, sc, out, wob, |_| rgb(8, 8, 16));
        let top = mix(c.pal.hi, rgb(255, 255, 255), 0.5 + 0.5 * a.pulse);
        let bot = c.pal.at(hue + 0.55);
        big_word(fb, word, cx, cy, sc, 0.0, wob, |gy| mix(top, bot, gy as f32 / 6.0));
    }
}

// ---------------------------------------------------------------- DOT LOGO

const DOT_WORDS: [&[&str]; 3] = [
    &["UFO", "HEJ", "DANS", "MUSIK", "KUL"],
    &["UFO", "DOTS", "BOOM", "PARTY", "WOW!"],
    &["UFO", "PIXEL", "TAKT", "LJUD", "YES!"],
];

struct Dot {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    tx: f32,
    ty: f32,
    u: f32,
}

/// Dot logo: a word built from dots that bursts apart on the beat and springs
/// back together, often as the next word.
struct DotLogo {
    targets: Vec<Vec<(f32, f32, f32)>>,
    dots: Vec<Dot>,
    mode: u32,
    rng: Rng,
    last: u32,
    free: f32,
    rows: Vec<u32>,
}

impl DotLogo {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let words = DOT_WORDS[rng.below(3) as usize];
        let mode = rng.below(3);
        let targets: Vec<Vec<(f32, f32, f32)>> = words
            .iter()
            .map(|w| {
                let n = w.chars().count() as f32;
                let s = (330.0 / (n * 6.0 - 1.0)).min(14.0);
                let (w0, h0) = ((n * 6.0 - 1.0) * s, 7.0 * s);
                let mut t = Vec::new();
                for (gx, gy) in word_pixels(w, false) {
                    for sy in 0..2 {
                        for sx in 0..2 {
                            let x = W as f32 / 2.0 - w0 / 2.0 + (gx as f32 + 0.25 + sx as f32 * 0.5) * s;
                            let y = H as f32 / 2.0 - h0 / 2.0 + (gy as f32 + 0.25 + sy as f32 * 0.5) * s;
                            t.push((x, y, (x - W as f32 / 2.0 + w0 / 2.0) / w0));
                        }
                    }
                }
                t
            })
            .collect();
        let n = targets.iter().map(|t| t.len()).max().unwrap_or(1);
        let dots = (0..n)
            .map(|i| {
                let (tx, ty, u) = targets[0][i % targets[0].len()];
                Dot { x: rng.range(0.0, W as f32), y: rng.range(0.0, H as f32), vx: 0.0, vy: 0.0, tx, ty, u }
            })
            .collect();
        DotLogo { targets, dots, mode, rng, last: 0, free: 0.0, rows: vec![0; H] }
    }
}

impl Effect for DotLogo {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
        if c.beats != self.last {
            self.last = c.beats;
            let big = c.beats % 4 == 0;
            if big {
                // explode, and come back as the next word
                let t = &self.targets[(c.beats / 4) as usize % self.targets.len()];
                for (i, d) in self.dots.iter_mut().enumerate() {
                    (d.tx, d.ty, d.u) = t[i % t.len()];
                }
                self.free = 0.35;
            }
            let force = if big { 1.0 + a.bass } else { 0.18 };
            for d in self.dots.iter_mut() {
                let (dx, dy) = (d.x - cx, d.y - cy);
                let l = dx.hypot(dy).max(1.0);
                let sp = self.rng.range(150.0, 420.0) * force;
                let (vx, vy) = match self.mode {
                    0 => (dx / l * sp + self.rng.range(-60.0, 60.0) * force, dy / l * sp + self.rng.range(-60.0, 60.0) * force),
                    1 => ((-dy / l + dx / l * 0.3) * sp, (dx / l + dy / l * 0.3) * sp),
                    _ => (self.rng.range(-0.5, 0.5) * sp, -self.rng.range(0.4, 1.0) * sp),
                };
                d.vx += vx;
                d.vy += vy;
            }
        }
        let dt = c.dt;
        let free = self.free > 0.0;
        self.free -= dt;
        for d in self.dots.iter_mut() {
            let (ax, ay) = if free {
                let g = if self.mode == 2 { 700.0 } else { 0.0 };
                (-d.vx * 1.5, g - d.vy * 1.5)
            } else {
                ((d.tx - d.x) * 95.0 - d.vx * 11.0, (d.ty - d.y) * 95.0 - d.vy * 11.0)
            };
            d.vx += ax * dt;
            d.vy += ay * dt;
            d.x += d.vx * dt;
            d.y += d.vy * dt;
        }
        // background: stepped dark gradient with thin copper lines top and bottom
        for y in 0..H {
            let band = (y as i32 - H as i32 / 2).unsigned_abs() as usize / 14;
            self.rows[y] = scale(c.pal.at(0.6 + c.time * 0.02), (0.34 - band as f32 * 0.04).max(0.12));
        }
        let line = add(c.pal.hi, scale(rgb(255, 255, 255), a.pulse * 0.4));
        for y in [14, 15, H - 16, H - 15] {
            self.rows[y] = line;
        }
        fill_rows(fb, &self.rows);
        let hue = c.time * 0.08;
        for d in &self.dots {
            let sp = (d.vx.hypot(d.vy) / 500.0).min(1.0);
            let col = mix(lift(c.pal, hue + d.u * 0.6), rgb(255, 255, 255), sp * 0.7 + a.pulse * 0.2);
            let (x, y) = (d.x as i32, d.y as i32);
            let (edge, core) = (scale(col, 0.55), mix(col, rgb(255, 255, 255), 0.55));
            for (dx, dy, k) in [(-1, -1, edge), (0, -1, col), (1, -1, edge), (-1, 0, col), (0, 0, core), (1, 0, col), (-1, 1, edge), (0, 1, col), (1, 1, edge)] {
                fb.put(x + dx, y + dy, k);
            }
        }
    }
}

// ---------------------------------------------------------------- VERTICAL BARS

/// Vertical bars: copper bars standing upright, wrapped round a turning
/// cylinder or weaving, optionally waving per scanline, between two metal lids.
struct VerticalBars {
    mode: u32,
    n: usize,
    wave: bool,
    ph: f32,
    freq: Vec<f32>,
    line: Vec<u32>,
    order: Vec<(f32, f32, f32)>,
}

const VPAD: usize = 48;
const VTOP: usize = 40;
const VBOT: usize = 180;

impl VerticalBars {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let mode = r.below(3);
        let n = [12, 9, 8][mode as usize];
        VerticalBars {
            mode,
            n,
            wave: r.below(2) == 0,
            ph: r.range(0.0, 10.0),
            freq: (0..n).map(|_| r.range(0.6, 1.3)).collect(),
            line: vec![0; W + 2 * VPAD],
            order: Vec::with_capacity(16),
        }
    }
}

impl Effect for VerticalBars {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.ph += c.dt * (0.7 + 1.0 * a.mid);
        let ph = self.ph;
        let hue = c.time * 0.03;
        let lw = self.line.len();
        let cx = lw as f32 / 2.0;
        for (x, p) in self.line.iter_mut().enumerate() {
            let d = ((x as f32 - cx).abs() / 40.0) as i32;
            *p = scale(c.pal.at(hue + 0.5), (0.22 - d as f32 * 0.035).max(0.06));
        }
        self.order.clear();
        for i in 0..self.n {
            let fi = i as f32;
            let (x, z) = match self.mode {
                1 => {
                    let p = ph * self.freq[i] + fi * 1.3;
                    (cx + 150.0 * p.sin(), p.cos())
                }
                _ => {
                    let p = ph + fi * TAU / self.n as f32;
                    (cx + (115.0 + a.bass * 30.0) * p.sin(), p.cos())
                }
            };
            self.order.push((z, x, hue + fi / self.n as f32 * 0.8));
        }
        self.order.sort_by(|p, q| p.0.total_cmp(&q.0));
        for &(z, x, h) in &self.order {
            let depth = (z + 1.0) * 0.5;
            let half = 5.0 + 4.0 * depth + a.bass * 3.0;
            let col = add(scale(c.pal.at(h), 0.3 + 0.7 * depth), scale(rgb(255, 255, 255), a.pulse * 0.35 * depth));
            for xx in ((x - half) as i32).max(0)..((x + half) as i32 + 1).min(lw as i32) {
                let d = (xx as f32 + 0.5 - x) / half;
                if d.abs() < 1.0 {
                    self.line[xx as usize] = bar_shade(col, d, 0.3 + 0.6 * depth);
                }
            }
        }
        if self.mode == 2 {
            // kaleidoscope: the right half mirrored onto the left
            let m = lw / 2;
            for d in 0..m {
                self.line[m - 1 - d] = self.line[m + d];
            }
        }
        let shadow = c.pal.bg;
        for y in VTOP..VBOT {
            let fy = y as f32;
            let off = if self.wave {
                ((fy * 0.04 + ph * 2.2).sin() * (4.0 + a.mid * 14.0) + (fy * 0.013 - ph).sin() * 6.0) as i32
            } else {
                0
            };
            let s = (VPAD as i32 + off).clamp(0, 2 * VPAD as i32) as usize;
            let row = &mut fb.px[y * W..(y + 1) * W];
            row.copy_from_slice(&self.line[s..s + W]);
            // soft shadow under the lids
            let edge = (y - VTOP).min(VBOT - 1 - y);
            if edge < 6 {
                for p in row.iter_mut() {
                    *p = if edge < 3 { half_mix(half_mix(*p, shadow), shadow) } else { half_mix(*p, shadow) };
                }
            }
        }
        // lids: calm stepped gradients with a chrome rim bar
        let lid = c.pal.at(0.62);
        for y in 0..VTOP {
            fb.row(y as i32, scale(lid, 0.15 + (y / 6) as f32 * 0.03));
        }
        for y in VBOT..H {
            fb.row(y as i32, scale(lid, (0.33 - ((y - VBOT) / 6) as f32 * 0.03).max(0.1)));
        }
        let rimc = add(c.pal.hi, scale(rgb(255, 255, 255), a.pulse * 0.3));
        fb.copper_bar(VTOP as f32 - 3.0, 4.0, rimc);
        fb.copper_bar(VBOT as f32 + 3.0, 4.0, rimc);
    }
}

// ---------------------------------------------------------------- CRAWL

const CRAWL_TEXT: [&[&str]; 3] = [
    &[
        "UFO PRESENTS", "", "IN A COLD PARTY HALL", "FULL OF HUMMING SCREENS,", "A SMALL CREW OF CODERS",
        "SWORE TO PUSH EVERY", "PIXEL TO THE BEAT.", "", "NO SLEEP. NO MERCY.", "ONLY COPPER BARS,",
        "SINE WAVES AND", "FAR TOO MUCH COFFEE.", "", "TURN IT UP!", "", "", "",
    ],
    &[
        "UFO PRESENTERAR", "", "EN KALL VINTERNATT", "I EN FULLSATT HALL", "SATT NÅGRA NÖRDAR", "OCH FICK PIXLAR",
        "ATT DANSA TILL MUSIKEN.", "", "INGEN SÖMN, BARA", "KAFFE, KOD OCH", "KOPPARSTAPLAR.", "", "HÖJ VOLYMEN!",
        "", "", "",
    ],
    &[
        "GREETINGS FLY OUT TO", "", "EVERYONE WHO EVER", "CARRIED A COMPUTER", "THROUGH THE SNOW", "TO A PARTY",
        "AND STAYED UP", "ALL NIGHT", "TO MAKE IT MOVE.", "", "HEJ HEJ FRÅN UFO!", "", "SEE YOU", "AT THE NEXT ONE.",
        "", "", "",
    ],
];

/// Crawl: lines of text on a tilted plane, receding towards the horizon over
/// a twinkling starfield.
struct Crawl {
    mask: Vec<u8>,
    mw: usize,
    mh: usize,
    line_h: usize,
    scroll: f32,
    hy: i32,
    stars: Vec<(u16, u16, u8)>,
}

impl Crawl {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let lines = CRAWL_TEXT[r.below(3) as usize];
        let sc = 2usize;
        let line_h = 11 * sc;
        let maxc = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1);
        let mw = maxc * 7 * sc + 8;
        let mh = lines.len() * line_h;
        let mut bits = vec![0u8; mw * mh];
        for (li, l) in lines.iter().enumerate() {
            let w = (l.chars().count() * 7) as i32 * sc as i32;
            let ox = (mw as i32 - w) / 2;
            for (gx, gy) in word_pixels(l, true) {
                for yy in 0..sc {
                    for xx in 0..sc {
                        let x = (ox + gx * sc as i32) as usize + xx;
                        let y = li * line_h + gy as usize * sc + yy + 2;
                        bits[y * mw + x] = 1;
                    }
                }
            }
        }
        // soften edges with a 3×3 box so distant text does not sparkle
        let mut mask = vec![0u8; mw * mh];
        for y in 1..mh - 1 {
            for x in 1..mw - 1 {
                let mut s = 0u32;
                for dy in 0..3 {
                    for dx in 0..3 {
                        s += bits[(y + dy - 1) * mw + x + dx - 1] as u32;
                    }
                }
                mask[y * mw + x] = (s * 255 / 9) as u8;
            }
        }
        let stars = (0..260).map(|_| (r.below(W as u32) as u16, r.below(H as u32) as u16, r.below(256) as u8)).collect();
        Crawl { mask, mw, mh, line_h, scroll: 0.0, hy: 26 + r.below(3) as i32 * 8, stars }
    }
}

impl Effect for Crawl {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.scroll += c.dt * (20.0 + 28.0 * a.bass);
        let hue = c.time * 0.02;
        fb.clear(rgb(0, 0, 0));
        // horizon glow in a few bands, flashing on the beat
        let glow = add(scale(c.pal.at(hue + 0.55), 0.5), scale(c.pal.hi, a.pulse * 0.5));
        for d in 0..14 {
            let k = 1.0 - d as f32 / 14.0;
            let col = scale(glow, k * k * 0.8);
            fb.row(self.hy - d, col);
            fb.row(self.hy + d / 3, col);
        }
        for (i, &(x, y, b)) in self.stars.iter().enumerate() {
            let tw = (c.time * (2.0 + (i % 5) as f32) + i as f32).sin() * 0.5 + 0.5;
            let k = b as f32 / 255.0 * (0.4 + 0.6 * tw) * (0.6 + 0.6 * a.treble);
            if k > 0.12 {
                fb.put(x as i32, y as i32, scale(rgb(220, 230, 255), k.min(1.0)));
            }
        }
        // the text plane
        let total = (self.mh + H * 2) as f32;
        let span = (H as i32 - self.hy) as f32;
        let us = self.mw as f32 / 330.0;
        let vs = 46.0;
        for y in self.hy + 1..HI {
            let d = (y - self.hy) as f32;
            let k = span / d;
            let fade = (d / span * 2.2).min(1.0);
            if fade < 0.08 {
                continue;
            }
            let v = (self.scroll - (k - 1.0) * vs).rem_euclid(total);
            if v >= self.mh as f32 {
                continue;
            }
            let vi = v as usize;
            let line = vi / self.line_h;
            let col = add(c.pal.at(hue + line as f32 * 0.06), scale(c.pal.hi, a.pulse * 0.4));
            let col = mix(col, rgb(255, 255, 240), 0.25);
            let full = scale(col, (fade * 4.0).ceil() / 4.0);
            let part = scale(full, 0.5);
            let du = k * us;
            let mut u = self.mw as f32 / 2.0 - W as f32 / 2.0 * du;
            let mrow = &self.mask[vi * self.mw..(vi + 1) * self.mw];
            let row = &mut fb.px[y as usize * W..(y as usize + 1) * W];
            for p in row.iter_mut() {
                if u >= 0.0 && (u as usize) < self.mw {
                    let m = mrow[u as usize];
                    if m > 150 {
                        *p = full;
                    } else if m > 60 {
                        *p = part;
                    }
                }
                u += du;
            }
        }
    }
}

// ---------------------------------------------------------------- NIGHT CITY

/// Night city: an 8-bit style skyline scrolling sideways at two speeds, its
/// windows lighting up with the music, a road, street lamps and a small saucer.
struct NightCity {
    idx: Vec<u8>,
    pos: f32,
    eq: bool,
    dir: f32,
    moon_x: i32,
    salt: u32,
    lvl: [f32; 8],
    tmp: [f32; 8],
}

const GY: usize = 90;

const SAUCER: [&str; 6] = [".....ddd.....", "....ddddd....", "..hhhhhhhhh..", "hhLhhLhhLhhLh", "..uuuuuuuuu..", "....u...u...."];

impl NightCity {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        NightCity {
            idx: vec![0; LW * LH],
            pos: r.range(0.0, 5000.0),
            eq: r.below(2) == 0,
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
            moon_x: 30 + r.below(130) as i32,
            salt: r.u32(),
            lvl: [0.0; 8],
            tmp: [0.0; 8],
        }
    }

    fn set(&mut self, x: i32, y: i32, v: u8) {
        if x >= 0 && y >= 0 && (x as usize) < LW && (y as usize) < LH {
            self.idx[y as usize * LW + x as usize] = v;
        }
    }
}

impl Effect for NightCity {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.pos += c.dt * (12.0 + 22.0 * a.bass) * self.dir;
        spectrum(c, &mut self.tmp);
        for b in 0..8 {
            self.lvl[b] = self.tmp[b].max(self.lvl[b] - c.dt * 1.2);
        }
        let flash = a.pulse > 0.5;
        let cols: [u32; 16] = [
            rgb(8, 8, 36),
            rgb(24, 16, 72),
            rgb(64, 28, 100),
            rgb(240, 236, 200),
            rgb(255, 255, 255),
            rgb(44, 36, 88),
            rgb(16, 12, 36),
            q8(mix(c.pal.at(0.1), rgb(255, 220, 120), 0.3)),
            q8(mix(c.pal.at(0.15), rgb(255, 255, 160), 0.2 + 0.4 * a.pulse)),
            q8(mix(c.pal.hi, rgb(255, 255, 255), 0.3)),
            rgb(36, 36, 72),
            if flash { rgb(255, 255, 255) } else { q8(c.pal.at(0.6)) },
            rgb(100, 100, 120),
            rgb(28, 24, 44),
            rgb(255, 48, 48),
            rgb(176, 180, 204),
        ];
        // sky: three dithered bands
        for y in 0..GY {
            let v = y as f32 / GY as f32 * 2.0;
            let base = v as u8;
            let fr = v - base as f32;
            for x in 0..LW {
                self.idx[y * LW + x] = base + dith(fr, x, y) as u8;
            }
        }
        for y in GY..LH {
            self.idx[y * LW..(y + 1) * LW].fill(13);
        }
        // stars and moon
        let tick = (c.time * 6.0) as u32;
        for i in 0..48u32 {
            let h = h2(i, self.salt);
            let (x, y) = ((h % LW as u32) as i32, ((h >> 10) % 56) as i32);
            if h2(i, tick) % 6 != 0 || a.treble > 0.8 {
                self.set(x, y, 4);
            }
        }
        for dy in -7i32..=7 {
            for dx in -7i32..=7 {
                if dx * dx + dy * dy <= 49 {
                    let crater = (dx - 2) * (dx - 2) + (dy + 1) * (dy + 1) <= 3 || (dx + 3) * (dx + 3) + (dy - 3) * (dy - 3) <= 1;
                    self.set(self.moon_x + dx, 17 + dy, if crater { 12 } else { 3 });
                }
            }
        }
        // far skyline
        let off = (self.pos * 0.3).floor() as i32;
        for x in 0..LW as i32 {
            let wx = x + off;
            let (cell, lx) = (wx.div_euclid(9), wx.rem_euclid(9));
            let hh = h2(cell as u32, self.salt);
            if lx == 8 && hh & 1 == 0 {
                continue;
            }
            let top = GY as i32 - 16 - ((hh >> 4) % 36) as i32;
            for y in top..GY as i32 {
                let win = lx % 2 == 1 && (y - top) % 3 == 2 && y < GY as i32 - 2;
                let lit = win && h3(cell as u32, lx as u32, y as u32) % 100 < 12 + (self.lvl[cell.rem_euclid(8) as usize] * 40.0) as u32;
                self.set(x, y, if lit { 7 } else { 5 });
            }
        }
        // near skyline: lit windows follow the spectrum, one band per building
        let off = (self.pos * 0.65).floor() as i32;
        for x in 0..LW as i32 {
            let wx = x + off;
            let (cell, lx) = (wx.div_euclid(30), wx.rem_euclid(30));
            let hh = h2(cell as u32, self.salt ^ 0x55);
            let bw = 18 + (hh % 9) as i32;
            if lx >= bw {
                continue;
            }
            let h = 28 + ((hh >> 5) % 44) as i32;
            let top = GY as i32 - h;
            let band = cell.rem_euclid(8) as usize;
            let level = self.lvl[band];
            let nrow = (h - 6) / 4;
            for y in top..GY as i32 {
                let ly = y - top;
                let mut v = if ly == 0 || lx == 0 { 5 } else { 6 };
                let (wxk, wyk) = (lx - 2, ly - 3);
                if wxk >= 0 && wyk >= 0 && lx < bw - 2 && wxk % 4 < 2 && wyk % 4 < 2 && y < GY as i32 - 3 {
                    let (i, j) = ((wxk / 4) as u32, (wyk / 4) as u32);
                    let wh = h3(cell as u32, i, j);
                    let mut on = if self.eq {
                        (nrow - j as i32) as f32 / nrow.max(1) as f32 <= level * 1.15 + 0.08
                    } else {
                        (wh % 256) as f32 <= 40.0 + level * 190.0
                    };
                    if h3(wh, c.beats, 7) % 13 == 0 {
                        on = !on;
                    }
                    v = if on { if wh & 16 != 0 { 8 } else { 9 } } else { 10 };
                }
                if hh % 4 == 1 && (1..3).contains(&lx) && (5..17).contains(&ly) {
                    v = if ly % 3 == 0 { 6 } else { 11 };
                }
                self.set(x, y, v);
            }
            if hh % 3 == 0 && lx == bw / 2 {
                for y in top - 6..top {
                    self.set(x, y, 12);
                }
                let blink = ((c.time * 2.0) as i32 + cell) % 2 == 0;
                self.set(x, top - 7, if blink { 14 } else { 6 });
            }
        }
        // road, kerb, dashes and street lamps
        for x in 0..LW as i32 {
            self.set(x, GY as i32, 12);
            if (x + self.pos.floor() as i32).rem_euclid(14) < 7 {
                self.set(x, GY as i32 + 9, 7);
            }
        }
        let loff = (self.pos * 1.0).floor() as i32;
        for k in 0..6 {
            let x = k * 40 - loff.rem_euclid(40);
            for y in GY as i32 - 13..GY as i32 {
                self.set(x, y, 12);
            }
            for dx in -1..3 {
                self.set(x + dx, GY as i32 - 14, 9);
            }
            for dy in 1..6 {
                for dx in -dy / 2 - 1..dy / 2 + 2 {
                    if (x + dx + dy) % 2 == 0 {
                        self.set(x + dx, GY as i32 + dy, 7);
                    }
                }
            }
        }
        // the saucer drifts across, beaming down on every fourth beat
        let span = (LW + 40) as f32;
        let t = fract(c.time / 11.0) * span - 20.0;
        let sx = if self.dir > 0.0 { t } else { LW as f32 - t } as i32;
        let sy = 26 + ((c.time * 1.8).sin() * 4.0 + a.bass * 3.0) as i32;
        if c.beats % 4 == 0 && a.pulse > 0.2 {
            for y in sy + 6..GY as i32 {
                let w = 2 + (y - sy - 6) / 3;
                for dx in -w..=w {
                    if (dx + y) % 2 == 0 {
                        self.set(sx + 6 + dx, y, 9);
                    }
                }
            }
        }
        for (ry, row) in SAUCER.iter().enumerate() {
            for (rx, ch) in row.bytes().enumerate() {
                let v = match ch {
                    b'd' => 9,
                    b'h' => 15,
                    b'u' => 12,
                    b'L' => {
                        if (rx as u32 / 3 + c.beats) % 4 == 0 { 4 } else if (rx / 3) % 2 == 0 { 14 } else { 8 }
                    }
                    _ => continue,
                };
                self.set(sx + rx as i32, sy + ry as i32, v);
            }
        }
        blit2x(fb, &self.idx, &cols);
    }
}

// ---------------------------------------------------------------- SNOWFALL

const SKY: u8 = 255;

/// Snowfall: an 8-bit Swedish winter village — red cottages with snowy roofs,
/// pines, northern lights over the hills — windows glowing with the beat and
/// snow falling faster with the treble.
struct Snowfall {
    base: Vec<u8>,
    idx: Vec<u8>,
    flakes: Vec<[f32; 4]>,
    chimneys: Vec<(i32, i32)>,
    stars: Vec<(i32, i32)>,
    aurora: usize,
    wind: f32,
    aur: [(f32, f32, f32); LW],
}

impl Snowfall {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let mut base = vec![SKY; LW * LH];
        let put = |b: &mut Vec<u8>, x: i32, y: i32, v: u8| {
            if x >= 0 && y >= 0 && (x as usize) < LW && (y as usize) < LH {
                b[y as usize * LW + x as usize] = v;
            }
        };
        // far hills: snow with blue shadows, a forest line on the ridge
        let p1 = r.range(0.0, TAU);
        let far: Vec<i32> = (0..LW).map(|x| (66.0 + 5.0 * (x as f32 * 0.04 + p1).sin() + 3.0 * (x as f32 * 0.11).sin()) as i32).collect();
        for x in 0..LW {
            for y in far[x]..LH as i32 {
                let v = if dith(((y - far[x]) as f32 / 10.0).min(1.0) * 0.6, x, y as usize) { 8 } else { 9 };
                put(&mut base, x as i32, y, v);
            }
        }
        let pine = |b: &mut Vec<u8>, x: i32, foot: i32, h: i32| {
            for k in 0..h {
                let y = foot - 1 - k;
                let tier = (h - k) % (h / 3).max(2);
                let half = ((h - k) as f32 * 0.32) as i32 - tier / 2;
                for dx in -half..=half {
                    put(b, x + dx, y, if tier == 0 && dx < 0 { 7 } else { 10 });
                }
                if tier <= 1 && half > 0 {
                    put(b, x - half, y, 7);
                }
            }
            put(b, x, foot - h - 1, 7);
            put(b, x, foot, 12);
        };
        let mut x = r.range(0.0, 6.0) as i32;
        while x < LW as i32 {
            let h = 5 + r.below(6) as i32;
            pine(&mut base, x, far[x as usize] + 2, h);
            x += 3 + r.below(6) as i32;
        }
        // near ground
        let p2 = r.range(0.0, TAU);
        let near: Vec<i32> = (0..LW).map(|x| (86.0 + 3.0 * (x as f32 * 0.05 + p2).sin()) as i32).collect();
        for x in 0..LW {
            for y in near[x]..LH as i32 {
                let v = if (y - near[x]) % 7 == 3 && dith(0.4, x, y as usize) { 8 } else { 7 };
                put(&mut base, x as i32, y, v);
            }
        }
        // cottages: falu red walls, white corners, snowy roofs, lit windows
        let mut chimneys = Vec::new();
        let mut spans = Vec::new();
        let count = 3 + r.below(2) as i32;
        let slot = LW as i32 / count;
        for k in 0..count {
            let w = 18 + r.below(9) as i32;
            let wh = 9 + r.below(4) as i32;
            let x0 = k * slot + r.below((slot - w - 4).max(1) as u32) as i32 + 2;
            let foot = near[(x0 + w / 2).clamp(0, LW as i32 - 1) as usize] + 3 + r.below(4) as i32;
            let top = foot - wh;
            spans.push((x0 - 3, x0 + w + 3));
            for y in top..foot {
                for x in x0..x0 + w {
                    let v = if x == x0 || x == x0 + w - 1 { 7 } else { 11 };
                    put(&mut base, x, y, v);
                }
            }
            // windows with white frames and a cross
            let nwin = (w - 4) / 7;
            for i in 0..nwin {
                let wx = x0 + 3 + i * 7 + if i * 2 >= nwin { 2 } else { 0 };
                for dy in 0..5 {
                    for dx in 0..5 {
                        let frame = dx == 0 || dy == 0 || dx == 4 || dy == 4 || dx == 2 || dy == 2;
                        put(&mut base, wx + dx, top + 3 + dy, if frame { 7 } else { 13 });
                    }
                }
            }
            let dx0 = x0 + w / 2 - 1;
            for y in foot - 6..foot {
                for x in dx0..dx0 + 3 {
                    put(&mut base, x, y, if y == foot - 6 { 7 } else { 12 });
                }
            }
            // roof with snow on top
            let rh = 6 + w / 8;
            for j in 0..rh {
                let y = top - 1 - j;
                let half = w / 2 + 2 - (j * (w / 2 + 2)) / rh;
                let cxr = x0 + w / 2;
                for x in cxr - half..cxr + half {
                    let snow = j > 1 || dith(0.5, x as usize, y as usize);
                    put(&mut base, x, y, if snow { 7 } else { 12 });
                }
                put(&mut base, cxr - half, y, 8);
            }
            let chx = x0 + w / 4;
            let chy = top - rh / 2 - 3;
            for y in chy..chy + 4 {
                put(&mut base, chx, y, 12);
                put(&mut base, chx + 1, y, 12);
            }
            put(&mut base, chx, chy, 7);
            put(&mut base, chx + 1, chy, 7);
            chimneys.push((chx, chy - 1));
            // a pine beside some cottages
            if r.below(2) == 0 {
                let px = if k % 2 == 0 { x0 + w + 4 } else { x0 - 5 };
                pine(&mut base, px, foot + 1, 18 + r.below(8) as i32);
                spans.push((px - 6, px + 6));
            }
        }
        // a snowman in the foreground, in a gap between the cottages
        let mut sx = 8;
        for _ in 0..40 {
            sx = r.below(LW as u32 - 16) as i32 + 8;
            if spans.iter().all(|&(a, b)| sx + 4 < a || sx - 4 > b) {
                break;
            }
        }
        let sy = LH as i32 - 7;
        for (cy, rr) in [(sy - 4, 4), (sy - 11, 3), (sy - 17, 2)] {
            for dy in -rr - 1..=rr + 1 {
                for dx in -rr - 1..=rr + 1 {
                    let d2 = dx * dx + dy * dy;
                    if d2 <= rr * rr {
                        put(&mut base, sx + dx, cy + dy, if dx + dy > rr / 2 { 8 } else { 7 });
                    } else if d2 <= (rr + 1) * (rr + 1) {
                        put(&mut base, sx + dx, cy + dy, 9);
                    }
                }
            }
        }
        for (x, y, v) in [(-1, -18, 12), (1, -18, 12), (1, -17, 14), (2, -17, 14), (0, -12, 12), (0, -10, 12)] {
            put(&mut base, sx + x, sy + y, v);
        }
        for k in 0..4 {
            put(&mut base, sx - 4 - k, sy - 12 - k / 2, 12);
            put(&mut base, sx + 4 + k, sy - 12 - k / 2, 12);
        }
        // moon
        if r.below(2) == 0 {
            let (mx, my) = (20 + r.below(150) as i32, 12);
            for dy in -5i32..=5 {
                for dx in -5i32..=5 {
                    if dx * dx + dy * dy <= 25 {
                        put(&mut base, mx + dx, my + dy, if (dx + 2) * (dx + 2) + dy * dy <= 16 { 6 } else { 8 });
                    }
                }
            }
        }
        let stars = (0..60).map(|_| (r.below(LW as u32) as i32, r.below(60) as i32)).collect();
        let flakes = (0..240).map(|i| [r.range(0.0, LW as f32), r.range(0.0, LH as f32), (i % 3) as f32, r.range(0.0, TAU)]).collect();
        Snowfall {
            base,
            idx: vec![0; LW * LH],
            flakes,
            chimneys,
            stars,
            aurora: r.below(3) as usize,
            wind: r.range(-6.0, 6.0),
            aur: [(0.0, 0.0, 0.0); LW],
        }
    }
}

impl Effect for Snowfall {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let t = c.time;
        let glow = (0.35 + 0.65 * a.pulse.max(a.bass * 0.7)).min(1.0);
        // the aurora keeps its own colours: classic green, cyan-green or a rare red one
        let (aur_lo, aur_hi, aur_top) = [
            (rgb(24, 128, 80), rgb(96, 255, 150), rgb(160, 64, 176)),
            (rgb(20, 110, 120), rgb(120, 255, 210), rgb(60, 200, 90)),
            (rgb(30, 130, 70), rgb(130, 255, 120), rgb(210, 50, 90)),
        ][self.aurora];
        let cols: [u32; 16] = [
            rgb(4, 8, 32),
            rgb(12, 20, 60),
            rgb(32, 40, 92),
            aur_lo,
            add(aur_hi, scale(rgb(255, 255, 255), a.pulse * 0.2)),
            aur_top,
            rgb(255, 255, 255),
            rgb(232, 238, 255),
            rgb(146, 164, 220),
            rgb(70, 80, 136),
            rgb(16, 60, 42),
            rgb(164, 32, 28),
            rgb(44, 32, 40),
            q8(mix(rgb(150, 70, 20), rgb(255, 230, 90), glow)),
            rgb(255, 120, 40),
            rgb(130, 130, 150),
        ];
        // aurora curtains, one column at a time
        let strength = 0.7 + 0.4 * a.bass + 0.3 * a.pulse;
        for (x, au) in self.aur.iter_mut().enumerate() {
            let fx = x as f32;
            let base = 32.0 + 8.0 * (fx * 0.045 + t * 0.25).sin() + 4.0 * (fx * 0.12 - t * 0.4).sin();
            let h = (14.0 + 10.0 * (fx * 0.07 + t * 0.3).sin()) * strength;
            let i = (0.55 + 0.45 * (fx * 0.21 + t * 0.9).sin() * (fx * 0.05 - t * 0.2).sin()) * strength;
            *au = (base, h.max(2.0), i);
        }
        let tick = (t * 5.0) as u32;
        for y in 0..LH {
            for x in 0..LW {
                let i = y * LW + x;
                let b = self.base[i];
                self.idx[i] = if b != SKY {
                    b
                } else {
                    let (base, h, inten) = self.aur[x];
                    let dy = base - y as f32;
                    let v = if dy >= 0.0 { inten * (1.0 - dy / h).max(0.0) } else { inten * (1.0 + dy / 3.0).max(0.0) };
                    let lv = (v * 2.0 + BAYER[y & 3][x & 3] as f32 / 16.0) as u8;
                    if lv >= 1 && dy > h * 0.6 {
                        5
                    } else if lv >= 2 {
                        4
                    } else if lv == 1 {
                        3
                    } else {
                        let g = (y as f32 / 70.0).min(1.0) * 2.0;
                        let gb = g as u8;
                        gb.min(2) + (gb < 2 && dith(g - gb as f32, x, y)) as u8
                    }
                };
            }
        }
        for (k, &(x, y)) in self.stars.iter().enumerate() {
            let i = y as usize * LW + x as usize;
            if self.idx[i] <= 2 && h2(k as u32, tick) % 5 != 0 {
                self.idx[i] = 6;
            }
        }
        // chimney smoke
        for &(cx, cy) in &self.chimneys {
            for p in 0..8 {
                let age = fract(t * 0.3 + p as f32 / 8.0);
                let x = cx as f32 + age * (8.0 + self.wind) + (age * 9.0 + p as f32).sin() * 1.5;
                let y = cy as f32 - age * 22.0;
                if age < 0.85 && (x as usize) < LW && y >= 0.0 && dith(1.0 - age, x as usize, y as usize) {
                    self.idx[y as usize * LW + x as usize] = 15;
                    if age > 0.3 && (x as usize + 1) < LW {
                        self.idx[y as usize * LW + x as usize + 1] = 15;
                    }
                }
            }
        }
        // snow: three layers, faster with the treble, swaying in the wind
        let fall = 0.6 + 1.8 * a.treble;
        for f in self.flakes.iter_mut() {
            let layer = f[2];
            f[1] += c.dt * (5.0 + 7.0 * layer) * fall;
            f[0] += c.dt * (self.wind * (0.5 + layer * 0.4) + (t * 1.3 + f[3]).sin() * 4.0);
            if f[1] >= LH as f32 {
                f[1] -= LH as f32;
            }
            f[0] = f[0].rem_euclid(LW as f32);
            let i = (f[1] as usize).min(LH - 1) * LW + (f[0] as usize).min(LW - 1);
            self.idx[i] = if layer >= 2.0 { 6 } else if layer >= 1.0 { 7 } else { 8 };
        }
        blit2x(fb, &self.idx, &cols);
    }
}
