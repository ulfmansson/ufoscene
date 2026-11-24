//! Effects: PIXEL theme — per-pixel 2D fields. Plasma, fire, metaballs, moiré,
//! water ripples, Julia and Mandelbrot fractals, kaleidoscope, hypno spirals,
//! Game of Life, glyph rain, Worley/Voronoi cells and domain-warped lava.
//!
//! Heavy maths runs on lookup tables built in `make`, on per-frame colour LUTs
//! (256 entries, so palette work is never per pixel) or at 2×2 "chunky" resolution.

use std::f32::consts::{PI, TAU};

use super::{Ctx, Def, Effect, LOGO_TOP, OWN_TEXT, PIXEL, Palette, Rng, SCROLL_BOTTOM, fract, xor_texture};
use crate::font;
use crate::gfx::*;

pub fn register(v: &mut Vec<Def>) {
    v.push(Def { name: "PLASMA", flags: PIXEL | LOGO_TOP | SCROLL_BOTTOM, make: |s| Box::new(Plasma::new(s)) });
    v.push(Def { name: "FIRE", flags: PIXEL | LOGO_TOP, make: |s| Box::new(Fire::new(s)) });
    v.push(Def { name: "METABALLS", flags: PIXEL, make: |s| Box::new(Metaballs::new(s)) });
    v.push(Def { name: "MOIRE", flags: PIXEL, make: |s| Box::new(Moire::new(s)) });
    v.push(Def { name: "RIPPLES", flags: PIXEL, make: |s| Box::new(Ripples::new(s)) });
    v.push(Def { name: "JULIA", flags: PIXEL, make: |s| Box::new(Julia::new(s)) });
    v.push(Def { name: "MANDELBROT", flags: PIXEL, make: |s| Box::new(Mandelbrot::new(s)) });
    v.push(Def { name: "KALEIDOSCOPE", flags: PIXEL, make: |s| Box::new(Kaleidoscope::new(s)) });
    v.push(Def { name: "HYPNO", flags: PIXEL, make: |s| Box::new(Hypno::new(s)) });
    v.push(Def { name: "LIFE", flags: PIXEL, make: |s| Box::new(Life::new(s)) });
    v.push(Def { name: "GLYPH RAIN", flags: PIXEL | OWN_TEXT, make: |s| Box::new(GlyphRain::new(s)) });
    v.push(Def { name: "VORONOI", flags: PIXEL, make: |s| Box::new(Voronoi::new(s)) });
    v.push(Def { name: "LAVA", flags: PIXEL | LOGO_TOP | SCROLL_BOTTOM, make: |s| Box::new(Lava::new(s)) });
}

// ---------------------------------------------------------------- shared helpers

const WF: f32 = W as f32;
const HF: f32 = H as f32;

/// Fast sine (parabola fit, error ~0.001) for any input.
#[inline]
fn fsin(x: f32) -> f32 {
    let x = x - TAU * (x * (1.0 / TAU) + 0.5).floor();
    let y = (4.0 / PI) * x - (4.0 / (PI * PI)) * x * x.abs();
    0.225 * (y * y.abs() - y) + y
}

/// Multiply a colour by `k / 256` (k above 256 brightens), integer maths.
#[inline]
fn shade(c: u32, k: u32) -> u32 {
    let r = (((c & 0xFF) * k) >> 8).min(255);
    let g = ((((c >> 8) & 0xFF) * k) >> 8).min(255);
    let b = ((((c >> 16) & 0xFF) * k) >> 8).min(255);
    0xFF00_0000 | (b << 16) | (g << 8) | r
}

/// Saturating per-channel add.
#[inline]
fn addc(a: u32, b: u32) -> u32 {
    let r = ((a & 0xFF) + (b & 0xFF)).min(255);
    let g = (((a >> 8) & 0xFF) + ((b >> 8) & 0xFF)).min(255);
    let bl = (((a >> 16) & 0xFF) + ((b >> 16) & 0xFF)).min(255);
    0xFF00_0000 | (bl << 16) | (g << 8) | r
}

/// Fill a 256-entry LUT with the cyclic palette, shifted by `off` and scaled by `gain`.
fn cyc_lut(lut: &mut [u32; 256], pal: &Palette, off: f32, gain: f32) {
    for (i, e) in lut.iter_mut().enumerate() {
        *e = scale(pal.at(i as f32 / 256.0 + off), gain);
    }
}

/// Fingerprint of a palette, so cached textures are rebuilt when it changes.
fn pal_key(p: &Palette) -> u32 {
    p.at(0.1) ^ p.at(0.6).rotate_left(7) ^ p.hi.rotate_left(13) ^ p.bg.rotate_left(21)
}

/// Per-pixel polar coordinates around the screen centre: angle in turns (-0.5..0.5) and radius.
fn polar() -> (Vec<f32>, Vec<f32>) {
    let mut ang = Vec::with_capacity(W * H);
    let mut rad = Vec::with_capacity(W * H);
    for y in 0..H {
        for x in 0..W {
            let (dx, dy) = (x as f32 + 0.5 - WF / 2.0, y as f32 + 0.5 - HF / 2.0);
            ang.push(dy.atan2(dx) / TAU);
            rad.push((dx * dx + dy * dy).sqrt());
        }
    }
    (ang, rad)
}

/// A 256×256 tileable texture in palette colours, built lazily and rebuilt on palette change.
struct Tex {
    kind: u32,
    seed: u32,
    key: Option<u32>,
    px: Vec<u32>,
}

impl Tex {
    fn new(kind: u32, seed: u32) -> Self {
        Tex { kind, seed, key: None, px: Vec::new() }
    }

    fn update(&mut self, pal: &Palette) {
        let k = pal_key(pal);
        if self.key != Some(k) {
            self.key = Some(k);
            self.px = match self.kind {
                0 => xor_texture(pal),
                1 => tiles_texture(pal, self.seed),
                _ => sweets_texture(pal, self.seed),
            };
        }
    }
}

/// Bevelled tiles, each a different palette colour.
fn tiles_texture(pal: &Palette, seed: u32) -> Vec<u32> {
    let mut r = Rng::new(seed ^ 0x7111);
    let hues: Vec<f32> = (0..64).map(|_| r.f()).collect();
    let mut t = vec![0; 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            let (lx, ly) = (x % 32, y % 32);
            let base = pal.at(hues[(y / 32) * 8 + x / 32]);
            t[y * 256 + x] = if lx == 31 || ly == 31 {
                pal.bg
            } else if lx < 2 || ly < 2 {
                mix(base, rgb(255, 255, 255), 0.45)
            } else if lx > 28 || ly > 28 {
                scale(base, 0.45)
            } else {
                scale(base, 1.0 - (lx + ly) as f32 / 140.0)
            };
        }
    }
    t
}

/// A tileable plasma covered in shiny round sweets.
fn sweets_texture(pal: &Palette, seed: u32) -> Vec<u32> {
    let mut r = Rng::new(seed ^ 0x5eed);
    let k = TAU / 256.0;
    let mut t = vec![0; 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            let (fx, fy) = (x as f32 * k, y as f32 * k);
            let v = (fx * 2.0).sin() + (fy * 3.0).sin() + ((fx + fy) * 1.0).sin();
            t[y * 256 + x] = scale(pal.at(v * 0.12 + 0.3), 0.55);
        }
    }
    for _ in 0..34 {
        let (cx, cy, rad) = (r.range(0.0, 256.0), r.range(0.0, 256.0), r.range(7.0, 22.0));
        let col = pal.at(r.f());
        let ri = rad as i32 + 2;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let d = ((dx * dx + dy * dy) as f32).sqrt() / rad;
                if d > 1.08 {
                    continue;
                }
                let (px, py) = ((cx as i32 + dx) & 255, (cy as i32 + dy) & 255);
                let i = py as usize * 256 + px as usize;
                t[i] = if d > 1.0 {
                    scale(col, 0.15)
                } else {
                    let hl = (1.0 - (((dx as f32 / rad) + 0.35).powi(2) + ((dy as f32 / rad) + 0.35).powi(2)) * 3.0).max(0.0);
                    mix(scale(col, 1.0 - d * 0.5), rgb(255, 255, 255), hl)
                };
            }
        }
    }
    t
}

// ---------------------------------------------------------------- PLASMA

/// PLASMA: classic sum-of-sines plasma cycling through the palette (four formulas).
struct Plasma {
    kind: u32,
    f: [f32; 4],
    tt: f32,
    rot: f32,
    off: f32,
    col: Vec<f32>,
    row: Vec<f32>,
    diag: Vec<f32>,
    lut: [u32; 256],
}

impl Plasma {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(4);
        let f = [r.range(0.035, 0.06), r.range(0.04, 0.065), r.range(0.02, 0.035), r.range(0.05, 0.08)];
        Plasma {
            kind,
            f,
            tt: r.range(0.0, 50.0),
            rot: r.range(0.0, TAU),
            off: r.f(),
            col: vec![0.0; W],
            row: vec![0.0; H],
            diag: vec![0.0; W + H],
            lut: [0; 256],
        }
    }
}

impl Effect for Plasma {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tt += c.dt * (0.8 + a.bass * 1.4);
        self.rot += c.dt * (0.1 + a.mid * 0.5);
        self.off += c.dt * (0.03 + a.bass * 0.08);
        cyc_lut(&mut self.lut, c.pal, self.off + a.pulse * 0.1, 0.85 + 0.35 * a.pulse);
        let t = self.tt;
        let [f0, f1, f2, f3] = self.f;
        let (cx, cy) = (WF * 0.5 + fsin(t * 0.37) * WF * 0.3, HF * 0.5 + fsin(t * 0.29 + 1.0) * HF * 0.3);
        let (cx2, cy2) = (WF * 0.5 + fsin(t * 0.23 + 2.0) * WF * 0.35, HF * 0.5 + fsin(t * 0.41 + 4.0) * HF * 0.35);
        let lut = &self.lut;
        let idx = |v: f32| lut[(v * 44.0) as i32 as usize & 255];
        match self.kind {
            0 => {
                // four sines: columns, rows, diagonals and rings round a moving centre
                for (x, e) in self.col.iter_mut().enumerate() {
                    *e = fsin(x as f32 * f0 + t);
                }
                for (y, e) in self.row.iter_mut().enumerate() {
                    *e = fsin(y as f32 * f1 - t * 1.3);
                }
                for (k, e) in self.diag.iter_mut().enumerate() {
                    *e = fsin(k as f32 * f2 + t * 0.7);
                }
                let fr = f3 * (1.0 + a.bass * 0.3);
                for y in 0..H {
                    let dy2 = (y as f32 - cy).powi(2);
                    let (ry, out) = (self.row[y], &mut fb.px[y * W..(y + 1) * W]);
                    for (x, p) in out.iter_mut().enumerate() {
                        let d = ((x as f32 - cx).powi(2) + dy2).sqrt();
                        *p = idx(self.col[x] + ry + self.diag[x + y] + fsin(d * fr - t * 2.0));
                    }
                }
            }
            1 => {
                // two interfering ring sources over a soft grid
                for (x, e) in self.col.iter_mut().enumerate() {
                    *e = fsin(x as f32 * f2 + t * 0.5);
                }
                for (y, e) in self.row.iter_mut().enumerate() {
                    *e = fsin(y as f32 * f2 * 1.3 - t * 0.4);
                }
                for y in 0..H {
                    let (dy1, dy2) = ((y as f32 - cy).powi(2), (y as f32 - cy2).powi(2));
                    let (ry, out) = (self.row[y], &mut fb.px[y * W..(y + 1) * W]);
                    for (x, p) in out.iter_mut().enumerate() {
                        let d1 = ((x as f32 - cx).powi(2) + dy1).sqrt();
                        let d2 = ((x as f32 - cx2).powi(2) + dy2).sqrt();
                        *p = idx(fsin(d1 * f3 - t * 2.0) + fsin(d2 * f0 * 1.3 + t * 1.7) + self.col[x] + ry);
                    }
                }
            }
            2 => {
                // rotating, self-warping stripes plus rings
                let (sa, ca) = self.rot.sin_cos();
                for y in 0..H {
                    let yc = y as f32 - HF / 2.0;
                    let dy2 = (y as f32 - cy).powi(2);
                    let out = &mut fb.px[y * W..(y + 1) * W];
                    for (x, p) in out.iter_mut().enumerate() {
                        let xc = x as f32 - WF / 2.0;
                        let (u, w) = (xc * ca + yc * sa, yc * ca - xc * sa);
                        let d = ((x as f32 - cx).powi(2) + dy2).sqrt();
                        *p = idx(
                            fsin(u * f0 + t)
                                + fsin(w * f1 + 1.6 * fsin(u * f2 * 1.5 + t * 0.8))
                                + fsin(d * f2 * 1.5 - t),
                        );
                    }
                }
            }
            _ => {
                // liquid: each axis' sine bends the other
                for (x, e) in self.col.iter_mut().enumerate() {
                    *e = 2.2 * fsin(x as f32 * f1 - t * 1.1);
                }
                for (y, e) in self.row.iter_mut().enumerate() {
                    *e = 2.2 * fsin(y as f32 * f1 * 1.2 + t);
                }
                for (k, e) in self.diag.iter_mut().enumerate() {
                    *e = fsin(k as f32 * f2 + t * 0.6);
                }
                for y in 0..H {
                    let (ry, yf) = (self.row[y], y as f32 * f0 * 1.3);
                    let out = &mut fb.px[y * W..(y + 1) * W];
                    for (x, p) in out.iter_mut().enumerate() {
                        *p = idx(fsin(x as f32 * f0 + ry) + fsin(yf + self.col[x]) + self.diag[x + y]);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- FIRE

/// FIRE: classic rising fire from a hot bottom row, with fireballs flaring on beats.
struct Fire {
    s: usize,
    fw: usize,
    fh: usize,
    heat: Vec<u8>,
    coolmap: Vec<u8>,
    scroll: usize,
    rng: Rng,
    wind: i32,
    tt: f32,
    ph: [f32; 3],
    balls: Vec<[f32; 5]>,
    lut: [u32; 256],
}

impl Fire {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let s = 1 + r.below(2) as usize;
        let (fw, fh) = (W / s, H / s);
        let wind = r.below(3) as i32 - 1;
        // a blurred random field that scrolls up with the flames and cools them unevenly: wisps
        let mut cm: Vec<f32> = (0..fw * fh).map(|_| r.f()).collect();
        for _ in 0..1 {
            let src = cm.clone();
            for y in 0..fh {
                for x in 0..fw {
                    let at = |dx: usize, dy: usize| src[(y + dy) % fh * fw + (x + dx) % fw];
                    cm[y * fw + x] = (at(0, 0) + at(1, 0) + at(fw - 1, 0) + at(0, 1) + at(0, fh - 1)) / 5.0;
                }
            }
        }
        let (lo, hi) = cm.iter().fold((1.0f32, 0.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        let coolmap = cm.iter().map(|&v| (((v - lo) / (hi - lo)).powi(3) * 255.0) as u8).collect();
        let ph = [r.range(0.0, TAU), r.range(0.0, TAU), r.range(0.0, TAU)];
        Fire { s, fw, fh, heat: vec![0; fw * (fh + 3)], coolmap, scroll: 0, rng: r, wind, tt: 0.0, ph, balls: Vec::new(), lut: [0; 256] }
    }
}

impl Effect for Fire {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (fw, fh, s) = (self.fw, self.fh, self.s);
        let mut rng = Rng(self.rng.0);
        // hot source rows below the visible area: wandering coals (so flames form tongues)
        // that flicker, hotter and wider with the bass
        self.tt += c.dt;
        let (t, ph) = (self.tt, self.ph);
        let fs = s as f32;
        let thresh = 0.62 - a.bass * 0.22 - a.pulse * 0.1;
        for x in 0..fw {
            let xf = x as f32 * fs;
            let v = 0.5
                + 0.25 * fsin(xf * 0.061 + t * 1.3 + ph[0])
                + 0.15 * fsin(xf * 0.137 - t * 2.1 + ph[1])
                + 0.2 * fsin(xf * 0.023 + t * 0.6 + ph[2]);
            let flick = rng.u32() >> 24;
            let hv = if v > thresh && flick > 40 { 255 } else { (v * 90.0) as u8 };
            for row in 0..3 {
                self.heat[(fh + row) * fw + x] = hv;
            }
        }
        // fireballs: spawned on beats, rise and leave flames behind
        if a.beat {
            for _ in 0..2 + rng.below(3) {
                let x = rng.range(0.1, 0.9) * fw as f32;
                let vy = -rng.range(0.25, 0.5) * fh as f32;
                self.balls.push([x, fh as f32 - 2.0, rng.range(-0.08, 0.08) * fw as f32, vy, rng.range(0.6, 1.1)]);
            }
        }
        let rad = 5.0 / s as f32;
        for b in &mut self.balls {
            b[0] += b[2] * c.dt;
            b[1] += b[3] * c.dt;
            b[4] -= c.dt;
            let r = rad * (0.4 + b[4]).min(1.2);
            let ri = r.ceil() as i32;
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let (x, y) = (b[0] as i32 + dx, b[1] as i32 + dy);
                    if (dx * dx + dy * dy) as f32 <= r * r && x >= 0 && y >= 0 && (x as usize) < fw && (y as usize) < fh {
                        self.heat[y as usize * fw + x as usize] = 255;
                    }
                }
            }
        }
        self.balls.retain(|b| b[4] > 0.0 && b[1] > 0.0);
        // propagate upwards: average of the pixels below (jittered sideways), minus a cooling
        // map that scrolls up with the flames, so cool spots tear the fire into tongues
        let k = if s == 1 { 1.0 } else { 2.0 };
        let base = (k * 1.3 * (1.2 - 0.45 * a.bass) * 256.0) as u32;
        let cm_k = (k * 4.0) as u32;
        let steps = if s == 1 { 2 } else { 1 };
        let h = &mut self.heat;
        for _ in 0..steps {
            self.scroll = (self.scroll + 1) % fh;
            for y in 0..fh + 1 {
                let (r0, r1, r2) = (y * fw, (y + 1) * fw, (y + 2) * fw);
                let cm = &self.coolmap[(y + self.scroll) % fh * fw..][..fw];
                for x in 0..fw {
                    let rnd = rng.u32();
                    let j = (rnd % 3) as i32 - 1 + if rnd & 0x700 == 0 { self.wind } else { 0 };
                    let xs = (x as i32 + j).rem_euclid(fw as i32) as usize;
                    let xl = if xs == 0 { fw - 1 } else { xs - 1 };
                    let xr = if xs + 1 == fw { 0 } else { xs + 1 };
                    let sum = h[r1 + xl] as u32 + 2 * h[r1 + xs] as u32 + h[r1 + xr] as u32 + 2 * h[r2 + xs] as u32;
                    let loss = ((cm[x] as u32 * cm_k) >> 8) + ((base + ((rnd >> 8) & 255)) >> 8);
                    h[r0 + x] = ((sum * 341) >> 11).saturating_sub(loss) as u8;
                }
            }
        }
        self.rng = rng;
        let gain = 1.0 + 0.3 * a.pulse;
        for (i, e) in self.lut.iter_mut().enumerate() {
            *e = c.pal.ramp8((i as f32 * gain).min(255.0) as u8);
        }
        for y in 0..H {
            let src = &h[(y / s) * fw..(y / s + 1) * fw];
            let out = &mut fb.px[y * W..(y + 1) * W];
            if s == 1 {
                for (p, &v) in out.iter_mut().zip(src) {
                    *p = self.lut[v as usize];
                }
            } else {
                for (x, p) in out.iter_mut().enumerate() {
                    *p = self.lut[src[x / s] as usize];
                }
            }
        }
    }
}

// ---------------------------------------------------------------- METABALLS

/// METABALLS: 2D blobs merging into one isosurface, with glow or contour lines; bass grows them.
struct Metaballs {
    kind: u32,
    base: usize,
    mot: Vec<[f32; 5]>,
    dx2: Vec<f32>,
    r2: Vec<f32>,
    by: Vec<f32>,
    dy2: Vec<f32>,
    lut: [u32; 256],
    tt: f32,
    off: f32,
}

const EXTRA_BLOBS: usize = 3;

impl Metaballs {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(3);
        let base = 4 + r.below(3) as usize;
        let n = base + EXTRA_BLOBS;
        let mot = (0..n)
            .map(|_| [r.range(0.3, 1.1), r.range(0.3, 1.1), r.range(0.0, TAU), r.range(0.0, TAU), r.range(0.7, 1.25)])
            .collect();
        Metaballs {
            kind,
            base,
            mot,
            dx2: vec![0.0; n * W],
            r2: vec![0.0; n],
            by: vec![0.0; n],
            dy2: vec![0.0; n],
            lut: [0; 256],
            tt: r.range(0.0, 30.0),
            off: r.f(),
        }
    }
}

impl Effect for Metaballs {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tt += c.dt * (0.45 + a.bass * 0.6);
        self.off += c.dt * 0.05;
        let n = self.mot.len();
        for (i, m) in self.mot.iter().enumerate() {
            let mut rr = (24.0 + a.bass * 12.0 + a.pulse * 9.0) * m[4];
            if i >= self.base {
                rr *= smoothstep(a.bass * 3.0 - (i - self.base) as f32);
            }
            self.r2[i] = rr * rr;
            let bx = WF / 2.0 + fsin(self.tt * m[0] + m[2]) * WF * 0.36;
            self.by[i] = HF / 2.0 + fsin(self.tt * m[1] + m[3]) * HF * 0.32;
            for x in 0..W {
                self.dx2[x * n + i] = (x as f32 - bx).powi(2);
            }
        }
        // field value f in 0..4 → LUT index f * 64
        let (pal, off, p) = (c.pal, self.off, a.pulse);
        for (i, e) in self.lut.iter_mut().enumerate() {
            let f = i as f32 / 64.0;
            *e = match self.kind {
                0 if f < 1.0 => pal.ramp(f * f * 0.55),
                0 if f < 1.12 => mix(pal.hi, rgb(255, 255, 255), 0.3 + 0.5 * p),
                0 => scale(pal.at(off + f * 0.12), 0.6 + 0.4 * ((f - 1.0) / 1.5).min(1.0) + 0.2 * p),
                1 => {
                    let band = fract(f * 4.0 - off * 8.0);
                    let col = pal.at(off + f * 0.2);
                    if band < 0.2 {
                        scale(col, 1.0 + 0.3 * p)
                    } else if f >= 1.0 {
                        scale(col, 0.45)
                    } else {
                        scale(pal.bg, 1.0 + f)
                    }
                }
                _ => {
                    let g = pal.ramp((f * 0.38).min(0.95) + 0.1 * p);
                    if (1.0..1.08).contains(&f) { pal.hi } else { g }
                }
            };
        }
        for y in 0..H {
            for (d, &by) in self.dy2.iter_mut().zip(&self.by) {
                *d = (y as f32 - by).powi(2) + 1.0;
            }
            let out = &mut fb.px[y * W..(y + 1) * W];
            for (x, px) in out.iter_mut().enumerate() {
                let dx = &self.dx2[x * n..x * n + n];
                let f: f32 = dx.iter().zip(&self.dy2).zip(&self.r2).map(|((&dx, &dy), &r2)| r2 / (dx + dy)).sum();
                *px = self.lut[((f * 64.0) as usize).min(255)];
            }
        }
    }
}

// ---------------------------------------------------------------- MOIRE

/// MOIRE: concentric ring sets wandering over each other, XOR'ed into interference patterns.
struct Moire {
    kind: u32,
    dist: Vec<u16>,
    mot: [[f32; 4]; 3],
    tt: f32,
    flow: f32,
    off: f32,
    lut_a: [u32; 256],
    lut_b: [u32; 256],
}

impl Moire {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(3);
        // distance (×16) from the centre of a 2W×2H table; each ring set is a window into it
        let mut dist = Vec::with_capacity(4 * W * H);
        for y in 0..2 * H {
            for x in 0..2 * W {
                let (dx, dy) = (x as f32 - WF, y as f32 - HF);
                dist.push(((dx * dx + dy * dy).sqrt() * 16.0) as u16);
            }
        }
        let mut m = || [r.range(0.3, 0.9), r.range(0.3, 0.9), r.range(0.0, TAU), r.range(0.0, TAU)];
        let mot = [m(), m(), m()];
        Moire { kind, dist, mot, tt: 0.0, flow: 0.0, off: 0.0, lut_a: [0; 256], lut_b: [0; 256] }
    }
}

impl Effect for Moire {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tt += c.dt * (0.45 + a.bass * 0.7);
        self.flow = (self.flow + c.dt * (1.5 + a.pulse * 6.0)).rem_euclid(256.0);
        self.off += c.dt * 0.04;
        cyc_lut(&mut self.lut_a, c.pal, self.off, 1.0 + 0.25 * a.pulse);
        for (i, e) in self.lut_b.iter_mut().enumerate() {
            *e = mix(c.pal.bg, c.pal.at(i as f32 / 256.0 + self.off + 0.5), 0.18);
        }
        let width = 6.0 + 2.0 * fsin(self.tt * 0.4) - a.pulse * 1.5;
        let sc = (65536.0 / (16.0 * width)) as u32;
        let flow = (self.flow * 65536.0) as u32;
        let reach = 0.38 + 0.08 * a.bass;
        let offs: Vec<usize> = self
            .mot
            .iter()
            .map(|m| {
                let cx = (WF / 2.0 + fsin(self.tt * m[0] + m[2]) * WF * reach).clamp(0.0, WF - 1.0) as usize;
                let cy = (HF / 2.0 + fsin(self.tt * m[1] + m[3]) * HF * reach).clamp(0.0, HF - 1.0) as usize;
                (H - cy) * 2 * W + (W - cx)
            })
            .collect();
        let ring = |d: u16| (d as u32 * sc).wrapping_sub(flow) >> 16;
        for y in 0..H {
            let (o1, o2, o3) = (offs[0] + y * 2 * W, offs[1] + y * 2 * W, offs[2] + y * 2 * W);
            let out = &mut fb.px[y * W..(y + 1) * W];
            for (x, p) in out.iter_mut().enumerate() {
                let (i1, i2) = (ring(self.dist[o1 + x]), ring(self.dist[o2 + x]));
                *p = match self.kind {
                    0 => {
                        let idx = (i1.wrapping_add(i2) * 4) as usize & 255;
                        if (i1 ^ i2) & 1 == 1 { self.lut_a[idx] } else { self.lut_b[idx] }
                    }
                    1 => {
                        let i3 = ring(self.dist[o3 + x]);
                        let idx = (i1 * 6) as usize & 255;
                        if (i1 ^ i2 ^ i3) & 1 == 1 { self.lut_a[idx] } else { self.lut_b[idx] }
                    }
                    _ => {
                        let v = i1 ^ i2;
                        let idx = (v * 9) as usize & 255;
                        if (i1 & i2) & 2 == 0 { self.lut_a[idx] } else { self.lut_b[idx] }
                    }
                };
            }
        }
    }
}

// ---------------------------------------------------------------- RIPPLES

/// RIPPLES: water height field refracting a palette texture; a drop falls on every beat.
struct Ripples {
    cur: Vec<i32>,
    prev: Vec<i32>,
    tex: Tex,
    wake: bool,
    rng: Rng,
    tt: f32,
    scroll: (f32, f32),
}

impl Ripples {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(3);
        let wake = r.below(2) == 0;
        let scroll = (r.range(-12.0, 12.0), r.range(-8.0, 8.0));
        Ripples { cur: vec![0; W * H], prev: vec![0; W * H], tex: Tex::new(kind, seed), wake, rng: r, tt: 0.0, scroll }
    }

    fn drop(&mut self, x: f32, y: f32, r: f32, amp: f32) {
        let ri = r.ceil() as i32;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let (px, py) = (x as i32 + dx, y as i32 + dy);
                let d = ((dx * dx + dy * dy) as f32).sqrt() / r;
                if d < 1.0 && px > 0 && py > 0 && px < WI - 1 && py < HI - 1 {
                    self.cur[py as usize * W + px as usize] += (amp * 0.5 * (1.0 + (d * PI).cos())) as i32;
                }
            }
        }
    }
}

impl Effect for Ripples {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tex.update(c.pal);
        self.tt += c.dt;
        if a.beat || c.t == 0.0 {
            let (x, y) = (self.rng.range(40.0, WF - 40.0), self.rng.range(30.0, HF - 30.0));
            self.drop(x, y, 6.0 + a.bass * 6.0, 2600.0);
        }
        if self.rng.f() < a.treble * 0.35 {
            let (x, y) = (self.rng.range(5.0, WF - 5.0), self.rng.range(5.0, HF - 5.0));
            self.drop(x, y, 2.5, 500.0);
        }
        if self.wake {
            let t = self.tt * (0.6 + a.bass * 0.3);
            let (x, y) = (WF / 2.0 + (t * 0.9).sin() * WF * 0.38, HF / 2.0 + (t * 1.3).sin() * HF * 0.36);
            self.drop(x, y, 3.0, 160.0);
        }
        // wave steps: average of the neighbours minus the previous height, damped
        for _ in 0..2 {
            for y in 1..H - 1 {
                for x in 1..W - 1 {
                    let i = y * W + x;
                    let cur = &self.cur;
                    let n = ((cur[i - 1] + cur[i + 1] + cur[i - W] + cur[i + W]) >> 1) - self.prev[i];
                    self.prev[i] = n - (n >> 6);
                }
            }
            std::mem::swap(&mut self.cur, &mut self.prev);
        }
        // refract: the slope offsets the texture lookup and lights the surface
        let (sx, sy) = ((self.tt * self.scroll.0) as i32, (self.tt * self.scroll.1) as i32);
        let tex = &self.tex.px;
        let h = &self.cur;
        for y in 1..H - 1 {
            for x in 1..W - 1 {
                let i = y * W + x;
                let dx = h[i - 1] - h[i + 1];
                let dy = h[i - W] - h[i + W];
                let tx = (x as i32 + sx + (dx >> 4)) & 255;
                let ty = (y as i32 + sy + (dy >> 4)) & 255;
                let light = (256 + (dx + dy) / 2).clamp(70, 600) as u32;
                fb.px[i] = shade(tex[(ty * 256 + tx) as usize], light);
            }
        }
        for y in 0..H {
            fb.px[y * W] = fb.px[y * W + 1];
            fb.px[y * W + W - 1] = fb.px[y * W + W - 2];
        }
        fb.px.copy_within(W..2 * W, 0);
        fb.px.copy_within((H - 2) * W..(H - 1) * W, (H - 1) * W);
    }
}

// ---------------------------------------------------------------- JULIA

/// JULIA: animated Julia set whose constant orbits the Mandelbrot boundary; pulses punch the zoom.
struct Julia {
    kind: u32,
    maxit: u32,
    tt: f32,
    rot: f32,
    off: f32,
    lut: [u32; 256],
}

impl Julia {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        Julia { kind: r.below(3), maxit: 40 + r.below(3) * 8, tt: r.range(0.0, 20.0), rot: r.range(0.0, TAU), off: r.f(), lut: [0; 256] }
    }

    fn constant(&self) -> (f32, f32) {
        let t = self.tt;
        match self.kind {
            0 => {
                let a = t * 0.23;
                (0.7885 * a.cos(), 0.7885 * a.sin())
            }
            1 => {
                // just outside the main cardioid: dendrites and spirals
                let a = t * 0.17 + 1.0;
                let k = 1.0 + 0.03 * (t * 0.5).sin();
                (k * (a.cos() / 2.0 - (2.0 * a).cos() / 4.0), k * (a.sin() / 2.0 - (2.0 * a).sin() / 4.0))
            }
            _ => (-0.79 + 0.05 * (t * 0.31).sin(), 0.16 + 0.06 * (t * 0.47).cos()),
        }
    }
}

impl Effect for Julia {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tt += c.dt * (0.6 + a.bass * 0.6);
        self.rot += c.dt * (0.08 + a.mid * 0.3);
        self.off += c.dt * 0.06;
        for (i, e) in self.lut.iter_mut().enumerate() {
            let k = i as f32 / 255.0;
            let bright = 0.18 + 0.95 * smoothstep(k * 3.0) + 0.25 * a.pulse;
            *e = scale(c.pal.at(k * 1.3 + self.off), bright);
        }
        let inside = scale(c.pal.at(self.off + 0.5), 0.22 + 0.25 * a.pulse);
        let (cr, ci) = self.constant();
        let zoom = 1.0 + 0.35 * a.pulse;
        let sc = 3.3 / WF / zoom;
        let (sa, ca) = self.rot.sin_cos();
        let maxit = self.maxit;
        // f(-z) = f(z): compute the top half in 2×2 cells and mirror it through the centre
        for by in (0..H / 2).step_by(2) {
            for bx in (0..W).step_by(2) {
                let (px, py) = ((bx as f32 + 1.0 - WF / 2.0) * sc, (by as f32 + 1.0 - HF / 2.0) * sc);
                let (mut x, mut y) = (px * ca - py * sa, px * sa + py * ca);
                let mut n = 0;
                let mut m = 0.0;
                while n < maxit {
                    let (x2, y2) = (x * x, y * y);
                    m = x2 + y2;
                    if m > 16.0 {
                        break;
                    }
                    y = 2.0 * x * y + ci;
                    x = x2 - y2 + cr;
                    n += 1;
                }
                let col = if n == maxit {
                    inside
                } else {
                    let nu = n as f32 + 1.0 - (0.5 * m.ln()).log2();
                    self.lut[((nu * 255.0 / maxit as f32) as usize).min(255)]
                };
                let (mx, my) = (W - 2 - bx, H - 2 - by);
                for (ox, oy) in [(bx, by), (mx, my)] {
                    let i = oy * W + ox;
                    fb.px[i] = col;
                    fb.px[i + 1] = col;
                    fb.px[i + W] = col;
                    fb.px[i + W + 1] = col;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- MANDELBROT

/// MANDELBROT: a slow zoom into (and back out of) a boundary point, palette cycling.
struct Mandelbrot {
    target: (f64, f64),
    depth: f32,
    period: f32,
    tt: f32,
    off: f32,
    lut: [u32; 256],
    iters: Vec<u32>,
    nus: Vec<f32>,
    cols: Vec<u32>,
}

const MANDEL_SPOTS: [(f64, f64, f32); 4] = [
    (-0.152_8, 1.039_7, 10.5),
    (-0.101_096_363_845_622, 0.956_286_510_809_142, 11.0),
    (-1.543_689_012_692_076, 0.0, 11.0),
    (0.254_987_037_514_476_6, -0.000_567_979_052_846_5, 10.0),
];

/// Mandelbrot cells are 2×2 pixels.
const MW: usize = W / 2;
const MH: usize = H / 2;

/// Escape-time iteration: (count, smooth count); count == maxit means inside.
fn mandel(cr: f64, ci: f64, maxit: u32) -> (u32, f32) {
    // main cardioid and period-2 bulb are inside: skip them
    let q = (cr - 0.25).powi(2) + ci * ci;
    if q * (q + cr - 0.25) < 0.25 * ci * ci || (cr + 1.0).powi(2) + ci * ci < 0.0625 {
        return (maxit, 0.0);
    }
    let (mut x, mut y, mut n, mut m) = (0.0f64, 0.0f64, 0, 0.0);
    let (mut sx, mut sy, mut check) = (0.0, 0.0, 8);
    while n < maxit {
        let (x2, y2) = (x * x, y * y);
        m = x2 + y2;
        if m > 64.0 {
            break;
        }
        y = 2.0 * x * y + ci;
        x = x2 - y2 + cr;
        n += 1;
        // periodicity check: an orbit back at a saved point never escapes
        if (x - sx).abs() < 1e-7 && (y - sy).abs() < 1e-7 {
            return (maxit, 0.0);
        }
        if n == check {
            (sx, sy, check) = (x, y, check * 2);
        }
    }
    if n >= maxit {
        return (maxit, 0.0);
    }
    (n, n as f32 + 1.0 - (0.5 * (m as f32).ln()).log2())
}

impl Mandelbrot {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let (x, y, depth) = MANDEL_SPOTS[r.below(4) as usize];
        Mandelbrot {
            target: (x, y),
            depth,
            period: r.range(26.0, 34.0),
            tt: 0.0,
            off: r.f(),
            lut: [0; 256],
            iters: vec![0; MW * MH],
            nus: vec![0.0; MW * MH],
            cols: vec![0; MW * MH],
        }
    }
}

impl Effect for Mandelbrot {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tt += c.dt * (0.85 + a.bass * 0.3);
        self.off += c.dt * (0.08 + a.bass * 0.12);
        cyc_lut(&mut self.lut, c.pal, self.off, 1.0 + 0.3 * a.pulse);
        // ping-pong zoom on a log scale: in for half a period, back out for the other half
        let z = self.depth * smoothstep(super::tri(self.tt / self.period));
        let step = 1.7 * 0.5f64.powf(z as f64) * 4.0 / W as f64;
        let (sa, ca) = ((self.tt * 0.04) as f64).sin_cos();
        let (tx, ty) = self.target;
        // ease from the whole set towards the target while zooming starts
        let k = smoothstep(z / 2.0) as f64;
        let (ox, oy) = (-0.6 + (tx + 0.6) * k, ty * k);
        let maxit = 56 + (z * 8.0) as u32;
        let inside = scale(c.pal.bg, 0.6);
        let lut = &self.lut;
        let colour = |(n, nu): (u32, f32)| {
            if n >= maxit {
                return inside;
            }
            let col = lut[(nu * 9.0) as usize & 255];
            if nu < 10.0 { shade(col, (nu * 25.6) as u32) } else { col }
        };
        let point = |cx: usize, cy: usize| {
            let (px, py) = ((cx as f64 + 0.5 - MW as f64 / 2.0) * step, (cy as f64 + 0.5 - MH as f64 / 2.0) * step);
            mandel(ox + px * ca - py * sa, oy + px * sa + py * ca, maxit)
        };
        // pass 1: a checkerboard of cells; pass 2: the other cells, interpolated when the
        // four neighbours are all inside or within two iterations of each other
        for pass in 0..2 {
            for cy in 0..MH {
                for cx in ((cy + pass) & 1..MW).step_by(2) {
                    let i = cy * MW + cx;
                    if pass == 1 && cx > 0 && cy > 0 && cx < MW - 1 && cy < MH - 1 {
                        let nb = [i - 1, i + 1, i - MW, i + MW];
                        let (lo, hi) = nb.iter().fold((u32::MAX, 0), |(lo, hi), &j| (lo.min(self.iters[j]), hi.max(self.iters[j])));
                        if lo == hi || (hi < maxit && hi - lo <= 2) {
                            let nu = nb.iter().map(|&j| self.nus[j]).sum::<f32>() * 0.25;
                            self.iters[i] = lo;
                            self.nus[i] = nu;
                            self.cols[i] = colour((hi, nu));
                            continue;
                        }
                    }
                    let r = point(cx, cy);
                    self.iters[i] = r.0;
                    self.nus[i] = r.1;
                    self.cols[i] = colour(r);
                }
            }
        }
        for y in 0..H {
            let src = &self.cols[(y / 2) * MW..(y / 2 + 1) * MW];
            for (x, p) in fb.px[y * W..(y + 1) * W].iter_mut().enumerate() {
                *p = src[x / 2];
            }
        }
    }
}

// ---------------------------------------------------------------- KALEIDOSCOPE

/// KALEIDOSCOPE: a scrolling, rotating texture folded into mirrored wedges.
struct Kaleidoscope {
    n: f32,
    ang: Vec<f32>,
    rad: Vec<f32>,
    fold: Vec<(f32, f32)>,
    tex: Tex,
    spin: f32,
    tt: f32,
    rot: f32,
    pos: (f32, f32),
    dir: (f32, f32),
}

const FOLD_STEPS: usize = 512;

impl Kaleidoscope {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let n = [6.0, 8.0, 10.0, 12.0][r.below(4) as usize];
        let (ang, mut rad) = polar();
        // a mild fisheye in some variants
        let bulge = r.below(2) == 0;
        if bulge {
            for v in &mut rad {
                *v = v.powf(0.8) * 2.2;
            }
        }
        // cos/sin of the folded angle, for m in 0..0.5 of a wedge
        let fold = (0..=FOLD_STEPS)
            .map(|k| {
                let th = k as f32 / FOLD_STEPS as f32 * 0.5 * TAU / n;
                (th.cos(), th.sin())
            })
            .collect();
        let spin = if r.below(2) == 0 { 1.0 } else { -1.0 } * r.range(0.03, 0.08);
        let d = r.range(0.0, TAU);
        Kaleidoscope {
            n,
            ang,
            rad,
            fold,
            tex: Tex::new(r.below(3), seed),
            spin,
            tt: 0.0,
            rot: 0.0,
            pos: (0.0, 0.0),
            dir: (d.cos() * 30.0, d.sin() * 30.0),
        }
    }
}

impl Effect for Kaleidoscope {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tex.update(c.pal);
        self.tt += c.dt;
        self.rot += c.dt * (0.15 + a.mid * 0.6);
        let sp = 0.6 + a.bass * 1.2;
        self.pos.0 += self.dir.0 * sp * c.dt;
        self.pos.1 += self.dir.1 * sp * c.dt;
        let mirror = self.tt * self.spin;
        let zm = (0.75 + 0.2 * (self.tt * 0.3).sin()) * (1.0 - 0.3 * a.pulse);
        let (sr, cr) = self.rot.sin_cos();
        let (m00, m01, m10, m11) = (cr * zm, -sr * zm, sr * zm, cr * zm);
        let (ox, oy) = (self.pos.0 + 1024.0, self.pos.1 + 1024.0);
        let tex = &self.tex.px;
        let fs = FOLD_STEPS as f32 * 2.0;
        for (i, p) in fb.px.iter_mut().enumerate() {
            let w = fract((self.ang[i] + mirror) * self.n);
            let m = if w > 0.5 { 1.0 - w } else { w };
            let (cs, sn) = self.fold[(m * fs) as usize];
            let r = self.rad[i];
            let (u, v) = (r * cs, r * sn);
            let tx = (u * m00 + v * m01 + ox) as i32 & 255;
            let ty = (u * m10 + v * m11 + oy) as i32 & 255;
            *p = tex[(ty * 256 + tx) as usize];
        }
        if a.pulse > 0.05 {
            // a flash of light in the middle of the mirrors
            let k = a.pulse * 0.8;
            for y in 0..H {
                for x in 0..W {
                    let i = y * W + x;
                    let g = (1.0 - self.rad[i] / 90.0).max(0.0) * k;
                    if g > 0.01 {
                        fb.px[i] = addc(fb.px[i], scale(c.pal.hi, g));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------- HYPNO

/// HYPNO: rotating spirals and rings that step round in time with the beat.
struct Hypno {
    kind: u32,
    arms: f32,
    twist: f32,
    dir: f32,
    ang: Vec<f32>,
    lr: Vec<f32>,
    rad: Vec<f32>,
    zone: Vec<u8>,
    lut: Vec<u32>,
    tt: f32,
    off: f32,
}

const ZONES: usize = 32;

impl Hypno {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(4);
        let (ang, rad) = polar();
        let lr = rad.iter().map(|&r| r.max(1.0).ln()).collect();
        let zone = rad.iter().map(|&r| ((r / 8.0) as usize).min(ZONES - 1) as u8).collect();
        Hypno {
            kind,
            arms: (2 + r.below(4)) as f32,
            twist: r.range(2.0, 4.0),
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
            ang,
            lr,
            rad,
            zone,
            lut: vec![0; ZONES * 256],
            tt: 0.0,
            off: r.f(),
        }
    }
}

impl Effect for Hypno {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tt += c.dt;
        self.off += c.dt * 0.05;
        // beat-stepped spin: snaps forward on each beat, eases in between
        let ph = c.phase.clamp(0.0, 1.0);
        let beat = c.beats as f32 + (1.0 - (1.0 - ph).powi(3));
        let spin = (beat * 0.25 + self.tt * 0.15) * self.dir;
        // LUT: one stripe pair per 256 entries, hue and depth darkening per radial zone
        for z in 0..ZONES {
            let hue = self.off + z as f32 * 0.03;
            let depth = smoothstep(z as f32 / 6.0) * (0.85 + 0.35 * a.pulse) + 0.08;
            let bright = scale(c.pal.at(hue), depth);
            let dark = scale(c.pal.at(hue + 0.5), 0.12 * depth);
            for i in 0..256 {
                let k = (fsin(i as f32 / 256.0 * TAU) * 2.5 + 0.5).clamp(0.0, 1.0);
                self.lut[z * 256 + i] = mix(dark, bright, k);
            }
        }
        let (arms, tw) = (self.arms, self.twist * (1.0 + a.mid * 0.3));
        let push = a.pulse * 0.35;
        let wob = a.bass * 0.35;
        for i in 0..W * H {
            let (an, lr) = (self.ang[i], self.lr[i]);
            let idx = match self.kind {
                0 => ((an * arms + (lr - push) * tw - spin) * 256.0) as i32 & 255,
                1 => {
                    let v = (self.rad[i] - push * 20.0) * 0.05 - spin * 2.0 + wob * fsin(an * TAU * arms + self.tt * 2.0);
                    (v * 256.0) as i32 & 255
                }
                2 => {
                    let v1 = an * arms + lr * tw - spin;
                    let v2 = -an * arms + lr * tw - spin * 0.5;
                    if ((v1 * 2.0).floor() as i32 ^ (v2 * 2.0).floor() as i32) & 1 == 1 { 64 } else { 192 }
                }
                _ => {
                    let v = an * arms + self.rad[i] * 0.025 * tw - spin;
                    (v * 256.0) as i32 & 255
                }
            };
            fb.px[i] = self.lut[self.zone[i] as usize * 256 + idx as usize];
        }
    }
}

// ---------------------------------------------------------------- LIFE

/// LIFE: Conway-style cellular automaton on a coarse grid, coloured by cell age; beats reseed patches.
struct Life {
    cs: usize,
    gw: usize,
    gh: usize,
    birth: u16,
    survive: u16,
    cells: Vec<u8>,
    next: Vec<u8>,
    age: Vec<u8>,
    ghost: Vec<u8>,
    rng: Rng,
    acc: f32,
    hue: f32,
    lut_age: [u32; 256],
    lut_ghost: [u32; 64],
    row: Vec<u32>,
}

impl Life {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let cs = if r.below(2) == 0 { 4 } else { 6 };
        let (gw, gh) = (W / cs, H / cs);
        // B3/S23 (Life), B36/S23 (HighLife), B3678/S34678 (Day & Night)
        let (birth, survive, fill) = match r.below(3) {
            0 => (1 << 3, (1 << 2) | (1 << 3), 0.3),
            1 => ((1 << 3) | (1 << 6), (1 << 2) | (1 << 3), 0.3),
            _ => (0b1_1100_1000, 0b1_1101_1000, 0.5),
        };
        let cells = (0..gw * gh).map(|_| (r.f() < fill) as u8).collect();
        Life {
            cs,
            gw,
            gh,
            birth,
            survive,
            cells,
            next: vec![0; gw * gh],
            age: vec![0; gw * gh],
            ghost: vec![0; gw * gh],
            hue: r.f(),
            rng: r,
            acc: 0.0,
            lut_age: [0; 256],
            lut_ghost: [0; 64],
            row: vec![0; W / cs],
        }
    }

    fn step(&mut self) {
        let (gw, gh) = (self.gw, self.gh);
        for y in 0..gh {
            let (ym, yp) = ((y + gh - 1) % gh * gw, (y + 1) % gh * gw);
            let yr = y * gw;
            for x in 0..gw {
                let (xm, xp) = ((x + gw - 1) % gw, (x + 1) % gw);
                let c = &self.cells;
                let n = c[ym + xm] + c[ym + x] + c[ym + xp] + c[yr + xm] + c[yr + xp] + c[yp + xm] + c[yp + x] + c[yp + xp];
                let alive = c[yr + x] == 1;
                let mask = if alive { self.survive } else { self.birth };
                self.next[yr + x] = ((mask >> n) & 1) as u8;
            }
        }
        std::mem::swap(&mut self.cells, &mut self.next);
        for i in 0..gw * gh {
            if self.cells[i] == 1 {
                self.age[i] = if self.next[i] == 1 { self.age[i].saturating_add(1) } else { 0 };
                self.ghost[i] = 63;
            } else {
                self.ghost[i] = self.ghost[i].saturating_sub(4);
            }
        }
    }

    fn seed_patch(&mut self, w: usize, h: usize, density: f32) {
        let x0 = self.rng.below(self.gw as u32) as usize;
        let y0 = self.rng.below(self.gh as u32) as usize;
        for y in 0..h {
            for x in 0..w {
                if self.rng.f() < density {
                    let i = (y0 + y) % self.gh * self.gw + (x0 + x) % self.gw;
                    self.cells[i] = 1;
                    self.age[i] = 0;
                }
            }
        }
    }
}

impl Effect for Life {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        if a.beat {
            let (w, h) = (8 + self.rng.below(10) as usize, 6 + self.rng.below(8) as usize);
            self.seed_patch(w, h, 0.45);
        }
        let pop = self.cells.iter().filter(|&&v| v == 1).count();
        if pop < self.gw * self.gh / 30 {
            self.seed_patch(self.gw / 3, self.gh / 3, 0.4);
        }
        // a few generations per beat, faster with bass
        self.acc += c.dt * (3.0 + a.bass * 3.0) / a.period.max(0.3);
        let mut steps = 0;
        while self.acc >= 1.0 && steps < 4 {
            self.acc -= 1.0;
            self.step();
            steps += 1;
        }
        self.acc = self.acc.min(1.0);
        self.hue += c.dt * 0.02;
        let pal = c.pal;
        for (i, e) in self.lut_age.iter_mut().enumerate() {
            *e = if i == 0 {
                mix(pal.hi, rgb(255, 255, 255), 0.2 + 0.6 * a.pulse)
            } else {
                scale(pal.at(self.hue + (i as f32).sqrt() * 0.08), 1.0 - (i as f32 / 400.0) + 0.2 * a.pulse)
            };
        }
        let bg = pal.bg;
        let gap = scale(bg, 0.5);
        for (i, e) in self.lut_ghost.iter_mut().enumerate() {
            *e = mix(bg, pal.at(self.hue + 0.5), i as f32 / 63.0 * 0.4);
        }
        let cs = self.cs;
        for gy in 0..self.gh {
            for gx in 0..self.gw {
                let i = gy * self.gw + gx;
                self.row[gx] = if self.cells[i] == 1 {
                    self.lut_age[self.age[i] as usize]
                } else {
                    self.lut_ghost[self.ghost[i] as usize]
                };
            }
            for py in 0..cs {
                let y = gy * cs + py;
                let out = &mut fb.px[y * W..(y + 1) * W];
                if py == cs - 1 {
                    out.fill(gap);
                    continue;
                }
                for (gx, cell) in out.chunks_exact_mut(cs).enumerate() {
                    cell[..cs - 1].fill(self.row[gx]);
                    cell[cs - 1] = gap;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- GLYPH RAIN

/// GLYPH RAIN: falling columns of font glyphs in palette colours; bass speeds them up,
/// and now and then a word lights up across the columns.
struct GlyphRain {
    sc: i32,
    cols: usize,
    rows: usize,
    up: bool,
    bits: Vec<[u8; 7]>,
    cells: Vec<u8>,
    drops: Vec<[f32; 4]>,
    word: Vec<(usize, usize, u8)>,
    word_t: f32,
    rng: Rng,
    off: f32,
}

const RAIN_GLYPHS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZÅÄÖ0123456789+*=<>#&?!/";
const RAIN_WORDS: [&str; 8] = ["UFO", "PIXEL", "HEJ", "RYMD", "DEMO", "BEAT", "LJUS", "BASS"];

impl GlyphRain {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let sc = if r.below(3) == 0 { 2 } else { 1 };
        let cols = W / (6 * sc as usize);
        let rows = H / (9 * sc as usize) + 1;
        let bits: Vec<[u8; 7]> = RAIN_GLYPHS.chars().map(font::glyph).collect();
        let cells = (0..cols * rows).map(|_| r.below(bits.len() as u32) as u8).collect();
        let per = 2;
        let mut drops = Vec::with_capacity(cols * per);
        for k in 0..cols * per {
            let len = r.range(5.0, rows as f32 * 0.8);
            drops.push([(k % cols) as f32, r.range(-(rows as f32), rows as f32), r.range(6.0, 16.0), len]);
        }
        GlyphRain { sc, cols, rows, up: r.below(4) == 0, bits, cells, drops, word: Vec::new(), word_t: 0.0, rng: r, off: 0.0 }
    }

    fn glyph(&self, fb: &mut Fb, g: u8, col: usize, row: usize, c: u32) {
        let (sc, cw, ch) = (self.sc, 6 * self.sc, 9 * self.sc);
        let row = if self.up { self.rows - 1 - row } else { row };
        let (x0, y0) = (col as i32 * cw, row as i32 * ch + sc);
        for (gy, bits) in self.bits[g as usize].iter().enumerate() {
            for gx in 0..5 {
                if (bits >> (4 - gx)) & 1 == 1 {
                    fb.fill_rect(x0 + gx * sc, y0 + gy as i32 * sc, sc, sc, c);
                }
            }
        }
    }
}

impl Effect for GlyphRain {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let pal = c.pal;
        self.off += c.dt * 0.03;
        fb.clear(scale(pal.bg, 0.7));
        // every glyph faintly, so the screen reads as a wall of characters
        let faint = mix(scale(pal.bg, 0.7), pal.at(self.off + 0.3), 0.1);
        for r in 0..self.rows {
            for col in 0..self.cols {
                self.glyph(fb, self.cells[r * self.cols + col], col, r, faint);
            }
        }
        let rows = self.rows as f32;
        let speed = 0.45 + a.bass * 1.3;
        let n = self.bits.len() as u32;
        // flicker: a few glyphs change every frame, more with treble
        for _ in 0..(2.0 + a.treble * 14.0) as usize {
            let i = self.rng.below(self.cells.len() as u32) as usize;
            self.cells[i] = self.rng.below(n) as u8;
        }
        for k in 0..self.drops.len() {
            let mut d = self.drops[k];
            d[1] += d[2] * speed * c.dt;
            if d[1] - d[3] > rows {
                d[1] = -self.rng.range(0.0, rows * 0.6);
                d[2] = self.rng.range(6.0, 16.0);
                d[3] = self.rng.range(5.0, rows * 0.8);
            }
            self.drops[k] = d;
            let col = d[0] as usize;
            let hue = self.off + col as f32 * 0.013;
            let head = d[1].floor();
            for r in 0..self.rows {
                let dist = head - r as f32;
                if dist < 0.0 || dist >= d[3] {
                    continue;
                }
                let colour = if dist < 1.0 {
                    mix(pal.hi, rgb(255, 255, 255), 0.5 + 0.5 * a.pulse)
                } else {
                    let k = 1.0 - dist / d[3];
                    mix(pal.ramp(0.1 + 0.55 * k * k), pal.at(hue + dist * 0.015), 0.3 * k)
                };
                self.glyph(fb, self.cells[r * self.cols + col], col, r, colour);
            }
        }
        // every fourth beat a word freezes in the rain
        if a.beat && c.beats.is_multiple_of(4) {
            let w = RAIN_WORDS[self.rng.below(RAIN_WORDS.len() as u32) as usize];
            let len = w.chars().count();
            let c0 = self.rng.below((self.cols - len) as u32) as usize;
            let r0 = 2 + self.rng.below(self.rows as u32 - 4) as usize;
            self.word = w
                .chars()
                .enumerate()
                .map(|(i, ch)| (c0 + i, r0, RAIN_GLYPHS.chars().position(|g| g == ch).unwrap_or(0) as u8))
                .collect();
            self.word_t = 1.6;
        }
        if self.word_t > 0.0 {
            self.word_t -= c.dt;
            let k = (self.word_t / 0.5).min(1.0);
            let colour = mix(pal.at(self.off), rgb(255, 255, 255), 0.4 + 0.4 * a.pulse);
            for &(col, row, g) in &self.word {
                self.glyph(fb, g, col, row, scale(colour, k));
            }
        }
    }
}

// ---------------------------------------------------------------- VORONOI

/// VORONOI: moving Worley cells with glowing edges; random cells light up on each beat.
struct Voronoi {
    kind: u32,
    metric: u32,
    mot: Vec<[f32; 4]>,
    pts: Vec<(f32, f32)>,
    cell_col: Vec<u32>,
    glow: [u32; 64],
    tt: f32,
    off: f32,
}

const VCELL: usize = 48;
const VGX: usize = W / VCELL + 2;
const VGY: usize = H / VCELL + 3;

impl Voronoi {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(3);
        let metric = r.below(3);
        let mot = (0..VGX * VGY).map(|_| [r.range(0.4, 1.2), r.range(0.4, 1.2), r.range(0.0, TAU), r.range(0.0, TAU)]).collect();
        Voronoi {
            kind,
            metric,
            mot,
            pts: vec![(0.0, 0.0); VGX * VGY],
            cell_col: vec![0; VGX * VGY],
            glow: [0; 64],
            tt: 0.0,
            off: r.f(),
        }
    }

    fn render<M: Fn(f32, f32) -> f32>(&self, fb: &mut Fb, dist: M, edge_k: f32) {
        let shade_k = 256.0 / VCELL as f32;
        for y in 0..H {
            let cy = y / VCELL + 1;
            let fy = y as f32;
            for x in 0..W {
                let cx = x / VCELL + 1;
                let fx = x as f32;
                let (mut d1, mut d2, mut id) = (f32::MAX, f32::MAX, 0);
                for j in cy - 1..=cy + 1 {
                    for i in cx - 1..=cx + 1 {
                        let k = j * VGX + i;
                        let (px, py) = self.pts[k];
                        let d = dist(px - fx, py - fy);
                        if d < d1 {
                            (d2, d1, id) = (d1, d, k);
                        } else if d < d2 {
                            d2 = d;
                        }
                    }
                }
                let (f1, f2) = if self.metric == 0 { (d1.sqrt(), d2.sqrt()) } else { (d1, d2) };
                let inner = (300.0 - f1 * shade_k * 1.2).max(40.0) as u32;
                let base = shade(self.cell_col[id], inner);
                let e = (((f2 - f1) * edge_k) as usize).min(63);
                fb.px[y * W + x] = addc(base, self.glow[e]);
            }
        }
    }
}

impl Effect for Voronoi {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let pal = c.pal;
        self.tt += c.dt * (0.5 + a.bass * 0.9);
        self.off += c.dt * 0.03;
        for j in 0..VGY {
            for i in 0..VGX {
                let k = j * VGX + i;
                let m = self.mot[k];
                let bx = (i as f32 - 0.5) * VCELL as f32;
                let by = (j as f32 - 0.5) * VCELL as f32;
                self.pts[k] = (
                    bx + fsin(self.tt * m[0] + m[2]) * VCELL as f32 * 0.4,
                    by + fsin(self.tt * m[1] + m[3]) * VCELL as f32 * 0.4,
                );
                // a different handful of cells flashes on every beat
                let lit = (k as u32 * 2_654_435_761u32).wrapping_add(c.beats.wrapping_mul(0x9E37_79B9)) >> 29 == 0;
                let hue = fract(k as f32 * 0.618 + self.off);
                self.cell_col[k] = match self.kind {
                    0 => scale(pal.at(hue), 0.55 + if lit { a.pulse * 0.9 } else { 0.0 }),
                    1 => scale(pal.bg, 1.0 + if lit { a.pulse * 6.0 } else { 0.0 }),
                    _ => scale(pal.at(hue * 0.3 + self.off), 0.35 + if lit { a.pulse * 0.8 } else { 0.0 }),
                };
            }
        }
        let edge_col = match self.kind {
            1 => pal.at(self.off * 2.0),
            _ => pal.hi,
        };
        let width = 1.0 + a.treble * 0.6;
        for (e, g) in self.glow.iter_mut().enumerate() {
            let d = e as f32 / 4.0 / width;
            let k = 1.0 / (1.0 + d * d * 0.6) * (0.9 + 0.3 * a.pulse);
            *g = scale(edge_col, if self.kind == 1 { k * 1.3 } else { k });
        }
        match self.metric {
            0 => self.render(fb, |dx, dy| dx * dx + dy * dy, 4.0),
            1 => self.render(fb, |dx: f32, dy: f32| dx.abs() + dy.abs(), 3.0),
            _ => self.render(fb, |dx: f32, dy: f32| dx.abs().max(dy.abs()), 4.0),
        }
    }
}

// ---------------------------------------------------------------- LAVA

/// LAVA: slow, thick domain-warped noise (lava, oil film or marble); bass stirs the warp.
struct Lava {
    kind: u32,
    perm: [u8; 512],
    val: [f32; 256],
    field: Vec<f32>,
    rowbuf: Vec<f32>,
    lut: [u32; 256],
    tt: f32,
    off: f32,
}

const LS: usize = 4;
const LGW: usize = W / LS + 1;
const LGH: usize = H / LS + 1;

impl Lava {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let mut perm = [0u8; 512];
        let mut p: Vec<u8> = (0..=255).collect();
        for i in (1..256).rev() {
            p.swap(i, r.below(i as u32 + 1) as usize);
        }
        for i in 0..512 {
            perm[i] = p[i & 255];
        }
        let mut val = [0.0; 256];
        for v in &mut val {
            *v = r.f();
        }
        Lava { kind: r.below(3), perm, val, field: vec![0.0; LGW * LGH], rowbuf: vec![0.0; LGW], lut: [0; 256], tt: r.range(0.0, 40.0), off: r.f() }
    }

    #[inline]
    fn noise(&self, x: f32, y: f32) -> f32 {
        let (xf, yf) = (x.floor(), y.floor());
        let (xi, yi) = (xf as i32 as usize & 255, yf as i32 as usize & 255);
        let (fx, fy) = (x - xf, y - yf);
        let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let p = &self.perm;
        let (a, b) = (p[xi] as usize + yi, p[xi + 1] as usize + yi);
        let (v00, v01) = (self.val[p[a] as usize], self.val[p[a + 1] as usize]);
        let (v10, v11) = (self.val[p[b] as usize], self.val[p[b + 1] as usize]);
        let top = v00 + (v10 - v00) * u;
        let bot = v01 + (v11 - v01) * u;
        top + (bot - top) * v
    }

    fn fbm(&self, mut x: f32, mut y: f32) -> f32 {
        let mut s = self.noise(x, y);
        (x, y) = (x * 1.6 + y * 1.2 + 3.1, y * 1.6 - x * 1.2 + 1.7);
        s += 0.5 * self.noise(x, y);
        (x, y) = (x * 1.6 + y * 1.2 + 7.3, y * 1.6 - x * 1.2 + 4.9);
        s += 0.25 * self.noise(x, y);
        s / 1.75
    }
}

impl Effect for Lava {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.tt += c.dt * (0.12 + a.bass * 0.15);
        self.off += c.dt * 0.02;
        let t = self.tt;
        let warp = 3.0 + a.bass * 2.0 + a.pulse * 0.8;
        let ns = 3.5 / WF;
        // the warped field on a coarse grid; it is smooth, so bilinear upsampling hides the grid
        for gy in 0..LGH {
            for gx in 0..LGW {
                let (x, y) = ((gx * LS) as f32 * ns * 1.2, (gy * LS) as f32 * ns * 1.2);
                let qx = self.fbm(x + t * 0.5, y + 0.3 * t);
                let qy = self.fbm(x + 5.2 - t * 0.3, y + 1.3);
                let f = self.fbm(x + warp * qx + 1.7, y + warp * qy + 9.2 + t * 0.2);
                self.field[gy * LGW + gx] = match self.kind {
                    0 => ((f - 0.3) * 2.6 + (qx - 0.5) * 0.5).clamp(0.0, 0.999),
                    1 => f * 3.0 + qx * 1.5,
                    _ => f * 5.0 + qy * 2.0,
                };
            }
        }
        let (pal, p) = (c.pal, a.pulse);
        for (i, e) in self.lut.iter_mut().enumerate() {
            let k = i as f32 / 256.0;
            *e = match self.kind {
                0 => pal.ramp((k * k * 1.1 + 0.08 + 0.12 * p).min(0.999)),
                1 => scale(pal.at(k + self.off), 0.75 + 0.35 * (k * TAU * 2.0).sin().abs() + 0.2 * p),
                _ => {
                    let vein = (1.0 - (k * TAU).sin().abs()).powi(6);
                    mix(scale(pal.at(self.off + k * 0.25), 0.55 + 0.2 * p), pal.hi, vein)
                }
            };
        }
        let wrap = self.kind != 0;
        for y in 0..H {
            let (gy, fy) = (y / LS, (y % LS) as f32 / LS as f32);
            let (r0, r1) = (&self.field[gy * LGW..(gy + 1) * LGW], &self.field[(gy + 1) * LGW..(gy + 2) * LGW]);
            for (b, (&u, &v)) in self.rowbuf.iter_mut().zip(r0.iter().zip(r1)) {
                *b = u + (v - u) * fy;
            }
            let out = &mut fb.px[y * W..(y + 1) * W];
            for (x, px) in out.iter_mut().enumerate() {
                let (gx, fx) = (x / LS, (x % LS) as f32 / LS as f32);
                let v = self.rowbuf[gx] + (self.rowbuf[gx + 1] - self.rowbuf[gx]) * fx;
                let i = (v * 256.0) as i32;
                *px = self.lut[if wrap { i as usize & 255 } else { i.clamp(0, 255) as usize }];
            }
        }
    }
}
