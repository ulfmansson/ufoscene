//! A Swedish Christmas night, the seasonal scene that opens the show from 23 to 28
//! December: a Falu red cottage with a candle bridge and a paper star in the windows, a
//! straw goat, a tree whose lights blink on the beat, northern lights over snowy hills,
//! smoke from the chimney, and GOD JUL dancing in the sky. All drawn from scratch.
//!
//! It is not in `all()`, so the 100 demos stay as they are; see `seasonal()`.

use std::f32::consts::{PI, TAU};

use super::*;
use crate::font;

pub fn def() -> Def {
    Def { name: "GOD JUL", flags: VECTOR | OWN_TEXT, make: |s| Box::new(Christmas::new(s)) }
}

const HORIZON: f32 = 150.0;
const TREE_X: f32 = 300.0;
const TREE_BASE: f32 = 186.0;
const TREE_H: f32 = 104.0;
/// Top-left corners of the cottage windows (16×14 each).
const WINDOWS: [(i32, i32); 2] = [(80, 150), (122, 150)];
const CHIMNEY: (f32, f32) = (122.0, 101.0);
const WHITE: u32 = 0xFFFF_F8F0;

fn far_hill(x: f32) -> f32 {
    146.0 + 7.0 * (x * 0.013 + 1.0).sin() + 4.0 * (x * 0.031).sin()
}

fn ground(x: f32) -> f32 {
    176.0 + 5.0 * (x * 0.017 + 2.0).sin() + 2.0 * (x * 0.05).sin()
}

pub struct Christmas {
    sky: Vec<u32>,
    /// Hills, trees, the cottage and the goat; 0 = see-through.
    front: Fb,
    stars: Vec<(i32, i32, f32)>,
    flakes: Vec<(f32, f32, f32)>,
    /// Chimney smoke: x, y, age.
    smoke: Vec<(f32, f32, f32)>,
    lights: Vec<(i32, i32, usize)>,
    pal: Palette,
    next_puff: f32,
}

impl Christmas {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed.wrapping_mul(7919) ^ 2412);
        let mut sky = vec![0; W * H];
        for y in 0..H {
            let k = (y as f32 / HORIZON).min(1.0);
            sky[y * W..(y + 1) * W].fill(mix(rgb(4, 6, 26), rgb(36, 46, 100), k * k));
        }
        let stars = (0..90).map(|_| ((rng.f() * W as f32) as i32, (rng.f() * 135.0) as i32, rng.f() * TAU)).collect();
        let flakes = (0..45).map(|_| (rng.f() * W as f32, rng.f() * H as f32, 0.5 + rng.f() * 0.5)).collect();
        let mut front = Fb::new();
        front.clear(0);
        landscape(&mut front, &mut rng);
        cottage(&mut front);
        goat(&mut front, 196, ground(196.0) as i32 + 6);
        let lights = tree(&mut front);
        // red to gold, for the dancing letters
        let pal = Palette::cosine([0.8, 0.45, 0.2], [0.2, 0.45, 0.2], [1.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
        Christmas { sky, front, stars, flakes, smoke: Vec::new(), lights, pal, next_puff: 0.0 }
    }
}

/// A pine: tiers of triangles, a little snow on each, a trunk.
fn pine(fb: &mut Fb, cx: f32, base: f32, h: f32, col: u32, snow: u32) {
    fb.fill_rect((cx - h / 16.0) as i32, (base - h * 0.14) as i32, (h / 8.0).max(2.0) as i32, (h * 0.14) as i32 + 1, rgb(60, 38, 24));
    for i in 0..4 {
        let apex = base - h + i as f32 * h * 0.2;
        let bottom = apex + h * 0.36;
        let hw = h * (0.13 + i as f32 * 0.075);
        fb.tri([(cx, apex), (cx - hw, bottom), (cx + hw, bottom)], col, 1.0);
        let sy = apex + h * 0.1;
        fb.tri([(cx, apex), (cx - hw * 0.3, sy), (cx + hw * 0.3, sy)], snow, 1.0);
        fb.hspan((cx - hw) as i32, (cx - hw * 0.4) as i32, bottom as i32 - 1, snow);
    }
}

fn landscape(fb: &mut Fb, rng: &mut Rng) {
    for x in 0..W {
        let y0 = far_hill(x as f32) as usize;
        for y in y0..H {
            fb.px[y * W + x] = mix(rgb(120, 135, 185), rgb(160, 175, 220), ((y - y0) as f32 / 30.0).min(1.0));
        }
    }
    for _ in 0..18 {
        let x = rng.f() * W as f32;
        let h = 12.0 + rng.f() * 16.0;
        pine(fb, x, far_hill(x) + 3.0, h, rgb(22, 42, 60), rgb(150, 165, 210));
    }
    for x in 0..W {
        let y0 = ground(x as f32) as usize;
        for y in y0..H {
            fb.px[y * W + x] = mix(rgb(232, 238, 252), rgb(165, 180, 225), ((y - y0) as f32 / 40.0).min(1.0));
        }
    }
    for (x, h) in [(30.0, 74.0), (236.0, 48.0), (366.0, 66.0)] {
        pine(fb, x, ground(x) + 5.0, h, rgb(16, 62, 42), rgb(225, 232, 250));
    }
}

/// A red Swedish cottage with white corners, a snowy roof, a chimney and a wreath on the door.
fn cottage(fb: &mut Fb) {
    let red = rgb(146, 30, 24);
    fb.fill_rect(72, 142, 72, 37, red);
    for x in (76..141).step_by(5) {
        fb.fill_rect(x, 142, 1, 37, rgb(120, 22, 18));
    }
    fb.fill_rect(72, 142, 3, 37, WHITE);
    fb.fill_rect(141, 142, 3, 37, WHITE);
    fb.fill_rect(118, 104, 8, 16, rgb(110, 38, 30));
    fb.fill_rect(117, 101, 10, 3, WHITE);
    let roof = rgb(46, 38, 44);
    fb.tri([(66.0, 143.0), (150.0, 143.0), (132.0, 118.0)], roof, 1.0);
    fb.tri([(66.0, 143.0), (132.0, 118.0), (84.0, 118.0)], roof, 1.0);
    let snow = rgb(234, 240, 253);
    fb.tri([(68.0, 139.0), (148.0, 139.0), (131.0, 116.0)], snow, 1.0);
    fb.tri([(68.0, 139.0), (131.0, 116.0), (85.0, 116.0)], snow, 1.0);
    // icicles along the roof edge
    for (i, x) in (70..147).step_by(4).enumerate() {
        fb.fill_rect(x, 140, 1, 2 + (i * 7 % 4) as i32, rgb(200, 220, 255));
    }
    for &(wx, wy) in &WINDOWS {
        fb.fill_rect(wx, wy, 16, 14, WHITE);
    }
    // door with a wreath
    fb.fill_rect(101, 155, 14, 24, WHITE);
    fb.fill_rect(103, 157, 10, 22, rgb(70, 46, 32));
    for k in 0..16 {
        let a = k as f32 / 16.0 * TAU;
        fb.put(108 + (a.cos() * 3.0).round() as i32, 164 + (a.sin() * 3.0).round() as i32, rgb(30, 110, 50));
    }
    fb.fill_rect(107, 167, 2, 2, rgb(220, 30, 30));
    // a drift of snow along the wall
    for x in 60..158 {
        let top = 176.0 + 2.0 * (x as f32 * 0.31).sin() + 1.5 * (x as f32 * 0.13).sin();
        for y in top as i32..184 {
            fb.put(x, y, mix(rgb(240, 244, 255), rgb(200, 210, 240), (y as f32 - top) / 8.0));
        }
    }
}

/// A straw goat with red ribbons, facing right; `base` is where its hooves stand.
fn goat(fb: &mut Fb, x: i32, base: i32) {
    let straw = rgb(220, 170, 72);
    let dark = rgb(150, 104, 36);
    let red = rgb(205, 30, 30);
    for lx in [x - 11, x - 7, x + 3, x + 7] {
        fb.fill_rect(lx, base - 15, 2, 15, straw);
    }
    fb.fill_rect(x - 12, base - 22, 22, 7, straw);
    fb.hspan(x - 12, x + 10, base - 15, dark);
    fb.fill_rect(x + 7, base - 31, 4, 10, straw);
    fb.fill_rect(x + 8, base - 34, 9, 4, straw);
    fb.put(x + 16, base - 32, dark);
    fb.fill_rect(x - 15, base - 24, 3, 3, straw);
    for k in 0..2 {
        for (dx, dy) in [(0, 0), (-1, -1), (-2, -2), (-3, -3), (-4, -3), (-5, -2), (-5, -1), (-4, 0)] {
            fb.fill_rect(x + 10 + k * 3 + dx, base - 35 + dy, 2, 1, if k == 0 { dark } else { straw });
        }
    }
    fb.fill_rect(x - 8, base - 23, 2, 9, red);
    fb.fill_rect(x + 3, base - 23, 2, 9, red);
    fb.fill_rect(x + 7, base - 26, 4, 2, red);
}

/// The Christmas tree; returns where its lights go and which colour each has.
fn tree(fb: &mut Fb) -> Vec<(i32, i32, usize)> {
    pine(fb, TREE_X, TREE_BASE, TREE_H, rgb(24, 96, 50), rgb(215, 228, 248));
    let mut lights = Vec::new();
    for i in 0..4 {
        let apex = TREE_BASE - TREE_H + i as f32 * TREE_H * 0.2;
        let hw = TREE_H * (0.13 + i as f32 * 0.075);
        let n = 4 + i * 2;
        for k in 0..n {
            let s = (k as f32 + 0.5) / n as f32;
            let y = apex + TREE_H * (0.16 + 0.17 * s);
            // inside the tier at that height, along a garland that runs down to the right
            let half = hw * (y - apex) / (TREE_H * 0.36);
            let x = TREE_X - half * 0.85 + s * half * 1.7;
            lights.push((x as i32, y as i32, (k + i) % 4));
        }
    }
    lights
}

fn star(fb: &mut Fb, cx: f32, cy: f32, r: f32, c: u32) {
    for k in 0..5 {
        let a0 = -PI / 2.0 + k as f32 * TAU / 5.0;
        let outer = (cx + a0.cos() * r, cy + a0.sin() * r);
        for side in [-1.0, 1.0] {
            let ai = a0 + side * TAU / 10.0;
            fb.tri([(cx, cy), outer, (cx + ai.cos() * r * 0.42, cy + ai.sin() * r * 0.42)], c, 1.0);
        }
    }
}

fn disc(fb: &mut Fb, cx: f32, cy: f32, r: f32, c: u32, alpha: f32) {
    let ri = r.ceil() as i32;
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            if (dx * dx + dy * dy) as f32 <= r * r {
                fb.blend(cx as i32 + dx, cy as i32 + dy, c, alpha);
            }
        }
    }
}

impl Effect for Christmas {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (t, a) = (c.time, c.a);
        fb.px.copy_from_slice(&self.sky);
        for &(x, y, ph) in &self.stars {
            let k = 0.45 + 0.4 * (t * 2.0 + ph * 3.0).sin() + a.treble * 0.4;
            fb.blend(x, y, rgb(255, 255, 230), k.clamp(0.0, 1.0));
        }
        // northern lights, brighter with the mid tones
        for x in 0..WI {
            let fx = x as f32;
            let base = 78.0 + 12.0 * (fx * 0.018 + t * 0.25).sin() + 6.0 * (fx * 0.047 - t * 0.6).sin();
            let k = (0.35 + 0.65 * a.mid.min(1.0)) * (0.55 + 0.45 * (fx * 0.03 + t * 0.8).sin());
            for dy in 0..46 {
                let f = dy as f32 / 46.0;
                let al = k * (1.0 - f).powf(1.6) * 0.6;
                if al > 0.02 {
                    fb.blend(x, base as i32 - dy, mix(rgb(70, 255, 160), rgb(160, 90, 230), f), al);
                }
            }
        }
        for (p, &q) in fb.px.iter_mut().zip(&self.front.px) {
            if q != 0 {
                *p = q;
            }
        }
        // warm windows: a candle bridge in one, a paper star in the other
        for (i, &(wx, wy)) in WINDOWS.iter().enumerate() {
            let fl = (0.86 + 0.1 * ((t * 9.0 + i as f32 * 2.0).sin() * (t * 13.7).sin()) + a.pulse * 0.04).min(1.0);
            let glass = scale(rgb(255, 196, 90), fl);
            for (px, py) in [(1, 1), (9, 1), (1, 8), (9, 8)] {
                fb.fill_rect(wx + px, wy + py, 6, 5, glass);
            }
            for dy in 0..6 {
                fb.hspan_blend(wx - 2, wx + 18, wy + 15 + dy, rgb(255, 200, 110), 0.18 * (1.0 - dy as f32 / 6.0) * fl);
            }
            if i == 0 {
                fb.tri([(wx as f32 + 2.0, wy as f32 + 13.0), (wx as f32 + 14.0, wy as f32 + 13.0), (wx as f32 + 8.0, wy as f32 + 6.0)], rgb(70, 34, 20), 1.0);
                for k in 0..7 {
                    let s = k as f32 / 6.0;
                    let py = wy as f32 + 12.0 - 6.0 * (1.0 - (2.0 * s - 1.0).abs());
                    fb.put(wx + 2 + (12.0 * s) as i32, py as i32 - 1, rgb(255, 252, 210));
                }
            } else {
                star(fb, wx as f32 + 8.0, wy as f32 + 6.5, 4.5, scale(rgb(255, 130, 40), fl));
            }
        }
        // smoke from the chimney, a puff on every beat (or every half second)
        self.next_puff -= c.dt;
        if a.beat || self.next_puff <= 0.0 {
            self.smoke.push((CHIMNEY.0, CHIMNEY.1, 0.0));
            self.next_puff = 0.6;
        }
        for (x, y, age) in self.smoke.iter_mut() {
            *age += c.dt;
            *y -= 11.0 * c.dt;
            *x += (5.0 + a.bass * 12.0) * c.dt;
        }
        self.smoke.retain(|s| s.2 < 4.0);
        for &(x, y, age) in &self.smoke {
            disc(fb, x, y, 1.5 + age * 1.8, rgb(175, 182, 205), 0.3 * (1.0 - age / 4.0));
        }
        // the tree's lights take turns on the beat; the star flashes with it
        const COLS: [u32; 4] = [0xFF30_30FF, 0xFF30_D0FF, 0xFFFF_9040, 0xFFE0_F0FF];
        for (i, &(x, y, ci)) in self.lights.iter().enumerate() {
            let col = COLS[ci];
            if (i as u32 + c.beats) % 3 != 0 {
                for (dx, dy) in [(-1, 0), (2, 0), (0, -1), (0, 2), (1, -1), (1, 2), (-1, 1), (2, 1)] {
                    fb.blend(x + dx, y + dy, col, 0.25 + a.pulse * 0.35);
                }
                fb.fill_rect(x, y, 2, 2, col);
            } else {
                fb.fill_rect(x, y, 2, 2, scale(col, 0.3));
            }
        }
        let top = TREE_BASE - TREE_H - 3.0;
        let r = 6.0 + a.pulse * 3.0;
        for k in 0..8 {
            let ang = k as f32 * TAU / 8.0 + t * 0.4;
            for d in 0..(r * 1.8) as i32 {
                let al = (1.0 - d as f32 / (r * 1.8)) * (0.2 + a.pulse * 0.4);
                fb.blend((TREE_X + ang.cos() * d as f32) as i32, (top + ang.sin() * d as f32) as i32, rgb(255, 240, 160), al);
            }
        }
        star(fb, TREE_X, top, r, rgb(255, 214, 70));
        // big flakes in front of everything
        for (x, y, z) in self.flakes.iter_mut() {
            *y += c.dt * (16.0 + 22.0 * *z);
            *x += c.dt * ((*y * 0.04).sin() * 10.0 + a.bass * 20.0 * *z);
            if *y >= H as f32 {
                *y -= H as f32 + 3.0;
            }
            *x = x.rem_euclid(W as f32);
            let s = if *z > 0.8 { 3 } else { 2 };
            fb.fill_rect(*x as i32, *y as i32, s, s, rgb(250, 250, 255));
        }
        let w = 7.0 * 7.0 * 4.0;
        dancing_text(fb, "GOD JUL", (W as f32 - w) / 2.0, 10.0, 4.0, t, a, c.beats, &self.pal);
        let sub = "OCH GOTT NYTT ÅR FRÅN UFOSCENE.NET";
        let sw = font::text_width(sub, 1, false);
        font::text(fb, sub, (WI - sw) / 2, 48, 1, false, rgb(236, 236, 246));
    }
}
