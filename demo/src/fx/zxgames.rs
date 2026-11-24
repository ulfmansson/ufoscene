//! Effects: original ZX Spectrum-style game scenes in classic 8-bit genres.
//!
//! Every scene draws into a real Spectrum-shaped screen ([`Scr`]): a 256×192
//! one-bit bitmap plus 32×24 attribute cells (one ink, one paper, BRIGHT and
//! FLASH per 8×8 cell) shown inside a border in the machine's 15 colours.
//! Sprites set the ink of every cell they touch, so attribute clash comes for
//! free. The genres are classics; every room, level, character, picture and
//! line of text is new.

use std::f32::consts::{PI, TAU};

use super::*;
use crate::audio::Audio;
use crate::font;
use crate::zx::{SH, SW, X0, Y0, zx};

pub fn register(v: &mut Vec<Def>) {
    v.push(Def { name: "ISOMETRIC", flags: RETRO8 | LOGO_TOP, make: |s| Box::new(Isometric::new(s)) });
    v.push(Def { name: "LADDERS", flags: RETRO8, make: |s| Box::new(Ladders::new(s)) });
    v.push(Def { name: "MAZE RAID", flags: RETRO8, make: |s| Box::new(MazeRaid::new(s)) });
    v.push(Def { name: "SPACE SHOOTER", flags: RETRO8, make: |s| Box::new(Shooter::new(s)) });
    v.push(Def { name: "TAPE LOADER", flags: RETRO8 | OWN_TEXT, make: |s| Box::new(TapeLoader::new(s)) });
    v.push(Def { name: "ADVENTURE", flags: RETRO8 | OWN_TEXT, make: |s| Box::new(Adventure::new(s)) });
    v.push(Def { name: "ROAD RACER", flags: RETRO8, make: |s| Box::new(RoadRacer::new(s)) });
    v.push(Def { name: "WAR MAP", flags: RETRO8, make: |s| Box::new(WarMap::new(s)) });
    v.push(Def { name: "PANORAMA", flags: RETRO8 | OWN_TEXT, make: |s| Box::new(Panorama::new(s)) });
}

// ================================================================ the Spectrum screen

const BLACK: u8 = 0;
const BLUE: u8 = 1;
const RED: u8 = 2;
const MAGENTA: u8 = 3;
const GREEN: u8 = 4;
const CYAN: u8 = 5;
const YELLOW: u8 = 6;
const WHITE: u8 = 7;
/// Add to a colour for its BRIGHT version.
const BR: u8 = 8;
/// Attribute bit: ink and paper swap on every other beat.
const FLASH: u8 = 0x80;

/// An attribute byte. BRIGHT is per cell, so it is on if either colour asks for it.
const fn at(ink: u8, paper: u8) -> u8 {
    (ink & 7) | ((paper & 7) << 3) | if (ink | paper) & BR != 0 { 0x40 } else { 0 }
}

#[inline]
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

/// The Spectrum colour (bright, never black or white) closest to the demo's
/// accent, so the same game wears different colours in different demos.
fn accent(pal: &Palette) -> u8 {
    let (r, g, b) = unpack(pal.hi);
    let m = r.max(g).max(b) * 0.8;
    let i = (b >= m) as u8 | ((r >= m) as u8) << 1 | ((g >= m) as u8) << 2;
    if i == 0 || i == 7 { CYAN | BR } else { i | BR }
}

/// Beat position: whole beats from the demo plus our own phase inside the
/// beat, so movement stays smooth even when the host only counts beats.
#[derive(Default)]
struct Clock {
    last: Option<u32>,
    frac: f32,
}

impl Clock {
    /// Returns the beat position and whether a new beat started this frame.
    fn tick(&mut self, c: &Ctx) -> (f32, bool) {
        let first = self.last.is_none();
        let new = self.last != Some(c.beats);
        if new {
            self.frac = 0.0;
        } else {
            self.frac = (self.frac + c.dt / c.a.period.max(0.1)).min(0.999);
        }
        self.last = Some(c.beats);
        (c.beats as f32 + self.frac, new && !first)
    }
}

/// Paint the border around the 256×192 screen, one colour per scanline.
fn border(fb: &mut Fb, mut f: impl FnMut(i32) -> u32) {
    for y in 0..HI {
        let c = f(y);
        if y < Y0 || y >= Y0 + SH {
            fb.row(y, c);
        } else {
            fb.hspan(0, X0, y, c);
            fb.hspan(X0 + SW, WI, y, c);
        }
    }
}

/// A one-bit sprite up to 30 pixels wide, with a one-pixel outline mask.
struct Spr {
    w: i32,
    h: i32,
    rows: Vec<u32>,
    /// (w+2)×(h+2): the sprite grown by one pixel; cleared before drawing.
    mask: Vec<u32>,
}

/// Build a sprite from rows of `X` (ink) and `.` (transparent).
fn spr(art: &[&str]) -> Spr {
    let h = art.len() as i32;
    let w = art[0].len() as i32;
    assert!(w <= 30 && art.iter().all(|r| r.len() as i32 == w), "sprite rows differ: {art:?}");
    let rows: Vec<u32> = art.iter().map(|r| r.bytes().fold(0, |acc, b| (acc << 1) | (b == b'X') as u32)).collect();
    let bit = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h && (rows[y as usize] >> (w - 1 - x)) & 1 == 1;
    let mask = (0..h + 2)
        .map(|my| {
            (0..w + 2).fold(0u32, |acc, mx| {
                let (x, y) = (mx - 1, my - 1);
                let on = bit(x, y) || bit(x - 1, y) || bit(x + 1, y) || bit(x, y - 1) || bit(x, y + 1);
                (acc << 1) | on as u32
            })
        })
        .collect();
    Spr { w, h, rows, mask }
}

/// 256×192 bitmap (row-major, 32 bytes per row) and 32×24 attributes.
struct Scr {
    bits: Vec<u8>,
    attr: Vec<u8>,
}

impl Scr {
    fn new() -> Self {
        Scr { bits: vec![0; 6144], attr: vec![at(WHITE, BLACK); 768] }
    }

    fn clear(&mut self, a: u8) {
        self.bits.fill(0);
        self.attr.fill(a);
    }

    #[inline]
    fn set(&mut self, x: i32, y: i32, on: bool) {
        if x >= 0 && y >= 0 && x < SW && y < SH {
            let i = (y * 32 + (x >> 3)) as usize;
            let m = 0x80u8 >> (x & 7);
            if on {
                self.bits[i] |= m;
            } else {
                self.bits[i] &= !m;
            }
        }
    }

    #[inline]
    fn plot(&mut self, x: i32, y: i32) {
        self.set(x, y, true);
    }

    /// Rectangle filled with a pattern (overwrites).
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, pat: impl Fn(i32, i32) -> bool) {
        for yy in y.max(0)..(y + h).min(SH) {
            for xx in x.max(0)..(x + w).min(SW) {
                self.set(xx, yy, pat(xx, yy));
            }
        }
    }

    /// A one-pixel rectangle outline (adds pixels).
    fn frame(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.ln(x, y, x + w - 1, y);
        self.ln(x, y + h - 1, x + w - 1, y + h - 1);
        self.ln(x, y, x, y + h - 1);
        self.ln(x + w - 1, y, x + w - 1, y + h - 1);
    }

    /// Clear whole cell rows `r0..r1` of pixels.
    fn clear_rows(&mut self, r0: i32, r1: i32) {
        self.bits[(r0 * 256) as usize..(r1 * 256) as usize].fill(0);
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), dotted: bool) {
        let (mut x0, mut y0) = (a.0.round() as i32, a.1.round() as i32);
        let (x1, y1) = (b.0.round() as i32, b.1.round() as i32);
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let mut err = dx + dy;
        let mut n = 0;
        loop {
            if !dotted || (x0 + y0) % 2 == 0 {
                self.plot(x0, y0);
            }
            n += 1;
            if (x0 == x1 && y0 == y1) || n > 1200 {
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

    fn ln(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.line((x0 as f32, y0 as f32), (x1 as f32, y1 as f32), false);
    }

    /// Fill a polygon with a pattern. `or` only adds pixels; otherwise the
    /// pattern overwrites (so `|_, _| false` cuts a hole).
    fn poly(&mut self, p: &[(f32, f32)], or: bool, pat: impl Fn(i32, i32) -> bool) {
        let (mut y0, mut y1) = (f32::MAX, f32::MIN);
        for q in p {
            y0 = y0.min(q.1);
            y1 = y1.max(q.1);
        }
        let mut xs = [0f32; 8];
        for y in (y0.floor() as i32).max(0)..(y1.ceil() as i32 + 1).min(SH) {
            let yc = y as f32 + 0.5;
            let mut n = 0;
            for i in 0..p.len() {
                let (a, b) = (p[i], p[(i + 1) % p.len()]);
                if (a.1 <= yc) != (b.1 <= yc) && n < 8 {
                    xs[n] = a.0 + (yc - a.1) * (b.0 - a.0) / (b.1 - a.1);
                    n += 1;
                }
            }
            xs[..n].sort_by(|a, b| a.total_cmp(b));
            for k in (0..n / 2).map(|k| k * 2) {
                let (xa, xb) = ((xs[k] - 0.5).ceil() as i32, (xs[k + 1] - 0.5).ceil() as i32);
                for x in xa.max(0)..xb.min(SW) {
                    let on = pat(x, y);
                    if on || !or {
                        self.set(x, y, on);
                    }
                }
            }
        }
    }

    /// Filled ellipse with a pattern (overwrites).
    fn oval(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, pat: impl Fn(i32, i32) -> bool) {
        for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
            let dy = (y as f32 + 0.5 - cy) / ry;
            if dy.abs() >= 1.0 {
                continue;
            }
            let half = rx * (1.0 - dy * dy).sqrt();
            for x in (cx - half).round() as i32..(cx + half).round() as i32 {
                self.set(x, y, pat(x, y));
            }
        }
    }

    /// Write the eight pixel rows of one cell.
    fn tile(&mut self, cx: i32, cy: i32, rows: &[u8; 8]) {
        if (0..32).contains(&cx) && (0..24).contains(&cy) {
            for (r, &b) in rows.iter().enumerate() {
                self.bits[((cy * 8 + r as i32) * 32 + cx) as usize] = b;
            }
        }
    }

    fn cell(&mut self, cx: i32, cy: i32, a: u8) {
        if (0..32).contains(&cx) && (0..24).contains(&cy) {
            self.attr[(cy * 32 + cx) as usize] = a;
        }
    }

    fn cells(&mut self, cx: i32, cy: i32, w: i32, h: i32, a: u8) {
        for y in cy..cy + h {
            for x in cx..cx + w {
                self.cell(x, y, a);
            }
        }
    }

    /// Apply `f` to the attribute of every cell under a pixel rectangle.
    fn each_cell(&mut self, x: i32, y: i32, w: i32, h: i32, f: impl Fn(u8) -> u8) {
        if x + w <= 0 || y + h <= 0 || x >= SW || y >= SH || w <= 0 || h <= 0 {
            return;
        }
        for r in (y.max(0) >> 3)..=((y + h - 1).min(SH - 1) >> 3) {
            for c in (x.max(0) >> 3)..=((x + w - 1).min(SW - 1) >> 3) {
                let a = &mut self.attr[(r * 32 + c) as usize];
                *a = f(*a);
            }
        }
    }

    /// Set the ink (and BRIGHT) of every cell under a pixel rectangle: attribute clash.
    fn ink(&mut self, x: i32, y: i32, w: i32, h: i32, ink: u8) {
        let br = if ink & BR != 0 { 0x40 } else { 0 };
        self.each_cell(x, y, w, h, |a| (a & 0b1011_1000) | (ink & 7) | br);
    }

    /// Change only the ink of the cells under a pixel rectangle, keeping BRIGHT.
    fn tint(&mut self, x: i32, y: i32, w: i32, h: i32, ink: u8) {
        self.each_cell(x, y, w, h, |a| (a & !7) | (ink & 7));
    }

    /// Text on the character grid in the chunky bold font.
    fn print(&mut self, col: i32, row: i32, s: &str, a: Option<u8>) {
        for (i, ch) in s.chars().enumerate() {
            let cx = col + i as i32;
            if !(0..32).contains(&cx) || !(0..24).contains(&row) {
                continue;
            }
            for gy in 0..8 {
                let mut b = 0u8;
                for gx in 0..6 {
                    if font::bit(ch, gx, gy, true) {
                        b |= 0x40 >> gx;
                    }
                }
                self.bits[((row * 8 + gy) * 32 + cx) as usize] = b;
            }
            if let Some(a) = a {
                self.cell(cx, row, a);
            }
        }
    }

    /// A zero-padded number without allocating.
    fn num(&mut self, col: i32, row: i32, n: u32, digits: usize, a: Option<u8>) {
        let mut buf = [b'0'; 10];
        let mut n = n;
        let d = digits.min(10);
        for i in (0..d).rev() {
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        self.print(col, row, std::str::from_utf8(&buf[..d]).unwrap_or(""), a);
    }

    /// Draw a sprite: clear its outline, then set its pixels.
    fn draw(&mut self, s: &Spr, x: i32, y: i32, mirror: bool) {
        for my in 0..s.h + 2 {
            let r = s.mask[my as usize];
            for mx in 0..s.w + 2 {
                if (r >> (s.w + 1 - mx)) & 1 == 1 {
                    let px = if mirror { s.w + 1 - mx } else { mx };
                    self.set(x - 1 + px, y - 1 + my, false);
                }
            }
        }
        for sy in 0..s.h {
            let r = s.rows[sy as usize];
            for sx in 0..s.w {
                if (r >> (s.w - 1 - sx)) & 1 == 1 {
                    let px = if mirror { s.w - 1 - sx } else { sx };
                    self.set(x + px, y + sy, true);
                }
            }
        }
    }

    /// Draw a sprite and colour the cells it touches.
    fn sprite(&mut self, s: &Spr, x: i32, y: i32, mirror: bool, ink: u8) {
        self.draw(s, x, y, mirror);
        self.ink(x, y, s.w, s.h, ink);
    }

    /// An explosion: sparks flying out, `age` 0..1.
    fn burst(&mut self, cx: f32, cy: f32, age: f32, seed: u32) {
        let r = 2.0 + age * 13.0;
        for k in 0..16u32 {
            let h = hash(seed.wrapping_mul(31) + k);
            let ang = k as f32 / 16.0 * TAU + (h & 255) as f32 * 0.004;
            let rr = r * (0.55 + ((h >> 8) & 255) as f32 / 560.0);
            let (x, y) = ((cx + ang.cos() * rr) as i32, (cy + ang.sin() * rr) as i32);
            self.plot(x, y);
            self.plot(x + 1, y);
            if age < 0.6 {
                self.plot((cx + ang.cos() * rr * 0.5) as i32, (cy + ang.sin() * rr * 0.5) as i32);
            }
        }
        if age < 0.3 {
            let k = 5.0 * (1.0 - age / 0.3);
            self.oval(cx, cy, k, k, |x, y| (x + y) % 2 == 0);
        }
    }

    /// Show the screen: border first, then 8 pixels per attribute byte.
    fn render(&self, fb: &mut Fb, flash_on: bool) {
        let mut cols = [0u32; 16];
        for (i, c) in cols.iter_mut().enumerate() {
            *c = zx(i as u8);
        }
        for y in 0..SH as usize {
            let row = (Y0 as usize + y) * W + X0 as usize;
            let arow = (y >> 3) * 32;
            for cx in 0..32 {
                let a = self.attr[arow + cx];
                let br = ((a >> 6) & 1) * 8;
                let (mut ink, mut paper) = (cols[((a & 7) | br) as usize], cols[(((a >> 3) & 7) | br) as usize]);
                if a & FLASH != 0 && flash_on {
                    std::mem::swap(&mut ink, &mut paper);
                }
                let b = self.bits[y * 32 + cx];
                let o = row + cx * 8;
                for i in 0..8 {
                    fb.px[o + i] = if b & (0x80 >> i) != 0 { ink } else { paper };
                }
            }
        }
    }
}

const BAR_FULL: [u8; 8] = [0, 0x7E, 0x7E, 0x7E, 0x7E, 0x7E, 0x7E, 0];
const BAR_EMPTY: [u8; 8] = [0, 0, 0, 0x18, 0x18, 0, 0, 0];

/// A HUD meter: `n` of `len` cells lit, green → yellow → red.
fn meter(s: &mut Scr, col: i32, row: i32, len: i32, level: f32) {
    let n = (level.clamp(0.0, 1.0) * len as f32).round() as i32;
    for i in 0..len {
        s.tile(col + i, row, if i < n { &BAR_FULL } else { &BAR_EMPTY });
        let ink = if i * 3 < len * 2 { GREEN } else if i * 6 < len * 5 { YELLOW } else { RED };
        s.cell(col + i, row, at(ink | BR, BLACK));
    }
}

// ================================================================ sprites

#[rustfmt::skip]
mod art {
    pub const ROBOT: [&str; 16] = [
        ".........XX.....",
        ".........X......",
        ".....XXXXXX.....",
        "....X......X....",
        "...X...X..X.X...",
        "...X...X..X.X...",
        "...X........X...",
        "....XXXXXXXX....",
        "......X..X......",
        "..XXXXXXXXXXXX..",
        ".X.X........X.X.",
        ".X.X.XXXXXX.X.X.",
        ".X.X.X.XX.X.X.X.",
        "...XXXXXXXXXX...",
        "....XX....XX....",
        "...XXX....XXX...",
    ];
    pub const ROBOT_HOP: [&str; 16] = [
        ".........XX.....",
        ".........X......",
        ".....XXXXXX.....",
        "....X......X....",
        "...X...X..X.X...",
        "...X...X..X.X...",
        "...X........X...",
        "X...XXXXXXXX...X",
        "X.....X..X.....X",
        ".XXXXXXXXXXXXXX.",
        "...X........X...",
        "...X.XXXXXX.X...",
        "...X.X.XX.X.X...",
        "...XXXXXXXXXX...",
        "...XX......XX...",
        "..XX........XX..",
    ];
    pub const ROBOT_BACK: [&str; 16] = [
        ".....XX.........",
        "......X.........",
        ".....XXXXXX.....",
        "....X......X....",
        "...X........X...",
        "...X.X.X.X..X...",
        "...X........X...",
        "....XXXXXXXX....",
        "......X..X......",
        "..XXXXXXXXXXXX..",
        ".X.X........X.X.",
        ".X.X.X....X.X.X.",
        ".X.X.X....X.X.X.",
        "...XXXXXXXXXX...",
        "....XX....XX....",
        "...XXX....XXX...",
    ];
    pub const GEM: [&str; 8] = [
        "........",
        ".XXXXXX.",
        "X.X..X.X",
        "XXXXXXXX",
        ".X.X..X.",
        "..X.XX..",
        "...XX...",
        "........",
    ];
    pub const DISK: [&str; 8] = [
        "XXXXXXX.",
        "X.XXX.XX",
        "X.XXX..X",
        "X......X",
        "X.XXXX.X",
        "X.X..X.X",
        "X.XXXX.X",
        "XXXXXXXX",
    ];
    pub const MUG: [&str; 8] = [
        ".X..X...",
        "..X..X..",
        "XXXXX...",
        "X...XXX.",
        "X...X.X.",
        "X...XXX.",
        "X...X...",
        ".XXX....",
    ];
    pub const BATTERY: [&str; 8] = [
        "...XX...",
        ".XXXXXX.",
        ".X....X.",
        ".X.XX.X.",
        ".X.XX.X.",
        ".X....X.",
        ".XXXXXX.",
        "........",
    ];
    pub const SPARKLE: [&str; 8] = [
        "...X....",
        "...X....",
        ".X.X.X..",
        "..XXX...",
        "XXX.XXX.",
        "..XXX...",
        ".X.X.X..",
        "...X....",
    ];
    pub const ASTRO: [&str; 16] = [
        ".....XXXXX......",
        "....X.....X.....",
        "...X...XXX.X....",
        "...X..XXXX.X....",
        "...X...XXX.X....",
        "....X.....X.....",
        "..XXXXXXXXXX....",
        ".XX.X......X....",
        ".XX.X..XXX.XX...",
        ".XX.X......X.X..",
        ".XX.XXXXXXXX....",
        "....X......X....",
        "....XX....XX....",
        "...XX......XX...",
        "..XX........XX..",
        ".XXX........XXX.",
    ];
    pub const ASTRO_B: [&str; 16] = [
        ".....XXXXX......",
        "....X.....X.....",
        "...X...XXX.X....",
        "...X..XXXX.X....",
        "...X...XXX.X....",
        "....X.....X.....",
        "..XXXXXXXXXX....",
        ".XX.X......X....",
        ".XX.X..XXX.XX...",
        ".XX.X......X.X..",
        ".XX.XXXXXXXX....",
        "....X......X....",
        ".....XX.XX......",
        ".....XX.XX......",
        ".....XX.XX......",
        "....XXX.XXX.....",
    ];
    pub const ASTRO_CLIMB: [&str; 16] = [
        ".....XXXXXX.....",
        "....X......X....",
        ".X.X........X...",
        ".X.X........X...",
        ".X..X......X....",
        ".XXXXXXXXXXXX...",
        "....XXXXXXXX.X..",
        "....X.XXXX.X.X..",
        "....X.X..X.X.X..",
        "....X.XXXX.XXX..",
        "....XXXXXXXX....",
        "....X......X....",
        "....X......XX...",
        "...XX.......X...",
        "...X........XX..",
        "...XX...........",
    ];
    pub const JELLY: [&str; 12] = [
        "....XXXXXXXX....",
        "..XX........XX..",
        ".X...XX..XX...X.",
        ".X...XX..XX...X.",
        ".X............X.",
        "..XXXXXXXXXXXX..",
        "..X.X..XX..X.X..",
        ".X..X..XX..X..X.",
        ".X...X.XX.X...X.",
        "X....X....X....X",
        "X...X......X...X",
        "................",
    ];
    pub const JELLY_B: [&str; 12] = [
        "....XXXXXXXX....",
        "..XX........XX..",
        ".X...XX..XX...X.",
        ".X...XX..XX...X.",
        ".X............X.",
        "..XXXXXXXXXXXX..",
        "..X.X..XX..X.X..",
        "..X..X.XX.X..X..",
        "...X.X.XX.X.X...",
        "...X..X..X..X...",
        "..X...X..X...X..",
        "..X..........X..",
    ];
    pub const CRAWLER: [&str; 10] = [
        "..X..........X..",
        "...X........X...",
        "....XXXXXXXX....",
        "..XXX.XXXX.XXX..",
        ".XXXX.XXXX.XXXX.",
        ".XXXXXXXXXXXXXX.",
        "..XXXXXXXXXXXX..",
        ".X.X.X....X.X.X.",
        "X..X..X..X..X..X",
        "................",
    ];
    pub const CRAWLER_B: [&str; 10] = [
        "...X........X...",
        "...X........X...",
        "....XXXXXXXX....",
        "..XXX.XXXX.XXX..",
        ".XXXX.XXXX.XXXX.",
        ".XXXXXXXXXXXXXX.",
        "..XXXXXXXXXXXX..",
        "..X.X.X..X.X.X..",
        ".X..X..XX..X..X.",
        "................",
    ];
    pub const SPINNER: [&str; 7] = [
        ".....XXXXXX.....",
        "...XX......XX...",
        "XXXXXXXXXXXXXXXX",
        "X.XX.XX.XX.XX.XX",
        "XXXXXXXXXXXXXXXX",
        "...XX......XX...",
        ".....XXXXXX.....",
    ];
    pub const SPINNER_B: [&str; 7] = [
        ".....XXXXXX.....",
        "...XX......XX...",
        "XXXXXXXXXXXXXXXX",
        ".XX.XX.XX.XX.XX.",
        "XXXXXXXXXXXXXXXX",
        "...XX......XX...",
        ".....XXXXXX.....",
    ];
    pub const FUEL: [&str; 8] = [
        "...XX...",
        ".XXXXXX.",
        ".X....X.",
        ".X.XX.X.",
        ".X.XX.X.",
        ".X....X.",
        ".XXXXXX.",
        "........",
    ];
    pub const HERO: [&str; 16] = [
        "......XXXX......",
        ".....XXXXXX.....",
        "....XXXXXXXX....",
        "....XX....XXX...",
        "....X.X..X.XX...",
        "....X......XX...",
        ".....X....XXX...",
        "....XXXXXXXXXX..",
        "...XXX.XX.XX.XXX",
        "..X.XX.XX.XX..XX",
        "..X.XXXXXXXX...X",
        "....XXXXXXXX....",
        "....XXX..XXX....",
        "....XX....XX....",
        "...XXX....XXX...",
        "...XXX.....XXX..",
    ];
    pub const HERO_B: [&str; 16] = [
        "......XXXX......",
        ".....XXXXXX.....",
        "....XXXXXXXX....",
        "....XX....XXX...",
        "....X.X..X.XX...",
        "....X......XX...",
        ".....X....XXX...",
        "....XXXXXXXXXX..",
        "...XXX.XX.XX.XXX",
        "..X.XX.XX.XX..XX",
        "..X.XXXXXXXX...X",
        "....XXXXXXXX....",
        ".....XXXXXX.....",
        ".....XX..XX.....",
        ".....XX..XX.....",
        "....XXX..XXX....",
    ];
    pub const TROLL: [&str; 16] = [
        "....XXXXXXX.....",
        "...X.......X....",
        "..X..XX.XX..X...",
        "..X..XX.XX..X...",
        "..X....X....X...",
        "..X.X.....X.X...",
        "...X.XXXXX.X....",
        "..XXXXXXXXXXX...",
        ".XXXXXXXXXXXXX.X",
        "X.XXXXXXXXXXX.XX",
        "X.XXXXXXXXXXX.X.",
        "XX.XXXXXXXXX..X.",
        "...XXXX.XXXX....",
        "...XXX...XXX....",
        "..XXXX...XXXX...",
        ".XXXXX...XXXXX..",
    ];
    pub const TROLL_B: [&str; 16] = [
        "....XXXXXXX.....",
        "...X.......X....",
        "..X..XX.XX..X...",
        "..X..XX.XX..X...",
        "..X....X....X...",
        "..X.X.....X.X...",
        "...X.XXXXX.X....",
        "..XXXXXXXXXXX...",
        ".XXXXXXXXXXXXX.X",
        "X.XXXXXXXXXXX.XX",
        "X.XXXXXXXXXXX.X.",
        "XX.XXXXXXXXX..X.",
        "...XXXXXXXXX....",
        "....XXX.XXX.....",
        "....XXX.XXX.....",
        "...XXXX.XXXX....",
    ];
    pub const KEY: [&str; 6] = [
        ".XXX....",
        "X...X...",
        "X...XXXX",
        "X...X.XX",
        ".XXX..X.",
        "........",
    ];
    pub const BOLT: [&str; 4] = [
        ".XX.",
        "XXXX",
        "XXXX",
        ".XX.",
    ];
    pub const TORCH: [&str; 8] = [
        "...X....",
        "..XX....",
        "..XXX...",
        ".XXXX...",
        ".XX.XX..",
        "..XXX...",
        "...X....",
        "..XXX...",
    ];
    pub const TORCH_B: [&str; 8] = [
        "....X...",
        "...XX...",
        "..XXX...",
        "..XXXX..",
        ".XX.XX..",
        "..XXX...",
        "...X....",
        "..XXX...",
    ];
    pub const SHIP: [&str; 8] = [
        "..XX............",
        "..XXXX..........",
        ".XXXXXXXXXX.....",
        "XXXX.X.XXXXXXX..",
        "XXXX.X.XXXXXXXXX",
        ".XXXXXXXXXX.....",
        "..XXXX..........",
        "..XX............",
    ];
    pub const DARTER: [&str; 6] = [
        "....XXXX....",
        "..XXXXXXXX..",
        ".XX.XXXX.XX.",
        "XXXXXXXXXXXX",
        "..X..XX..X..",
        ".X........X.",
    ];
    pub const DARTER_B: [&str; 6] = [
        "....XXXX....",
        "..XXXXXXXX..",
        ".XX.XXXX.XX.",
        "XXXXXXXXXXXX",
        ".X...XX...X.",
        "..X......X..",
    ];
    pub const ORB: [&str; 7] = [
        "...XXXXX...",
        ".XX.....XX.",
        "X..XXXXX..X",
        "X.XX.X.XX.X",
        "X..XXXXX..X",
        ".XX.....XX.",
        "...XXXXX...",
    ];
    pub const ORB_B: [&str; 7] = [
        "...XXXXX...",
        ".XX.....XX.",
        "X..XXXXX..X",
        "X.X.XXX.X.X",
        "X..XXXXX..X",
        ".XX.....XX.",
        "...XXXXX...",
    ];
    pub const SQUID: [&str; 9] = [
        "....XXX....",
        "..XXXXXXX..",
        ".XX.XXX.XX.",
        ".XXXXXXXXX.",
        "..XXXXXXX..",
        ".X.X.X.X.X.",
        "X..X.X.X..X",
        "X.X..X..X.X",
        "..X.....X..",
    ];
    pub const SQUID_B: [&str; 9] = [
        "....XXX....",
        "..XXXXXXX..",
        ".XX.XXX.XX.",
        ".XXXXXXXXX.",
        "..XXXXXXX..",
        ".X.X.X.X.X.",
        ".X.X.X.X.X.",
        "..X..X..X..",
        ".X...X...X.",
    ];
    pub const MINI_SHIP: [&str; 5] = [
        "XX......",
        "XXXXX...",
        "XXXXXXXX",
        "XXXXX...",
        "XX......",
    ];
    pub const RACER: [&str; 20] = [
        "...XXXXXXXXXX...",
        "...XXXXXXXXXX...",
        "......XXXX......",
        ".XX...XXXX...XX.",
        ".XX..XXXXXX..XX.",
        ".XXXXXXXXXXXXXX.",
        ".XX..XXXXXX..XX.",
        ".XX..XX..XX..XX.",
        ".....X....X.....",
        ".....X.XX.X.....",
        ".....X.XX.X.....",
        ".....XX..XX.....",
        "....XXXXXXXX....",
        ".XX.XXXXXXXX.XX.",
        ".XXXXXXXXXXXXXX.",
        ".XX.XXXXXXXX.XX.",
        ".XX..XXXXXX..XX.",
        "....XXXXXXXX....",
        "...XXXXXXXXXX...",
        "...X.X.XX.X.X...",
    ];
    pub const SALOON: [&str; 18] = [
        "....XXXXXXXX....",
        "...XXXXXXXXXX...",
        "..XX.XXXXXX.XX..",
        ".XXXXXXXXXXXXXX.",
        ".XXX........XXX.",
        ".XX..........XX.",
        ".XX.XXXXXXXX.XX.",
        "XXX.XXXXXXXX.XXX",
        "XXX.XXXXXXXX.XXX",
        ".XX.XXXXXXXX.XX.",
        ".XX.XXXXXXXX.XX.",
        ".XX..........XX.",
        ".XXX........XXX.",
        "XXXXXXXXXXXXXXXX",
        "XXX.XXXXXXXX.XXX",
        ".XXXXXXXXXXXXXX.",
        "..XX.XXXXXX.XX..",
        "...XXXXXXXXXX...",
    ];
    pub const PALM: [&str; 8] = [
        ".XX..XX.",
        "X..XX..X",
        "..XXXX..",
        ".X.XX.X.",
        "...X....",
        "...X....",
        "...X....",
        "..XXX...",
    ];
    pub const BANNER: [&str; 8] = [
        ".XXXXXXX........",
        "XXXXXXXX........",
        ".XXXXXXX........",
        "...XXXXX........",
        ".......X........",
        ".......X........",
        ".......X........",
        ".......X........",
    ];
    pub const BANNER_B: [&str; 8] = [
        "...XXXXX........",
        ".XXXXXXX........",
        "XXXXXXXX........",
        ".XX.XXXX........",
        ".......X........",
        ".......X........",
        ".......X........",
        ".......X........",
    ];
    pub const RIDER: [&str; 16] = [
        "......XXX.......",
        "......XXX.......",
        ".......X........",
        ".....XXXX.......",
        ".....XXXX....XX.",
        "....XXXXX...XXXX",
        "..XXXXXXXXXXXX.X",
        ".XXXXXXXXXXXX...",
        "X.XXXXXXXXXXX...",
        "X.XXXXXXXXXX....",
        "..XXXXXXXXXX....",
        "..X.X....X.X....",
        ".X...X...X..X...",
        ".X...X..X...X...",
        "X.....X.X....X..",
        "................",
    ];
    pub const RIDER_B: [&str; 16] = [
        "......XXX.......",
        "......XXX.......",
        ".......X........",
        ".....XXXX.......",
        ".....XXXX....XX.",
        "....XXXXX...XXXX",
        "..XXXXXXXXXXXX.X",
        ".XXXXXXXXXXXX...",
        "X.XXXXXXXXXXX...",
        "X.XXXXXXXXXX....",
        "..XXXXXXXXXX....",
        "..X.X....X.X....",
        "...XX....XX.....",
        "...X.X...X.X....",
        "..X...X.X...X...",
        "................",
    ];
    pub const SMALL_UFO: [&str; 6] = [
        "......XXXX......",
        ".....X.XX.X.....",
        "..XXXXXXXXXXXX..",
        "XXX.XX.XX.XX.XXX",
        "..XXXXXXXXXXXX..",
        "....X......X....",
    ];
}

// ================================================================ 1. ISOMETRIC

/// Stacks of blocks per room: (u, v, height in blocks).
const ISO_ROOMS: [&[(i32, i32, i32)]; 4] = [
    &[(0, 6, 2), (1, 7, 1), (6, 0, 3), (5, 5, 1), (2, 2, 1)],
    &[(3, 0, 2), (0, 2, 1), (7, 5, 1), (4, 6, 2), (6, 2, 1)],
    &[(0, 0, 3), (7, 0, 1), (0, 7, 1), (3, 4, 1), (5, 2, 2)],
    &[(2, 6, 1), (6, 6, 2), (1, 1, 2), (6, 3, 1), (3, 0, 1)],
];
/// Per room: the bouncing platform's tile, where the door is on the left wall
/// and where the window is on the right wall.
const ISO_PLATFORM: [(i32, i32); 4] = [(4, 3), (2, 5), (4, 6), (4, 2)];
const ISO_DOOR: [f32; 4] = [2.5, 4.5, 3.0, 4.0];
const ISO_WINDOW: [f32; 4] = [2.0, 5.0, 3.0, 5.0];
const ISO_INKS: [u8; 5] = [CYAN | BR, GREEN | BR, MAGENTA | BR, YELLOW | BR, RED | BR];
const ITEM_INK: [u8; 4] = [YELLOW | BR, MAGENTA | BR, GREEN | BR, RED | BR];

/// 2:1 isometric projection; a block is 24 pixels wide and 12 tall.
fn iso(u: f32, v: f32, z: f32) -> (f32, f32) {
    (128.0 + (u - v) * 12.0, 60.0 + (u + v) * 6.0 - z * 12.0)
}

/// An isometric block: cleared silhouette, three shaded faces, crisp edges.
fn iso_block(s: &mut Scr, u: f32, v: f32, z: f32, h: f32, top: u8) {
    let p = |du: f32, dv: f32, dz: f32| iso(u + du, v + dv, z + dz);
    let (a, b, c, d) = (p(0.0, 0.0, h), p(1.0, 0.0, h), p(1.0, 1.0, h), p(0.0, 1.0, h));
    let (b0, c0, d0) = (p(1.0, 0.0, 0.0), p(1.0, 1.0, 0.0), p(0.0, 1.0, 0.0));
    s.poly(&[a, b, b0, c0, d0, d], false, |_, _| false);
    s.poly(&[b, c, c0, b0], false, |_, y| y % 2 == 0);
    s.poly(&[d, c, c0, d0], false, |x, y| (x + y) % 2 == 0 && y % 4 != 1);
    match top {
        1 => s.poly(&[a, b, c, d], false, |x, y| x % 4 == 0 && y % 2 == 0),
        _ => s.poly(&[a, b, c, d], false, |x, y| (x + 2 * y) % 6 == 0 || (x - 2 * y).rem_euclid(6) == 0),
    }
    for (e0, e1) in [(a, b), (b, c), (c, d), (d, a), (b, b0), (c, c0), (d, d0), (b0, c0), (c0, d0)] {
        s.line(e0, e1, false);
    }
}

/// Two back walls (bricks or panels), a door, a window and a tiled floor slab.
fn iso_room(s: &mut Scr, style: u32, door: f32, window: f32) {
    for wall in 0..2 {
        let p = |t: f32, z: f32| if wall == 0 { iso(t, 0.0, z) } else { iso(0.0, t, z) };
        if style == 0 {
            for k in 1..6 {
                let z = k as f32 * 0.5;
                s.line(p(0.0, z), p(8.0, z), false);
            }
            for k in 0..6 {
                let z = k as f32 * 0.5;
                let off = (k % 2) as f32 * 0.5;
                for j in 0..9 {
                    let t = j as f32 + off;
                    if t > 0.0 && t < 8.0 {
                        s.line(p(t, z), p(t, z + 0.5), false);
                    }
                }
            }
        } else {
            s.poly(&[p(0.0, 0.4), p(8.0, 0.4), p(8.0, 2.5), p(0.0, 2.5)], false, |x, y| x % 4 == 0 && y % 4 == 0);
            for z in [0.4f32, 2.5, 2.7] {
                s.line(p(0.0, z), p(8.0, z), false);
            }
            for j in 1..8 {
                let t = j as f32;
                s.poly(&[p(t - 0.12, 0.4), p(t + 0.12, 0.4), p(t + 0.12, 2.5), p(t - 0.12, 2.5)], false, |_, _| false);
                s.line(p(t - 0.12, 0.4), p(t - 0.12, 2.5), false);
                s.line(p(t + 0.12, 0.4), p(t + 0.12, 2.5), false);
            }
        }
        s.line(p(0.0, 3.0), p(8.0, 3.0), false);
        s.line(p(8.0, 0.0), p(8.0, 3.0), false);
    }
    s.line(iso(0.0, 0.0, 0.0), iso(0.0, 0.0, 3.0), false);
    // an arched door in the left wall
    let d = |t: f32, z: f32| iso(0.0, door + t, z);
    s.poly(&[d(0.0, 0.0), d(0.0, 1.5), d(0.3, 1.8), d(1.1, 1.8), d(1.4, 1.5), d(1.4, 0.0)], false, |_, _| false);
    for (a, b) in [((0.0, 0.0), (0.0, 1.5)), ((0.0, 1.5), (0.3, 1.8)), ((0.3, 1.8), (1.1, 1.8)), ((1.1, 1.8), (1.4, 1.5)), ((1.4, 1.5), (1.4, 0.0))] {
        s.line(d(a.0, a.1), d(b.0, b.1), false);
    }
    for k in 0..3 {
        let z = k as f32 * 0.12;
        s.line(d(0.0, z), d(1.4, z), false);
    }
    // a window with a view of the night
    let w = |t: f32, z: f32| iso(window + t, 0.0, z);
    s.poly(&[w(0.0, 1.3), w(1.4, 1.3), w(1.4, 2.4), w(0.0, 2.4)], false, |_, _| false);
    for (a, b) in [((0.0, 1.3), (1.4, 1.3)), ((1.4, 1.3), (1.4, 2.4)), ((1.4, 2.4), (0.0, 2.4)), ((0.0, 2.4), (0.0, 1.3)), ((0.7, 1.3), (0.7, 2.4)), ((0.0, 1.85), (1.4, 1.85))] {
        s.line(w(a.0, a.1), w(b.0, b.1), false);
    }
    for (t, z) in [(0.25, 2.15), (1.0, 2.0), (0.45, 1.5), (1.2, 1.6)] {
        let (x, y) = w(t, z);
        s.plot(x as i32, y as i32);
    }
    // floor tiles and the front of the floor slab
    for k in 0..=8 {
        let t = k as f32;
        let dotted = k != 0 && k != 8;
        s.line(iso(t, 0.0, 0.0), iso(t, 8.0, 0.0), dotted);
        s.line(iso(0.0, t, 0.0), iso(8.0, t, 0.0), dotted);
    }
    let (f0, f1, f2) = (iso(8.0, 0.0, 0.0), iso(8.0, 8.0, 0.0), iso(0.0, 8.0, 0.0));
    let down = |p: (f32, f32)| (p.0, p.1 + 6.0);
    s.poly(&[f0, f1, down(f1), down(f0)], false, |_, y| y % 2 == 0);
    s.poly(&[f2, f1, down(f1), down(f2)], false, |x, y| (x + y) % 2 == 0);
    for (a, b) in [(f0, down(f0)), (f1, down(f1)), (f2, down(f2)), (down(f0), down(f1)), (down(f1), down(f2))] {
        s.line(a, b, false);
    }
}

/// A filmation-style room: a little robot hops from tile to tile on every beat
/// collecting things while a spring platform bounces with the kick.
struct Isometric {
    scr: Scr,
    clock: Clock,
    rng: Rng,
    walk: Spr,
    hop: Spr,
    back: Spr,
    items: [Spr; 4],
    sparkle: Spr,
    ink0: usize,
    style: u32,
    room: u32,
    solid: [bool; 64],
    from: (i32, i32),
    to: (i32, i32),
    step: i64,
    goods: [(i32, i32, f32); 4],
    taken: [u32; 4],
    spark: (i32, i32, f32),
    score: u32,
    order: Vec<(f32, u8, usize)>,
}

impl Isometric {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        Isometric {
            scr: Scr::new(),
            clock: Clock::default(),
            walk: spr(&art::ROBOT),
            hop: spr(&art::ROBOT_HOP),
            back: spr(&art::ROBOT_BACK),
            items: [spr(&art::GEM), spr(&art::DISK), spr(&art::MUG), spr(&art::BATTERY)],
            sparkle: spr(&art::SPARKLE),
            ink0: rng.below(5) as usize,
            style: rng.below(2),
            room: u32::MAX,
            solid: [false; 64],
            from: (3, 7),
            to: (3, 7),
            step: i64::MIN,
            goods: [(-1, -1, 0.0); 4],
            taken: [0; 4],
            spark: (0, 0, -99.0),
            score: 0,
            order: Vec::with_capacity(32),
            rng,
        }
    }

    fn layout(&self) -> usize {
        (self.room as usize + self.ink0) % 4
    }

    fn free(&self, t: (i32, i32)) -> bool {
        (0..8).contains(&t.0) && (0..8).contains(&t.1) && !self.solid[(t.1 * 8 + t.0) as usize]
    }

    fn free_tile(&mut self) -> (i32, i32) {
        for _ in 0..64 {
            let t = (self.rng.below(8) as i32, self.rng.below(8) as i32);
            if self.free(t) && t != self.to && !self.goods.iter().any(|g| (g.0, g.1) == t) {
                return t;
            }
        }
        (3, 7)
    }

    fn enter(&mut self, room: u32) {
        self.room = room;
        let k = self.layout();
        self.solid = [false; 64];
        for &(u, v, _) in ISO_ROOMS[k] {
            self.solid[(v * 8 + u) as usize] = true;
        }
        let (pu, pv) = ISO_PLATFORM[k];
        self.solid[(pv * 8 + pu) as usize] = true;
        if !self.free(self.to) {
            self.to = self.free_tile();
        }
        self.from = self.to;
        for i in 0..4 {
            self.goods[i] = (-1, -1, 0.0);
            let t = self.free_tile();
            self.goods[i] = (t.0, t.1, 0.0);
        }
    }

    /// Land on the next tile: pick up what is there, then choose where to hop next.
    fn hop_on(&mut self, bt: f32) {
        let here = self.to;
        for k in 0..4 {
            let (u, v, back) = self.goods[k];
            if (u, v) == here && bt >= back {
                self.taken[k] += 1;
                self.score += 150;
                self.spark = (u, v, bt);
                let t = self.free_tile();
                self.goods[k] = (t.0, t.1, bt + 6.0);
            }
        }
        let mut best: Option<(i32, (i32, i32))> = None;
        for &(u, v, back) in &self.goods {
            if bt >= back {
                let d = (u - here.0).abs() + (v - here.1).abs();
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, (u, v)));
                }
            }
        }
        let target = best.map(|b| b.1).unwrap_or((4, 4));
        let (du, dv) = ((target.0 - here.0).signum(), (target.1 - here.1).signum());
        let along_u = (target.0 - here.0).abs() >= (target.1 - here.1).abs();
        let cands = if along_u { [(here.0 + du, here.1), (here.0, here.1 + dv)] } else { [(here.0, here.1 + dv), (here.0 + du, here.1)] };
        let mut next = here;
        for cand in cands {
            if cand != here && self.free(cand) {
                next = cand;
                break;
            }
        }
        if next == here {
            for _ in 0..6 {
                let d = [(1, 0), (-1, 0), (0, 1), (0, -1)][self.rng.below(4) as usize];
                let cand = (here.0 + d.0, here.1 + d.1);
                if self.free(cand) {
                    next = cand;
                    break;
                }
            }
        }
        self.from = here;
        self.to = next;
    }

    fn hud(&mut self, a: &Audio, ink: u8) {
        let s = &mut self.scr;
        s.cells(0, 21, 32, 3, at(WHITE, BLACK));
        s.print(1, 21, "POWER", Some(at(CYAN | BR, BLACK)));
        meter(s, 7, 21, 8, a.bass);
        s.print(1, 22, "SCORE", Some(at(WHITE | BR, BLACK)));
        s.num(7, 22, self.score, 6, Some(at(YELLOW | BR, BLACK)));
        s.print(1, 23, "ROOM", Some(at(WHITE | BR, BLACK)));
        s.num(7, 23, self.room % 100 + 1, 2, Some(at(ink, BLACK)));
        for k in 0..4 {
            let cx = 16 + k as i32 * 4;
            let (x, y) = (cx * 8, 21 * 8 + 1);
            for (p, q) in [((x, y), (x + 17, y)), ((x + 17, y), (x + 17, y + 17)), ((x + 17, y + 17), (x, y + 17)), ((x, y + 17), (x, y))] {
                s.ln(p.0, p.1, q.0, q.1);
            }
            s.cells(cx, 21, 3, 3, at(WHITE, BLACK));
            if self.taken[k] > 0 {
                s.sprite(&self.items[k], x + 5, y + 5, false, ITEM_INK[k]);
            } else {
                s.draw(&self.items[k], x + 5, y + 5, false);
                s.ink(x + 5, y + 5, 8, 8, BLUE);
            }
            s.num(cx + 3, 23, self.taken[k].min(9), 1, Some(at(ITEM_INK[k], BLACK)));
        }
    }
}

impl Effect for Isometric {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, _) = self.clock.tick(c);
        let room = (bt / 32.0).max(0.0) as u32;
        if room != self.room {
            self.enter(room);
        }
        let st = bt.floor() as i64;
        if st != self.step {
            self.step = st;
            self.hop_on(bt);
        }
        let frac = fract(bt);
        let k = self.layout();
        let ink = ISO_INKS[(self.room as usize + self.ink0) % ISO_INKS.len()];
        self.scr.clear(at(ink, BLACK));
        iso_room(&mut self.scr, self.style, ISO_DOOR[k], ISO_WINDOW[k]);

        // painter's order: everything sorted by depth (u + v), lowest first
        let e = smoothstep(frac);
        let (fu, fv) = (lerp(self.from.0 as f32, self.to.0 as f32, e), lerp(self.from.1 as f32, self.to.1 as f32, e));
        self.order.clear();
        for (i, &(u, v, h)) in ISO_ROOMS[k].iter().enumerate() {
            for z in 0..h {
                self.order.push(((u + v) as f32 + z as f32 * 0.01, 0, i * 4 + z as usize));
            }
        }
        let (pu, pv) = ISO_PLATFORM[k];
        self.order.push(((pu + pv) as f32 + 0.05, 1, 0));
        for (i, g) in self.goods.iter().enumerate() {
            if bt >= g.2 && g.0 >= 0 {
                self.order.push(((g.0 + g.1) as f32 + 0.3, 2, i));
            }
        }
        let rd = (self.from.0 + self.from.1).max(self.to.0 + self.to.1) as f32 + 0.5;
        self.order.push((rd, 3, 0));
        self.order.sort_by(|p, q| p.0.total_cmp(&q.0));

        for n in 0..self.order.len() {
            let (_, kind, i) = self.order[n];
            match kind {
                0 => {
                    let (u, v, _) = ISO_ROOMS[k][i / 4];
                    iso_block(&mut self.scr, u as f32, v as f32, (i % 4) as f32, 1.0, 1);
                }
                1 => {
                    let z = 0.1 + a.pulse * 0.9 + a.bass * 0.25;
                    let (bx, by) = iso(pu as f32 + 0.5, pv as f32 + 0.5, 0.0);
                    let top = iso(pu as f32 + 0.5, pv as f32 + 0.5, z).1 + 6.0;
                    // the spring
                    let n = 6;
                    for j in 0..n {
                        let y0 = by + (top - by) * j as f32 / n as f32;
                        let y1 = by + (top - by) * (j + 1) as f32 / n as f32;
                        let sx = if j % 2 == 0 { 4.0 } else { -4.0 };
                        self.scr.line((bx - sx, y0), (bx + sx, y1), false);
                    }
                    iso_block(&mut self.scr, pu as f32, pv as f32, z, 0.3, 2);
                }
                2 => {
                    let (u, v, _) = self.goods[i];
                    let z = 0.35 + 0.12 * (c.t * 3.0 + i as f32 * 1.7).sin();
                    let (sx, sy) = iso(u as f32 + 0.5, v as f32 + 0.5, z);
                    let twinkle = a.treble > 0.6 && ((c.t * 12.0) as i32 + i as i32) % 2 == 0;
                    let ink = if twinkle { WHITE | BR } else { ITEM_INK[i] };
                    self.scr.sprite(&self.items[i], sx as i32 - 4, sy as i32 - 8, false, ink);
                }
                _ => {
                    let z = (PI * frac).sin() * (0.45 + 0.6 * a.bass);
                    let (gx, gy) = iso(fu + 0.5, fv + 0.5, 0.0);
                    if z > 0.08 {
                        let r = 6.0 - z * 2.0;
                        self.scr.oval(gx, gy, r, r * 0.5, |x, y| (x + y) % 2 == 0);
                    }
                    let (sx, sy) = iso(fu + 0.5, fv + 0.5, z);
                    let toward = self.to.0 > self.from.0 || self.to.1 > self.from.1 || self.to == self.from;
                    let left = (self.to.0 - self.from.0) - (self.to.1 - self.from.1) < 0;
                    let air = frac > 0.12 && frac < 0.88 && self.to != self.from;
                    let sp = if !toward { &self.back } else if air { &self.hop } else { &self.walk };
                    self.scr.sprite(sp, sx as i32 - 8, sy as i32 - 15, left, WHITE | BR);
                }
            }
        }
        let age = bt - self.spark.2;
        if age < 1.0 {
            let (sx, sy) = iso(self.spark.0 as f32 + 0.5, self.spark.1 as f32 + 0.5, 0.6 + age * 1.5);
            self.scr.sprite(&self.sparkle, sx as i32 - 4, sy as i32 - 8, (c.t * 10.0) as i32 % 2 == 0, YELLOW | BR);
        }
        self.hud(a, ink);
        let flash = age < 0.25;
        let bc = zx(if flash { ink & 7 } else { BLACK });
        border(fb, |_| bc);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}

// ================================================================ 2. LADDERS

struct LadderLevel {
    /// (row, first col, last col)
    floors: &'static [(i32, i32, i32)],
    /// (col, top floor row, bottom floor row)
    ladders: &'static [(i32, i32, i32)],
    /// (col, floor row)
    fuel: &'static [(i32, i32)],
    /// patrol (floor row, first col, last col)
    aliens: &'static [(i32, i32, i32)],
}

const LADDER_LEVELS: [LadderLevel; 2] = [
    LadderLevel {
        floors: &[(23, 0, 31), (18, 0, 11), (18, 15, 31), (13, 3, 28), (8, 0, 9), (8, 18, 31)],
        ladders: &[(10, 18, 23), (27, 18, 23), (5, 13, 18), (20, 13, 18), (7, 8, 13), (25, 8, 13)],
        fuel: &[(2, 8), (29, 8), (14, 13), (26, 13), (1, 18), (22, 18), (4, 23), (17, 23)],
        aliens: &[(23, 12, 30), (18, 15, 30), (13, 8, 26), (8, 18, 31)],
    },
    LadderLevel {
        floors: &[(23, 0, 31), (18, 4, 27), (13, 0, 13), (13, 18, 31), (8, 6, 25)],
        ladders: &[(6, 18, 23), (24, 18, 23), (11, 13, 18), (19, 13, 18), (8, 8, 13), (23, 8, 13)],
        fuel: &[(1, 13), (30, 13), (15, 8), (24, 8), (26, 18), (5, 18), (15, 23), (29, 23)],
        aliens: &[(23, 2, 30), (18, 7, 27), (13, 19, 31), (8, 9, 25)],
    },
];

const GIRDER: [u8; 8] = [0xFF, 0xC3, 0xA5, 0x99, 0x99, 0xA5, 0xC3, 0xFF];
const LADDER: [u8; 8] = [0x81, 0x81, 0xFF, 0x81, 0x81, 0x81, 0xFF, 0x81];

/// Platforms and ladders: an astronaut climbs about collecting glowing fuel
/// cells (shortest route by breadth-first search) while aliens patrol in step.
struct Ladders {
    scr: Scr,
    clock: Clock,
    walk: [Spr; 2],
    climb: Spr,
    foes: [[Spr; 2]; 3],
    cell: Spr,
    level: usize,
    girder: u8,
    floor: [bool; 768],
    ladder: [bool; 768],
    got: [bool; 8],
    stars: Vec<(i32, i32)>,
    pos: (i32, i32),
    prev: (i32, i32),
    prog: f32,
    left: bool,
    path: Vec<(i32, i32)>,
    dist: Vec<i16>,
    back: Vec<i16>,
    queue: Vec<i16>,
    score: u32,
    hi: u32,
    stage: u32,
    done_at: f32,
    stage_start: f32,
}

impl Ladders {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let mut l = Ladders {
            scr: Scr::new(),
            clock: Clock::default(),
            walk: [spr(&art::ASTRO), spr(&art::ASTRO_B)],
            climb: spr(&art::ASTRO_CLIMB),
            foes: [
                [spr(&art::JELLY), spr(&art::JELLY_B)],
                [spr(&art::CRAWLER), spr(&art::CRAWLER_B)],
                [spr(&art::SPINNER), spr(&art::SPINNER_B)],
            ],
            cell: spr(&art::FUEL),
            level: r.below(2) as usize,
            girder: [MAGENTA, RED, CYAN, GREEN][r.below(4) as usize] | BR,
            floor: [false; 768],
            ladder: [false; 768],
            got: [false; 8],
            stars: (0..40).map(|_| (r.below(256) as i32, 16 + r.below(170) as i32)).collect(),
            pos: (1, 23),
            prev: (1, 23),
            prog: 0.0,
            left: false,
            path: Vec::with_capacity(128),
            dist: vec![-1; 768],
            back: vec![0; 768],
            queue: vec![0; 768],
            score: 0,
            hi: 10000 + r.below(20) * 500,
            stage: 1,
            done_at: -1.0,
            stage_start: 0.0,
        };
        l.build();
        l
    }

    fn lv(&self) -> &'static LadderLevel {
        &LADDER_LEVELS[self.level]
    }

    fn build(&mut self) {
        self.floor = [false; 768];
        self.ladder = [false; 768];
        let lv = self.lv();
        for &(r, c0, c1) in lv.floors {
            for c in c0..=c1 {
                self.floor[(r * 32 + c) as usize] = true;
            }
        }
        for &(c, top, bottom) in lv.ladders {
            for r in top..bottom {
                self.ladder[(r * 32 + c) as usize] = true;
            }
        }
        self.pos = (1, 23);
        self.prev = self.pos;
        self.path.clear();
    }

    fn is(map: &[bool; 768], p: (i32, i32)) -> bool {
        (0..32).contains(&p.0) && (0..24).contains(&p.1) && map[(p.1 * 32 + p.0) as usize]
    }

    fn climbable(&self, p: (i32, i32)) -> bool {
        Self::is(&self.ladder, p) || Self::is(&self.ladder, (p.0, p.1 - 1))
    }

    fn can_move(&self, a: (i32, i32), b: (i32, i32)) -> bool {
        if !(0..32).contains(&b.0) || !(3..24).contains(&b.1) {
            return false;
        }
        if a.1 == b.1 {
            Self::is(&self.floor, a) && Self::is(&self.floor, b)
        } else {
            a.0 == b.0 && self.climbable(a) && self.climbable(b)
        }
    }

    /// Breadth-first search to the nearest fuel cell still on the level.
    fn route(&mut self) {
        let fuel = self.lv().fuel;
        self.dist.fill(-1);
        let start = (self.pos.1 * 32 + self.pos.0) as usize;
        self.dist[start] = 0;
        self.queue[0] = start as i16;
        let (mut head, mut tail) = (0, 1);
        while head < tail {
            let n = self.queue[head] as i32;
            head += 1;
            let (c, r) = (n % 32, n / 32);
            for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nb = (c + dc, r + dr);
                if !self.can_move((c, r), nb) {
                    continue;
                }
                let m = (nb.1 * 32 + nb.0) as usize;
                if self.dist[m] < 0 {
                    self.dist[m] = self.dist[n as usize] + 1;
                    self.back[m] = n as i16;
                    self.queue[tail] = m as i16;
                    tail += 1;
                }
            }
        }
        let mut best: Option<(i16, usize)> = None;
        for (i, &(fc, fr)) in fuel.iter().enumerate() {
            let d = self.dist[(fr * 32 + fc) as usize];
            if !self.got[i] && d >= 0 && best.is_none_or(|b| d < b.0) {
                best = Some((d, (fr * 32 + fc) as usize));
            }
        }
        self.path.clear();
        if let Some((_, mut n)) = best {
            while n != start {
                self.path.push(((n % 32) as i32, (n / 32) as i32));
                n = self.back[n] as usize;
            }
        }
    }

    fn arrive(&mut self, bt: f32) {
        self.prev = self.pos;
        let fuel = self.lv().fuel;
        for (i, &(fc, fr)) in fuel.iter().enumerate() {
            if !self.got[i] && fr == self.pos.1 && (fc - self.pos.0).abs() <= 1 {
                self.got[i] = true;
                self.score += 250;
                self.hi = self.hi.max(self.score);
            }
        }
        if self.got[..fuel.len()].iter().all(|&g| g) {
            self.done_at = bt;
            return;
        }
        if self.path.is_empty() {
            self.route();
        }
        if let Some(n) = self.path.pop() {
            self.pos = n;
            if n.0 != self.prev.0 {
                self.left = n.0 < self.prev.0;
            }
        }
    }
}

impl Effect for Ladders {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, _) = self.clock.tick(c);
        if self.done_at < 0.0 {
            self.prog += c.dt / a.period.max(0.1) * 3.2 * (0.75 + 0.6 * a.bass);
            while self.prog >= 1.0 && self.done_at < 0.0 {
                self.prog -= 1.0;
                self.arrive(bt);
            }
        } else if bt - self.done_at > 4.0 {
            self.done_at = -1.0;
            self.got = [false; 8];
            self.stage += 1;
            self.level = (self.level + 1) % LADDER_LEVELS.len();
            self.stage_start = bt;
            self.build();
        }
        let lv = self.lv();
        let s = &mut self.scr;
        s.clear(at(WHITE, BLACK));

        // a ringed planet and twinkling stars
        s.oval(220.0, 36.0, 12.0, 12.0, |x, y| (x + y) % 2 == 0 || (y % 3 == 0 && x % 2 == 0));
        s.oval(214.0, 31.0, 4.0, 3.0, |_, _| true);
        for t in 0..60 {
            let ang = t as f32 / 60.0 * TAU;
            let (x, y) = (220.0 + ang.cos() * 21.0, 36.0 + ang.sin() * 5.0);
            if !(ang.sin() < 0.0 && (x - 220.0).abs() < 12.0) {
                s.plot(x as i32, y as i32);
            }
        }
        s.cells(25, 2, 6, 5, at(MAGENTA | BR, BLACK));
        for (i, &(x, y)) in self.stars.iter().enumerate() {
            let cell = ((y >> 3) * 32 + (x >> 3)) as usize;
            if self.floor[cell] || self.ladder[cell] || (x > 192 && y < 56) {
                continue;
            }
            if hash(i as u32 ^ c.beats.wrapping_mul(0x9E37)) % 5 != 0 {
                s.plot(x, y);
                if a.treble > 0.6 && i % 4 == 0 {
                    s.plot(x - 1, y);
                    s.plot(x + 1, y);
                    s.plot(x, y - 1);
                    s.plot(x, y + 1);
                }
            }
        }

        // girders and ladders
        for i in 0..768 {
            let (cx, cy) = (i as i32 % 32, i as i32 / 32);
            if self.ladder[i] {
                s.tile(cx, cy, &LADDER);
                s.cell(cx, cy, at(YELLOW | BR, BLACK));
            } else if self.floor[i] {
                s.tile(cx, cy, &GIRDER);
                s.cell(cx, cy, at(self.girder, BLACK));
            }
        }

        // fuel cells glow and flash with the treble
        for (i, &(fc, fr)) in lv.fuel.iter().enumerate() {
            if self.got[i] {
                continue;
            }
            let ink = [YELLOW, GREEN, CYAN, WHITE][((c.t * (3.0 + a.treble * 16.0)) as usize + i) % 4] | BR;
            s.draw(&self.cell, fc * 8, fr * 8 - 8, false);
            s.cell(fc, fr - 1, at(ink, BLACK) | if a.treble > 0.55 { FLASH } else { 0 });
        }

        // aliens step a few pixels every quarter beat
        for (i, &(row, c0, c1)) in lv.aliens.iter().enumerate() {
            let kind = i % 3;
            let steps = (((c1 - c0) * 8 - 16) / 4).max(2);
            let tick = (bt * 4.0).floor() as i32 + i as i32 * 5;
            let k = tick.rem_euclid(steps * 2);
            let off = if k < steps { k } else { 2 * steps - k };
            let sp = &self.foes[kind][(tick & 1) as usize];
            let bob = if kind == 0 { ((bt * PI).sin().abs() * (3.0 + 5.0 * a.bass)) as i32 } else { 0 };
            let lift = if kind == 2 { 10 } else { 0 };
            let ink = [GREEN, MAGENTA, CYAN][kind] | BR;
            s.sprite(sp, c0 * 8 + off * 4, row * 8 - sp.h - bob - lift, k >= steps, ink);
        }

        // the astronaut
        let p = self.prog.min(1.0);
        let x = lerp(self.prev.0 as f32, self.pos.0 as f32, p) * 8.0 - 4.0;
        let mut y = lerp(self.prev.1 as f32, self.pos.1 as f32, p) * 8.0 - 16.0;
        let climbing = self.prev.0 == self.pos.0 && self.prev.1 != self.pos.1;
        if self.done_at >= 0.0 {
            y -= (bt * PI * 2.0).sin().abs() * 8.0;
        }
        if climbing {
            s.sprite(&self.climb, x as i32, y as i32, (y as i32 / 4) % 2 == 0, WHITE | BR);
        } else {
            let f = if self.prev == self.pos { 0 } else { ((x as i32) / 4).rem_euclid(2) as usize };
            s.sprite(&self.walk[f], x as i32, y as i32, self.left, WHITE | BR);
        }

        // status lines
        s.cells(0, 0, 32, 2, at(WHITE | BR, BLACK));
        s.print(0, 0, "SCORE", None);
        s.num(6, 0, self.score, 6, Some(at(YELLOW | BR, BLACK)));
        s.print(13, 0, "HI", None);
        s.num(16, 0, self.hi, 6, Some(at(CYAN | BR, BLACK)));
        s.print(24, 0, "STAGE", None);
        s.num(30, 0, self.stage % 100, 2, Some(at(GREEN | BR, BLACK)));
        s.print(0, 1, "FUEL", Some(at(GREEN | BR, BLACK)));
        for i in 0..lv.fuel.len() {
            s.tile(5 + i as i32, 1, &[0x18, 0x7E, 0x42, 0x5A, 0x5A, 0x42, 0x7E, 0]);
            s.cell(5 + i as i32, 1, at(if self.got[i] { GREEN | BR } else { BLUE }, BLACK));
        }
        s.print(16, 1, "OXYGEN", Some(at(CYAN | BR, BLACK)));
        meter(s, 23, 1, 9, 1.0 - (bt - self.stage_start) / 64.0);
        if self.done_at >= 0.0 {
            s.cells(8, 4, 16, 1, at(YELLOW | BR, RED) | FLASH);
            s.print(8, 4, " STAGE CLEARED! ", None);
        }
        let bc = if self.done_at >= 0.0 { zx(1 + ((bt * 4.0) as u32 % 7) as u8) } else { zx(BLACK) };
        border(fb, |_| bc);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}

// ================================================================ 3. MAZE RAID

const BRICK: [u8; 8] = [0xFE, 0xFE, 0xFE, 0x00, 0xEF, 0xEF, 0xEF, 0x00];
const STUD: [u8; 8] = [0xFF, 0x81, 0xBD, 0xA5, 0xA5, 0xBD, 0x81, 0xFF];
const BARS: [u8; 8] = [0xFF, 0x99, 0x99, 0xFF, 0x99, 0x99, 0x99, 0xFF];
const FLOOR_TILES: [[u8; 8]; 2] = [[0, 0, 0, 0x10, 0, 0, 0, 0], [0, 0x40, 0x20, 0, 0, 0x04, 0, 0]];

/// Where a doorway is: side 0 top, 1 right, 2 bottom, 3 left; `out` is just outside.
fn door_point(side: usize, out: bool) -> (f32, f32) {
    match side {
        0 => (128.0, if out { 4.0 } else { 40.0 }),
        1 => (if out { 268.0 } else { 230.0 }, 104.0),
        2 => (128.0, if out { 204.0 } else { 172.0 }),
        _ => (if out { -12.0 } else { 26.0 }, 104.0),
    }
}

fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy).max(1e-3)).clamp(0.0, 1.0);
    ((p.0 - a.0 - dx * t).powi(2) + (p.1 - a.1 - dy * t).powi(2)).sqrt()
}

/// Top-down flick-screen dungeon: a hooded hero grabs the key, the matching
/// door opens and the screen flicks to the next room every 8 beats. Trolls
/// lurch after the hero on the beat; the hero fires a bolt on every beat.
struct MazeRaid {
    scr: Scr,
    clock: Clock,
    seed: u32,
    hero: [Spr; 2],
    troll: [Spr; 2],
    key: Spr,
    torch: [Spr; 2],
    bolt: Spr,
    room: i64,
    map: [u8; 768],
    wall: u8,
    key_ink: u8,
    key_pos: (f32, f32),
    entry: usize,
    exit: usize,
    has_key: bool,
    trolls: [(f32, f32, f32); 3],
    bolts: [(f32, f32, f32, f32, bool); 4],
    keys: [u8; 8],
    nkeys: usize,
    score: u32,
    slain: u32,
    hero_pos: (f32, f32),
    left: bool,
}

impl MazeRaid {
    fn new(seed: u32) -> Self {
        MazeRaid {
            scr: Scr::new(),
            clock: Clock::default(),
            seed,
            hero: [spr(&art::HERO), spr(&art::HERO_B)],
            troll: [spr(&art::TROLL), spr(&art::TROLL_B)],
            key: spr(&art::KEY),
            torch: [spr(&art::TORCH), spr(&art::TORCH_B)],
            bolt: spr(&art::BOLT),
            room: i64::MIN,
            map: [1; 768],
            wall: RED | BR,
            key_ink: YELLOW | BR,
            key_pos: (128.0, 104.0),
            entry: 3,
            exit: 1,
            has_key: false,
            trolls: [(0.0, 0.0, -99.0); 3],
            bolts: [(0.0, 0.0, 0.0, 0.0, false); 4],
            keys: [0; 8],
            nkeys: 0,
            score: 0,
            slain: 0,
            hero_pos: (0.0, 0.0),
            left: false,
        }
    }

    fn enter(&mut self, room: i64) {
        let mut r = Rng::new(self.seed ^ (room as u32).wrapping_mul(0x2F6B_1A35) ^ 0xA5);
        let first = self.room == i64::MIN;
        self.entry = if first { r.below(4) as usize } else { (self.exit + 2) % 4 };
        self.exit = (self.entry + 1 + r.below(3) as usize) % 4;
        self.room = room;
        let walls = [RED, BLUE, MAGENTA, GREEN, CYAN, YELLOW];
        self.wall = walls[r.below(6) as usize] | BR;
        loop {
            self.key_ink = [YELLOW, CYAN, GREEN, MAGENTA, RED, WHITE][r.below(6) as usize] | BR;
            if self.key_ink != self.wall {
                break;
            }
        }
        let ctr = (128.0, 104.0);
        loop {
            self.key_pos = (r.range(40.0, 216.0), r.range(48.0, 160.0));
            if (self.key_pos.0 - ctr.0).hypot(self.key_pos.1 - ctr.1) > 46.0 {
                break;
            }
        }
        for row in 0..24 {
            for col in 0..32 {
                let edge = col == 0 || col == 31 || row <= 2 || row == 23;
                self.map[(row * 32 + col) as usize] = edge as u8;
            }
        }
        for (side, v) in [(self.entry, 0u8), (self.exit, 3u8)] {
            for k in 0..4 {
                let (c, rr) = match side {
                    0 => (14 + k, 2),
                    1 => (31, 11 + k),
                    2 => (14 + k, 23),
                    _ => (0, 11 + k),
                };
                self.map[(rr * 32 + c) as usize] = v;
            }
        }
        let segs = [(door_point(self.entry, false), ctr), (ctr, self.key_pos), (ctr, door_point(self.exit, false))];
        let mut placed = 0;
        for _ in 0..60 {
            if placed >= 7 {
                break;
            }
            let (cx, cy) = (2 + r.below(27) as i32, 4 + r.below(17) as i32);
            let p = (cx as f32 * 8.0 + 8.0, cy as f32 * 8.0 + 8.0);
            let clear = segs.iter().all(|&(a, b)| seg_dist(p, a, b) > 24.0)
                && (p.0 - self.key_pos.0).hypot(p.1 - self.key_pos.1) > 20.0
                && (0..4).all(|k| self.map[((cy + k / 2) * 32 + cx + k % 2) as usize] == 0);
            if clear {
                for k in 0..4 {
                    self.map[((cy + k / 2) * 32 + cx + k % 2) as usize] = 2;
                }
                placed += 1;
            }
        }
        self.has_key = false;
        let ent = door_point(self.entry, false);
        let corners = [(28.0, 44.0), (228.0, 44.0), (28.0, 166.0), (228.0, 166.0)];
        let mut n = 0;
        let mut order = [0usize, 1, 2, 3];
        order.sort_by(|&i, &j| {
            let di = (corners[i].0 - ent.0).hypot(corners[i].1 - ent.1);
            let dj = (corners[j].0 - ent.0).hypot(corners[j].1 - ent.1);
            dj.total_cmp(&di)
        });
        for &i in &order[..3] {
            self.trolls[n] = (corners[i].0, corners[i].1, -99.0);
            n += 1;
        }
        self.bolts = [(0.0, 0.0, 0.0, 0.0, false); 4];
    }

    fn blocked(&self, x: f32, y: f32) -> bool {
        [(-6.0, -6.0), (5.0, -6.0), (-6.0, 5.0), (5.0, 5.0)].iter().any(|&(dx, dy)| {
            let (px, py) = ((x + dx) as i32, (y + dy) as i32);
            !(0..SW).contains(&px) || !(0..SH).contains(&py) || self.map[((py >> 3) * 32 + (px >> 3)) as usize] != 0
        })
    }

    /// The hero's walk through the room, as a function of beats since entering.
    fn hero_at(&self, tb: f32) -> (f32, f32) {
        let (ent_o, ent_i) = (door_point(self.entry, true), door_point(self.entry, false));
        let (ex_i, ex_o) = (door_point(self.exit, false), door_point(self.exit, true));
        let (ctr, key) = ((128.0, 104.0), self.key_pos);
        let legs = [
            (0.0, 0.7, ent_o, ent_i),
            (0.7, 1.8, ent_i, ctr),
            (1.8, 3.0, ctr, key),
            (3.0, 3.6, key, key),
            (3.6, 4.7, key, ctr),
            (4.7, 6.2, ctr, ex_i),
            (6.2, 7.4, ex_i, ex_o),
        ];
        for (t0, t1, p0, p1) in legs {
            if tb < t1 {
                let k = smoothstep(((tb - t0) / (t1 - t0)).clamp(0.0, 1.0));
                return (lerp(p0.0, p1.0, k), lerp(p0.1, p1.1, k));
            }
        }
        ex_o
    }
}

impl Effect for MazeRaid {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, new_beat) = self.clock.tick(c);
        let room = (bt / 8.0).floor() as i64;
        if room != self.room {
            self.enter(room);
        }
        let tb = bt - room as f32 * 8.0;
        let frac = fract(bt);
        let hp = self.hero_at(tb);
        if (hp.0 - self.hero_pos.0).abs() > 0.05 {
            self.left = hp.0 < self.hero_pos.0;
        }
        let moving = (hp.0 - self.hero_pos.0).hypot(hp.1 - self.hero_pos.1) > 0.05;
        self.hero_pos = hp;
        if tb >= 3.0 && !self.has_key {
            self.has_key = true;
            self.score += 500;
            self.keys[self.nkeys % 8] = self.key_ink;
            self.nkeys += 1;
            for v in self.map.iter_mut() {
                if *v == 3 {
                    *v = 0;
                }
            }
        }
        let inside = tb > 0.5 && tb < 7.2;

        // trolls lurch after the hero, hardest right on the beat
        let lurch = (1.0 - frac).powi(2) * 2.0;
        for i in 0..3 {
            let (x, y, dead) = self.trolls[i];
            if dead > -50.0 {
                if bt - dead > 2.5 {
                    let corner = [(28.0, 44.0), (228.0, 44.0), (28.0, 166.0), (228.0, 166.0)][(hash(i as u32 + bt as u32) % 4) as usize];
                    if (corner.0 - hp.0).hypot(corner.1 - hp.1) > 60.0 {
                        self.trolls[i] = (corner.0, corner.1, -99.0);
                    }
                }
                continue;
            }
            let (dx, dy) = (hp.0 - x, hp.1 - y);
            let d = dx.hypot(dy).max(1.0);
            let sp = (22.0 + 34.0 * a.bass) * lurch * c.dt;
            let nx = x + dx / d * sp;
            let ny = y + dy / d * sp;
            let mut t = (x, y, dead);
            if !self.blocked(nx, y) {
                t.0 = nx;
            }
            if !self.blocked(t.0, ny) {
                t.1 = ny;
            }
            self.trolls[i] = t;
        }

        // a bolt on every beat, at the nearest troll
        if new_beat && inside {
            let target = (0..3).filter(|&i| self.trolls[i].2 < -50.0).min_by(|&i, &j| {
                let di = (self.trolls[i].0 - hp.0).hypot(self.trolls[i].1 - hp.1);
                let dj = (self.trolls[j].0 - hp.0).hypot(self.trolls[j].1 - hp.1);
                di.total_cmp(&dj)
            });
            if let (Some(i), Some(slot)) = (target, self.bolts.iter().position(|b| !b.4)) {
                let (dx, dy) = (self.trolls[i].0 - hp.0, self.trolls[i].1 - hp.1);
                let d = dx.hypot(dy).max(1.0);
                self.bolts[slot] = (hp.0, hp.1, dx / d * 260.0, dy / d * 260.0, true);
            }
        }
        for b in 0..4 {
            let (mut x, mut y, vx, vy, live) = self.bolts[b];
            if !live {
                continue;
            }
            x += vx * c.dt;
            y += vy * c.dt;
            let mut alive = !(x < 0.0 || y < 16.0 || x >= 256.0 || y >= 192.0) && self.map[((y as i32 >> 3) * 32 + (x as i32 >> 3)) as usize] == 0;
            for t in self.trolls.iter_mut() {
                if alive && t.2 < -50.0 && (t.0 - x).hypot(t.1 - y) < 9.0 {
                    t.2 = bt;
                    alive = false;
                    self.score += 100;
                    self.slain += 1;
                }
            }
            self.bolts[b] = (x, y, vx, vy, alive);
        }

        // the room
        let s = &mut self.scr;
        s.clear(at(BLUE, BLACK));
        for row in 2..24 {
            for col in 0..32 {
                let i = (row * 32 + col) as usize;
                match self.map[i] {
                    1 => {
                        s.tile(col, row, &BRICK);
                        s.cell(col, row, at(self.wall, BLACK));
                    }
                    2 => {
                        s.tile(col, row, &STUD);
                        s.cell(col, row, at(WHITE, BLACK));
                    }
                    3 => {
                        s.tile(col, row, &BARS);
                        s.cell(col, row, at(self.key_ink, BLACK));
                    }
                    _ => {
                        let h = hash(i as u32 ^ (self.room as u32).wrapping_mul(77)) % 9;
                        if h < 2 {
                            s.tile(col, row, &FLOOR_TILES[h as usize]);
                        }
                    }
                }
            }
        }
        let flick = (c.t * (6.0 + a.treble * 18.0)) as i32;
        for (k, &(col, row)) in [(6, 2), (25, 2), (0, 6), (31, 19), (6, 23), (25, 23)].iter().enumerate() {
            if self.map[(row * 32 + col) as usize] != 1 {
                continue;
            }
            s.tile(col, row, &[0; 8]);
            let f = ((flick + k as i32) % 2) as usize;
            s.draw(&self.torch[f], col * 8, row * 8, false);
            s.cell(col, row, at(if (flick + k as i32) % 3 == 0 { RED | BR } else { YELLOW | BR }, BLACK));
        }
        if !self.has_key {
            let (kx, ky) = (self.key_pos.0 as i32 - 4, self.key_pos.1 as i32 - 3 - (a.pulse * 3.0) as i32);
            s.sprite(&self.key, kx, ky, false, self.key_ink);
            s.each_cell(kx, ky, 8, 6, |v| v | FLASH);
        }
        for (i, &(x, y, dead)) in self.trolls.iter().enumerate() {
            if dead < -50.0 {
                let f = ((bt * 4.0) as i32 + i as i32) as usize % 2;
                s.sprite(&self.troll[f], x as i32 - 8, y as i32 - 8, x > hp.0, GREEN | BR);
            } else if bt - dead < 0.8 {
                s.burst(x, y, (bt - dead) / 0.8, i as u32 + dead as u32);
                s.ink(x as i32 - 16, y as i32 - 16, 32, 32, if (c.t * 20.0) as i32 % 2 == 0 { YELLOW | BR } else { RED | BR });
            }
        }
        for b in &self.bolts {
            if b.4 {
                s.sprite(&self.bolt, b.0 as i32 - 2, b.1 as i32 - 2, false, CYAN | BR);
            }
        }
        if tb < 7.4 {
            let f = if moving { ((bt * 4.0) as i32 % 2) as usize } else { 0 };
            s.sprite(&self.hero[f], hp.0 as i32 - 8, hp.1 as i32 - 8, self.left, WHITE | BR);
        }

        s.cells(0, 0, 32, 2, at(WHITE | BR, BLACK));
        s.tile(0, 1, &[0; 8]);
        s.print(0, 0, "SCORE", None);
        s.num(6, 0, self.score, 6, Some(at(YELLOW | BR, BLACK)));
        s.print(14, 0, "MAGIC", None);
        meter(s, 20, 0, 5, a.bass);
        s.print(25, 0, "ROOM", None);
        s.num(30, 0, (self.room.rem_euclid(100)) as u32 + 1, 2, Some(at(self.wall, BLACK)));
        s.print(0, 1, "KEYS", Some(at(WHITE | BR, BLACK)));
        for k in 0..self.nkeys.min(8) {
            let slot = (self.nkeys - 1 - k) % 8;
            s.tile(5 + k as i32, 1, &[0, 0x70, 0x88, 0x8F, 0x8B, 0x72, 0, 0]);
            s.cell(5 + k as i32, 1, at(self.keys[slot], BLACK));
        }
        s.print(16, 1, "TROLLS SLAIN", Some(at(GREEN | BR, BLACK)));
        s.num(29, 1, self.slain % 1000, 3, Some(at(GREEN | BR, BLACK)));
        let bc = zx(if tb < 0.12 { WHITE } else { BLACK });
        border(fb, |_| bc);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}

// ================================================================ 4. SPACE SHOOTER

/// Where enemy `i` of a formation is, `tw` beats into its wave.
fn foe_pos(kind: u32, i: usize, tw: f32, bt: f32) -> (f32, f32) {
    let fi = i as f32;
    let step = if (bt.floor() as i32) % 2 == 0 { -3.0 } else { 3.0 };
    match kind % 4 {
        0 => (264.0 + fi * 20.0 - tw * 36.0, 88.0 + 40.0 * (tw * 1.1 + fi * 0.6).sin() + step),
        1 => {
            let cx = 300.0 - tw.min(2.0) * 65.0 - (tw - 6.0).max(0.0) * 110.0;
            let ang = tw * 1.9 + fi * TAU / FOES as f32;
            (cx + 46.0 * ang.cos(), 88.0 + 42.0 * ang.sin())
        }
        2 => (
            300.0 - tw.min(2.5) * 64.0 + (i % 5) as f32 * 22.0 - (tw - 6.0).max(0.0) * 110.0,
            52.0 + (i / 5) as f32 * 44.0 + tri(bt * 0.5) * 24.0 + step,
        ),
        _ => (264.0 - tw * 34.0 + (fi - 4.5).abs() * 16.0, 88.0 + (fi - 4.5) * 12.0 + (bt * PI).sin() * 8.0),
    }
}

/// Enemies per wave.
const FOES: usize = 10;

fn ground(wx: f32) -> i32 {
    let h = 18.0 + 8.0 * (wx * 0.021).sin() + 6.0 * (wx * 0.0083 + 1.0).sin() + 3.0 * (wx * 0.067).sin();
    (h as i32) & !1
}

/// Horizontally scrolling shoot'em-up: star field, rainbow ground, enemy
/// formations dancing to the beat and a laser kill on every kick.
struct Shooter {
    scr: Scr,
    clock: Clock,
    seed: u32,
    ship: Spr,
    foes: [[Spr; 2]; 3],
    life: Spr,
    stars: Vec<(f32, i32, u8)>,
    scroll: f32,
    ship_y: f32,
    wave: i64,
    kind: u32,
    alive: [bool; FOES],
    boom: [f32; FOES],
    boom_at: [(f32, f32); FOES],
    laser: (f32, f32, f32, f32, f32),
    score: u32,
    hi: u32,
}

impl Shooter {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        Shooter {
            scr: Scr::new(),
            clock: Clock::default(),
            seed,
            ship: spr(&art::SHIP),
            foes: [[spr(&art::DARTER), spr(&art::DARTER_B)], [spr(&art::ORB), spr(&art::ORB_B)], [spr(&art::SQUID), spr(&art::SQUID_B)]],
            life: spr(&art::MINI_SHIP),
            stars: (0..56).map(|i| (r.range(0.0, 256.0), 18 + r.below(140) as i32, (i % 3) as u8)).collect(),
            scroll: r.range(0.0, 3000.0),
            ship_y: 90.0,
            wave: i64::MIN,
            kind: 0,
            alive: [true; FOES],
            boom: [-99.0; FOES],
            boom_at: [(0.0, 0.0); FOES],
            laser: (0.0, 0.0, 0.0, 0.0, -99.0),
            score: 0,
            hi: 25000 + r.below(40) * 250,
        }
    }
}

impl Effect for Shooter {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, new_beat) = self.clock.tick(c);
        let wave = (bt / 8.0).floor() as i64;
        if wave != self.wave {
            self.wave = wave;
            self.kind = hash(self.seed ^ wave as u32) % 4;
            self.alive = [true; FOES];
            self.boom = [-99.0; FOES];
        }
        let tw = bt - wave as f32 * 8.0;
        let speed = 1.0 + 2.5 * a.bass;
        self.scroll += c.dt * 60.0 * speed;

        // aim at the leftmost enemy in front of the ship; fire on the beat
        let sx = 18.0;
        let target = (0..FOES)
            .filter(|&i| self.alive[i])
            .map(|i| (i, foe_pos(self.kind, i, tw, bt)))
            .filter(|(_, p)| p.0 > sx + 30.0 && p.0 < 248.0)
            .min_by(|p, q| p.1.0.total_cmp(&q.1.0));
        let want = target.map(|t| t.1.1).unwrap_or(88.0 + (c.t * 0.9).sin() * 30.0);
        self.ship_y += (want - self.ship_y) * (1.0 - (-c.dt * 9.0).exp());
        if new_beat {
            if let Some((i, p)) = target {
                self.alive[i] = false;
                self.boom[i] = bt;
                self.boom_at[i] = p;
                self.laser = (sx + 16.0, self.ship_y, p.0, p.1, bt);
                self.score += 50 + self.kind * 25;
                self.hi = self.hi.max(self.score);
            }
        }

        let s = &mut self.scr;
        s.clear(at(WHITE | BR, BLACK));
        for st in self.stars.iter_mut() {
            st.0 -= c.dt * 60.0 * speed * (0.5 + st.2 as f32 * 0.8);
            if st.0 < 0.0 {
                st.0 += 256.0;
            }
            s.plot(st.0 as i32, st.1);
            if st.2 == 2 {
                s.plot(st.0 as i32 + 1, st.1);
            }
        }
        // scrolling ground; its colour bands jump a whole cell at a time
        for x in 0..SW {
            let top = SH - ground(self.scroll + x as f32);
            for y in top..SH {
                let d = y - top;
                if d < 2 || (d > 3 && (x + (y >> 1) + self.scroll as i32) % 3 == 0) {
                    s.plot(x, y);
                }
            }
        }
        let inks = [GREEN | BR, YELLOW | BR, accent(c.pal), RED | BR];
        for cx in 0..32 {
            let wc = (self.scroll as i32 + cx * 8) >> 3;
            for cy in 19..24 {
                s.cell(cx, cy, at(inks[((wc >> 2) & 3) as usize], BLACK));
            }
        }
        // the formation
        let fcol = [MAGENTA, CYAN, GREEN, YELLOW, RED];
        for i in 0..FOES {
            let (x, y) = foe_pos(self.kind, i, tw, bt);
            let kind = (self.kind as usize + (wave as usize)) % 3;
            if self.alive[i] {
                let sp = &self.foes[kind][((bt * 2.0) as i32 % 2) as usize];
                if x > -16.0 && x < 270.0 {
                    let ink = fcol[(wave.rem_euclid(5) as usize + i / 5) % 5] | BR;
                    s.sprite(sp, x as i32 - sp.w / 2, y as i32 - sp.h / 2, false, ink);
                }
            } else if bt - self.boom[i] < 0.7 {
                let age = (bt - self.boom[i]) / 0.7;
                let (bx, by) = self.boom_at[i];
                s.burst(bx - age * 20.0, by, age, i as u32 * 13 + wave as u32);
                let ink = [WHITE, YELLOW, RED][(age * 3.0) as usize % 3] | BR;
                s.ink(bx as i32 - 18 - (age * 20.0) as i32, by as i32 - 16, 32, 32, ink);
            }
        }
        // laser and ship
        if bt - self.laser.4 < 0.2 {
            let (x0, y0, x1, y1, _) = self.laser;
            s.line((x0, y0 - 1.0), (x1, y1 - 1.0), false);
            s.line((x0, y0), (x1, y1), false);
            s.ink(x0 as i32, y0.min(y1) as i32 - 1, (x1 - x0) as i32, (y1 - y0).abs() as i32 + 2, CYAN | BR);
        }
        let sy = self.ship_y as i32 - 4;
        s.sprite(&self.ship, sx as i32, sy, false, WHITE | BR);
        let flame = 2 + (hash((c.t * 30.0) as u32) % 4) as i32 + (a.treble * 4.0) as i32;
        for k in 0..flame {
            s.plot(sx as i32 - 1 - k, sy + 3 + (k % 2));
        }
        s.ink(sx as i32 - 8, sy + 3, 8, 2, YELLOW | BR);

        s.cells(0, 0, 32, 2, at(WHITE | BR, BLACK));
        s.clear_rows(0, 2);
        s.print(0, 0, "1UP", None);
        s.num(4, 0, self.score, 6, Some(at(YELLOW | BR, BLACK)));
        s.print(18, 0, "HI", None);
        s.num(21, 0, self.hi, 6, Some(at(CYAN | BR, BLACK)));
        s.print(0, 1, "WAVE", Some(at(MAGENTA | BR, BLACK)));
        s.num(5, 1, (self.wave.rem_euclid(100)) as u32 + 1, 2, Some(at(MAGENTA | BR, BLACK)));
        s.print(9, 1, "SHIELD", Some(at(GREEN | BR, BLACK)));
        meter(s, 16, 1, 8, a.bass);
        for k in 0..3 {
            s.draw(&self.life, 25 * 8 + k * 10, 9, false);
        }
        s.ink(200, 8, 56, 8, RED | BR);
        let bc = zx(if bt - self.laser.4 < 0.08 { CYAN } else { BLUE });
        border(fb, |_| bc);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}

// ================================================================ 5. TAPE LOADER

/// The original picture: a UFO over a Swedish lake at night, pines and a red cottage.
fn ufo_picture(seed: u32) -> Scr {
    let mut p = Scr::new();
    for cy in 0..24 {
        let a = match cy {
            0..=3 => at(WHITE, BLACK),
            4..=6 => at(BLACK, BLUE),
            7..=11 => at(WHITE, BLUE),
            12..=14 => at(GREEN, BLUE),
            15 => at(BLACK, GREEN),
            _ => at(CYAN, BLUE),
        };
        p.cells(0, cy, 32, 1, a);
    }
    // night fading into blue
    p.rect(0, 32, 256, 24, |x, y| match (y - 32) / 8 {
        0 => (x + y) % 4 != 0,
        1 => (x + y) % 2 == 0,
        _ => x % 2 == 0 && y % 2 == 0,
    });
    let mut r = Rng::new(seed ^ 0x5EED);
    for _ in 0..46 {
        let (x, y) = (r.below(256) as i32, r.below(34) as i32);
        p.plot(x, y);
        if r.below(6) == 0 {
            p.plot(x - 1, y);
            p.plot(x + 1, y);
            p.plot(x, y - 1);
            p.plot(x, y + 1);
        }
    }
    // the moon
    p.oval(214.0, 17.0, 10.0, 10.0, |_, _| true);
    p.oval(210.0, 14.0, 3.0, 3.0, |x, y| (x + y) % 2 == 0);
    p.oval(219.0, 21.0, 2.0, 2.0, |_, _| false);
    p.cells(25, 0, 4, 4, at(YELLOW | BR, BLACK));
    // far shore: a jagged line of spruce
    let mut x0: i32 = -4;
    let mut tops = [119i32; 256];
    while x0 < 260 {
        let h = 8 + (hash(x0 as u32 * 7 + seed) % 15) as i32;
        for x in (x0 - 12).max(0)..(x0 + 12).min(256) {
            let t = 119 - (h as f32 - (x - x0).abs() as f32 * 2.2).max(0.0) as i32;
            tops[x as usize] = tops[x as usize].min(t);
        }
        x0 += 6 + (hash(x0 as u32 + 99) % 6) as i32;
    }
    for x in 0..256 {
        for y in tops[x as usize].min(112)..120 {
            p.plot(x, y);
        }
    }
    // the shore and the lake
    p.rect(0, 120, 256, 8, |x, y| (x * 3 + y * 5) % 7 == 0);
    for _ in 0..170 {
        let y = 130 + (r.f().powf(0.7) * 60.0) as i32;
        let x = r.below(256) as i32;
        let len = 2 + (y - 128) / 10 + r.below(4) as i32;
        p.rect(x, y, len, 1, |_, _| true);
    }
    // the beam, reaching down to the water
    p.poly(&[(110.0, 60.0), (130.0, 60.0), (156.0, 142.0), (84.0, 142.0)], false, |x, y| (x + y) % 4 == 0 || (y % 3 == 0 && x % 2 == 0));
    p.oval(120.0, 142.0, 38.0, 4.0, |x, _| x % 3 != 0);
    for cy in 8..=17 {
        let k = ((cy * 8 + 4) as f32 - 60.0) / 82.0;
        let half = 10.0 + 26.0 * k;
        p.each_cell(((120.0 - half) as i32).max(0), cy * 8, (half * 2.0) as i32, 8, |a| (a & 0b1011_1000) | YELLOW | 0x40);
    }
    // the saucer
    p.oval(120.0, 44.0, 11.0, 8.0, |x, y| y <= 44 && ((x + y) % 2 == 0 || y < 38));
    p.oval(120.0, 49.0, 30.0, 5.0, |x, y| !(y == 49 && x % 6 < 2));
    p.oval(120.0, 55.0, 18.0, 2.5, |_, _| true);
    for k in -3..=3 {
        p.rect(120 + k * 7 - 1, 58, 2, 2, |_, _| true);
    }
    p.cells(13, 4, 4, 1, at(CYAN | BR, BLUE));
    p.cells(11, 5, 8, 2, at(CYAN | BR, BLUE));
    p.cells(13, 7, 5, 1, at(YELLOW | BR, BLUE));
    // its reflection in the lake
    for k in 0..5 {
        let y = 160 + k * 3;
        let half = 26 - k * 4;
        p.rect(120 - half, y, half * 2, 1, |x, _| (x + k * 3) % 6 < 4);
    }
    p.cells(11, 20, 8, 2, at(WHITE | BR, BLUE));
    // the red cottage with white corners and a lit window
    p.rect(24, 104, 48, 24, |x, y| x % 6 == 0 && y % 2 == 0);
    p.rect(24, 104, 2, 24, |_, _| true);
    p.rect(70, 104, 2, 24, |_, _| true);
    p.rect(24, 104, 48, 1, |_, _| true);
    p.cells(3, 13, 6, 3, at(WHITE, RED));
    p.poly(&[(48.0, 89.0), (78.0, 104.0), (18.0, 104.0)], false, |_, _| true);
    p.rect(60, 86, 4, 10, |_, _| true);
    p.cells(2, 11, 8, 2, at(BLACK, BLUE));
    for wx in [4, 7] {
        p.tile(wx, 13, &[0, 0xFF, 0x99, 0x99, 0xFF, 0x99, 0x99, 0xFF]);
        p.cell(wx, 13, at(BLACK, YELLOW | BR));
    }
    p.tile(6, 14, &[0x7E, 0x42, 0x42, 0x42, 0x4A, 0x42, 0x42, 0x42]);
    p.tile(6, 15, &[0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x7E]);
    // the jetty
    p.rect(72, 129, 42, 3, |_, _| true);
    for x in (76..114).step_by(9) {
        p.rect(x, 132, 1, 4, |_, _| true);
    }
    p.cells(9, 16, 6, 1, at(YELLOW, BLUE));
    // a headland with two big pines in front
    p.cells(25, 8, 7, 9, at(BLACK, BLUE));
    p.cells(24, 17, 8, 7, at(GREEN, BLUE));
    p.rect(192, 136, 64, 56, |_, _| false);
    p.poly(&[(194.0, 192.0), (200.0, 160.0), (206.0, 148.0), (216.0, 141.0), (234.0, 138.0), (256.0, 137.0), (256.0, 192.0)], false, |x, y| (x * 5 + y * 3) % 11 != 0);
    for (px, top) in [(214.0f32, 70.0f32), (240.0, 62.0)] {
        for j in 0..6 {
            let y = top + j as f32 * 12.0;
            let half = 5.0 + j as f32 * 3.5;
            p.poly(&[(px, y), (px + half, y + 16.0), (px - half, y + 16.0)], false, |_, _| true);
        }
        p.rect(px as i32 - 2, (top + 86.0) as i32, 4, (136.0 - top - 86.0).max(0.0) as i32 + 4, |_, _| true);
        p.rect(px as i32 - 1, 140, 3, 10, |_, _| false);
    }
    p
}

/// Spectrum memory order: screen thirds, then pixel row inside the character
/// row, then character row; turns a loading offset into a bitmap index.
fn mem_order(i: usize) -> usize {
    let (third, scan, line, col) = (i >> 11, (i >> 8) & 7, (i >> 5) & 7, i & 31);
    (third * 64 + line * 8 + scan) * 32 + col
}

/// A tape loading screen: pilot tone stripes, a header, then data stripes as
/// the picture arrives in the machine's own memory order, attributes last.
struct TapeLoader {
    scr: Scr,
    pic: Scr,
    clock: Clock,
    stage: u8,
    st: f32,
    bytes: f32,
    done: usize,
    roll: f32,
    frame: u32,
    rows: [u32; H],
}

impl TapeLoader {
    fn new(seed: u32) -> Self {
        let mut s = TapeLoader {
            scr: Scr::new(),
            pic: ufo_picture(seed),
            clock: Clock::default(),
            stage: 0,
            st: 0.0,
            bytes: 0.0,
            done: 0,
            roll: 0.0,
            frame: 0,
            rows: [0; H],
        };
        s.restart();
        s
    }

    fn restart(&mut self) {
        self.scr.clear(at(BLACK, WHITE));
        self.stage = 0;
        self.st = 0.0;
        self.bytes = 0.0;
        self.done = 0;
    }
}

impl Effect for TapeLoader {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (_, _) = self.clock.tick(c);
        let db = c.dt / a.period.max(0.1);
        self.st += db;
        self.frame = self.frame.wrapping_add(1);
        let rate = 330.0 * (0.55 + 0.9 * a.bass);
        match self.stage {
            0 if self.st > 3.0 => {
                self.stage = 1;
                self.st = 0.0;
            }
            1 if self.st > 0.6 => {
                self.stage = 2;
                self.st = 0.0;
                self.scr.print(0, 0, "PROGRAM: UFOSCENE", None);
            }
            2 if self.st > 2.5 => {
                self.stage = 3;
                self.st = 0.0;
            }
            3 => {
                self.bytes += db * rate;
                let to = (self.bytes as usize).min(6144);
                for i in self.done..to {
                    let j = mem_order(i);
                    self.scr.bits[j] = self.pic.bits[j];
                }
                self.done = to;
                if to >= 6144 {
                    self.stage = 4;
                    self.bytes = 0.0;
                    self.done = 0;
                }
            }
            4 => {
                self.bytes += db * rate;
                let to = (self.bytes as usize).min(768);
                self.scr.attr[self.done..to].copy_from_slice(&self.pic.attr[self.done..to]);
                self.done = to;
                if to >= 768 {
                    self.stage = 5;
                    self.st = 0.0;
                }
            }
            5 => {
                if self.st > 14.0 {
                    self.restart();
                } else {
                    // the picture comes alive: lights chase, the beam pulses with the kick
                    self.scr.attr.copy_from_slice(&self.pic.attr);
                    for cx in 13..18 {
                        let ink = [YELLOW, RED, MAGENTA, CYAN, GREEN, WHITE][(c.beats as usize + cx as usize) % 6];
                        self.scr.cell(cx, 7, at(ink | BR, BLUE));
                    }
                    if a.pulse < 0.25 {
                        for cy in 8..12 {
                            for cx in 9..22 {
                                let i = (cy * 32 + cx) as usize;
                                if self.pic.attr[i] & 7 == YELLOW {
                                    self.scr.attr[i] = at(WHITE, BLUE);
                                }
                            }
                        }
                    }
                    if a.treble > 0.5 {
                        self.scr.cell((self.frame / 8 % 24) as i32, (self.frame / 3 % 4) as i32, at(CYAN | BR, BLACK));
                    }
                }
            }
            _ => {}
        }

        // the border: pilot tone, data, or calm once loaded
        match self.stage {
            0 | 2 => {
                let thick = 8.0 + 5.0 * a.bass;
                self.roll += c.dt * 60.0 * (0.6 + 2.0 * a.bass);
                for (y, row) in self.rows.iter_mut().enumerate() {
                    let k = ((y as f32 + self.roll) / thick) as i32;
                    *row = zx(if k % 2 == 0 { RED } else { CYAN });
                }
            }
            1 | 3 | 4 => {
                let mut bit = (hash(self.frame) % 49152) as usize;
                let mut y = 0usize;
                let scale = 0.7 + 0.8 * a.bass;
                while y < H {
                    let byte = self.pic.bits[(bit / 8) % 6144];
                    let one = (byte >> (7 - bit % 8)) & 1 == 1;
                    let w = ((if one { 4.0 } else { 2.0 }) * scale).round().max(1.0) as usize;
                    for k in 0..w * 2 {
                        if y + k < H {
                            self.rows[y + k] = zx(if k < w { BLUE } else { YELLOW });
                        }
                    }
                    y += w * 2;
                    bit += 1;
                }
            }
            _ => self.rows.fill(zx(BLACK)),
        }
        let rows = &self.rows;
        border(fb, |y| rows[y as usize]);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}

// ================================================================ 6. ADVENTURE

struct Place {
    title: &'static str,
    desc: &'static str,
    cmd: &'static str,
    resp: &'static str,
}

const PLACES: [Place; 4] = [
    Place {
        title: "DEN KALLA FESTSALEN",
        desc: "DU STÅR I EN KALL FESTSAL. HUNDRA GAMLA TV-APPARATER FLIMRAR I TAKT OCH NÅGON HAR GLÖMT EN TERMOS KAFFE.",
        cmd: "DANCE",
        resp: "THE SCREENS FLICKER IN TIME. A CROWD CHEERS!",
    },
    Place {
        title: "THE NORTHERN FOREST",
        desc: "TALL PINES CREAK IN THE FROST. ABOVE THEM GREEN NORTHERN LIGHTS RIPPLE LIKE CURTAINS.",
        cmd: "LOOK UP",
        resp: "NORRSKENET DANSAR MED MUSIKEN. DU LER.",
    },
    Place {
        title: "SJÖN I NATT",
        desc: "EN SVART SJÖ UNDER MÅNEN. EN RÖD STUGA LYSER PÅ ANDRA SIDAN. NÅGOT BLINKAR OVANFÖR VATTNET...",
        cmd: "GO NORTH",
        resp: "YOU WADE IN. IT IS VERY COLD. BRRR!",
    },
    Place {
        title: "THE COMPUTER ROOM",
        desc: "RUBBER KEYS, A TAPE DECK AND A SMALL TV. A NOTE ON THE WALL SAYS: SPOLA TILLBAKA BANDET.",
        cmd: "TYPE RUN",
        resp: "DATORN SURRAR. SKÄRMEN FYLLS AV FÄRGER!",
    },
];

fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in s.split(' ') {
        let wl = w.chars().count();
        if !cur.is_empty() && cur.chars().count() + 1 + wl > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A text adventure: a location picture on top that changes every 16 beats,
/// the description typing itself below, then a command and the reply.
struct Adventure {
    scr: Scr,
    clock: Clock,
    offset: usize,
    accent: u8,
    desc: Vec<Vec<String>>,
    resp: Vec<Vec<String>>,
    place: i64,
    typed: f32,
    desc_done: f32,
    resp_typed: f32,
    tp: f32,
    small_ufo: Spr,
}

impl Adventure {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        Adventure {
            scr: Scr::new(),
            clock: Clock::default(),
            offset: r.below(4) as usize,
            accent: 0,
            desc: PLACES.iter().map(|p| wrap(p.desc, 32)).collect(),
            resp: PLACES.iter().map(|p| wrap(p.resp, 30)).collect(),
            place: i64::MIN,
            typed: 0.0,
            desc_done: -1.0,
            resp_typed: 0.0,
            tp: 0.0,
            small_ufo: spr(&art::SMALL_UFO),
        }
    }

    fn picture(&mut self, k: usize, t: f32, a: &Audio, bt: f32) {
        let s = &mut self.scr;
        s.clear_rows(0, 12);
        match k {
            0 => {
                // the party hall
                s.cells(0, 0, 32, 7, at(CYAN, BLUE));
                s.cells(0, 7, 32, 2, at(MAGENTA | BR, BLACK));
                s.cells(0, 9, 32, 3, at(CYAN | BR, BLACK));
                for x in (0..256).step_by(16) {
                    s.line((x as f32, 0.0), (x as f32, 55.0), true);
                }
                s.cells(9, 1, 14, 1, at(YELLOW | BR, RED));
                s.print(9, 1, "* * FESTEN * *", None);
                for k in -10..=10 {
                    s.ln(128 + k * 7, 56, 128 + k * 34, 95);
                }
                for y in [58, 62, 68, 76, 86] {
                    s.ln(0, y, 255, y);
                }
                s.rect(0, 48, 256, 2, |_, _| true);
                for k in 0..6 {
                    let cx = 1 + k * 5;
                    let (x, y) = (cx * 8, 24);
                    s.rect(x + 5, y - 2, 22, 21, |_, _| false);
                    s.ln(x + 6, y - 1, x + 25, y - 1);
                    s.ln(x + 6, y - 1, x + 6, y + 18);
                    s.ln(x + 25, y - 1, x + 25, y + 18);
                    s.rect(x + 6, y + 16, 20, 3, |_, _| true);
                    s.rect(x + 22, y + 17, 2, 1, |_, _| false);
                    s.cells(cx, 2, 4, 1, at(WHITE | BR, BLUE));
                    s.cells(cx, 5, 4, 1, at(WHITE | BR, BLUE));
                    s.cell(cx, 3, at(WHITE | BR, BLUE));
                    s.cell(cx, 4, at(WHITE | BR, BLUE));
                    s.cell(cx + 3, 3, at(WHITE | BR, BLUE));
                    s.cell(cx + 3, 4, at(WHITE | BR, BLUE));
                    let paper = [BLUE, RED, MAGENTA, GREEN, CYAN, YELLOW][(c_beats(bt) + k as usize + (a.treble * 3.0) as usize) % 6];
                    s.cells(cx + 1, 3, 2, 2, at(WHITE | BR, paper));
                    let scan = (t * 30.0) as i32;
                    for yy in 24..40 {
                        if (yy + scan) % 3 == 0 {
                            s.rect(x + 8, yy, 16, 1, |xx, _| (xx + yy) % 2 == 0);
                        }
                    }
                    for xx in 0..16 {
                        let wy = 32.0 + (xx as f32 * 0.7 + t * 9.0 + k as f32).sin() * a.mid * 6.0;
                        s.plot(x + 8 + xx, wy as i32);
                    }
                }
                for k in 0..7 {
                    let x = 18.0 + k as f32 * 36.0 + (t * 0.8 + k as f32).sin() * 4.0;
                    let bob = ((bt + k as f32 * 0.5) * PI).sin().abs() * 4.0 * (0.4 + a.bass);
                    let y = 78.0 - bob;
                    s.oval(x, y, 4.0, 4.5, |_, _| true);
                    s.poly(&[(x - 8.0, 96.0), (x - 6.0, y + 6.0), (x + 6.0, y + 6.0), (x + 8.0, 96.0)], false, |_, _| true);
                    if (c_beats(bt) + k) % 2 == 0 {
                        s.ln(x as i32 - 6, y as i32 + 7, x as i32 - 11, y as i32 - 6);
                        s.ln(x as i32 + 6, y as i32 + 7, x as i32 + 11, y as i32 - 6);
                    }
                }
            }
            1 => {
                // pines under the northern lights
                s.cells(0, 0, 32, 10, at(GREEN | BR, BLACK));
                s.cells(0, 10, 32, 2, at(BLACK, WHITE | BR));
                for cy in 0..2 {
                    s.cells(0, cy, 32, 1, at(MAGENTA | BR, BLACK));
                }
                let band = (t * 2.0) as i32;
                for cx in 0..32 {
                    if (cx + band).rem_euclid(9) < 2 {
                        s.cells(cx, 2, 1, 5, at(CYAN | BR, BLACK));
                    }
                }
                let ft = (t * 6.0) as u32;
                for x in 0..256 {
                    let xf = x as f32;
                    let top = 10.0 + 8.0 * (xf * 0.025 + t * 0.6).sin() + 5.0 * (xf * 0.061 - t * 1.1).sin();
                    let len = 18.0 + 16.0 * ((xf * 0.04 + t * 0.9).sin() * 0.5 + 0.5) * (0.5 + a.mid);
                    let streak = 0.35 + (hash(x as u32 / 2 + ft * 1000) & 15) as f32 / 24.0;
                    for y in top as i32..(top + len * streak) as i32 {
                        let k = (y as f32 - top) / len;
                        if k < 0.25 || (k < 0.6 && (x + y) % 2 == 0) || (x % 2 == 0 && y % 3 == 0) {
                            s.plot(x, y);
                        }
                    }
                }
                for i in 0..40u32 {
                    let h = hash(i * 977 + 5);
                    let (x, y) = ((h % 256) as i32, ((h >> 8) % 60) as i32);
                    if (h >> 20) % 3 != 0 || a.treble > 0.5 {
                        s.plot(x, y);
                    }
                }
                for k in 0..10 {
                    let px = 8.0 + k as f32 * 27.0 + (hash(k) % 9) as f32;
                    let top = 34.0 + (hash(k + 50) % 18) as f32;
                    let layers = 5;
                    for j in 0..layers {
                        let y = top + j as f32 * 11.0;
                        let half = 4.0 + j as f32 * 3.0;
                        s.poly(&[(px, y), (px + half, y + 14.0), (px - half, y + 14.0)], false, |x, y| y % 4 != 0 || (x + y) % 2 == 0);
                    }
                    s.rect(px as i32 - 1, (top + 60.0) as i32, 3, 40, |_, _| true);
                    for cy in ((top + 6.0) as i32 / 8)..10 {
                        let half = ((cy * 8) as f32 - top) / 11.0 * 3.0 + 3.0;
                        for cx in ((px - half * 0.6) as i32 / 8)..=((px + half * 0.6) as i32 / 8) {
                            s.cell(cx, cy, at(GREEN, BLACK));
                        }
                    }
                }
                s.rect(0, 80, 256, 16, |x, y| (x * 7 + y * 13) % 23 == 0 || (y == 80 + ((x / 20) % 3)));
            }
            2 => {
                // the lake at night
                s.cells(0, 0, 32, 6, at(WHITE, BLACK));
                s.cells(0, 6, 32, 2, at(BLUE | BR, BLACK));
                s.cells(0, 8, 32, 4, at(CYAN, BLUE));
                for i in 0..36u32 {
                    let h = hash(i * 131 + 9);
                    s.plot((h % 256) as i32, ((h >> 8) % 44) as i32);
                }
                s.oval(40.0, 18.0, 8.0, 8.0, |_, _| true);
                s.oval(44.0, 15.0, 7.0, 7.0, |_, _| false);
                s.cells(3, 1, 3, 2, at(YELLOW | BR, BLACK));
                for x in 0..256 {
                    let top = 52.0 + 4.0 * (x as f32 * 0.05).sin() + 3.0 * (x as f32 * 0.13 + 1.0).sin();
                    for y in top as i32..64 {
                        s.plot(x, y);
                    }
                }
                s.rect(176, 48, 24, 16, |_, _| false);
                s.cells(22, 7, 3, 1, at(YELLOW | BR, RED));
                s.poly(&[(188.0, 42.0), (202.0, 56.0), (174.0, 56.0)], false, |_, _| true);
                s.rect(182, 58, 3, 3, |_, _| true);
                s.rect(191, 58, 3, 3, |_, _| true);
                for y in (66..96).step_by(2) {
                    let half = 2 + (y - 64) / 5;
                    let off = ((y as f32 * 0.5 + t * 4.0).sin() * (1.0 + a.bass * 3.0)) as i32;
                    s.rect(40 - half + off, y, half * 2, 1, |x, _| x % 3 != 0);
                }
                s.cells(3, 8, 4, 4, at(YELLOW | BR, BLUE));
                for i in 0..40u32 {
                    let h = hash(i * 71 + 3);
                    let (x, y) = ((h % 256) as i32, 66 + ((h >> 8) % 30) as i32);
                    let sh = ((t * 10.0) as i32 + i as i32) % 5;
                    s.rect(x + sh, y, 4 + (y - 64) / 8, 1, |_, _| true);
                }
                let ux = 150.0 + (t * 0.7).sin() * 40.0;
                let uy = 24.0 + (t * 1.3).sin() * 4.0;
                let ink = [YELLOW, MAGENTA, CYAN, GREEN][c_beats(bt) % 4] | BR;
                s.sprite(&self.small_ufo, ux as i32 - 8, uy as i32, false, ink);
                if a.pulse > 0.4 {
                    s.rect(ux as i32 - 6, 70, 12, 1, |x, _| x % 2 == 0);
                    s.ink(ux as i32 - 8, 70, 16, 1, ink);
                }
            }
            _ => {
                // the computer room
                s.cells(0, 0, 32, 7, at(BLUE, CYAN));
                s.cells(0, 7, 32, 5, at(YELLOW, RED));
                s.rect(0, 0, 256, 56, |x, y| x % 12 == 0 && y % 2 == 0);
                s.rect(0, 56, 256, 40, |x, y| (y % 6 == 0 && (x / 9 + y) % 5 != 0) || (x + y * 3) % 47 == 0);
                s.rect(0, 56, 256, 1, |_, _| true);
                // the TV with a tiny loading screen
                s.rect(80, 4, 96, 52, |_, _| false);
                s.cells(10, 0, 12, 7, at(WHITE | BR, BLACK));
                s.rect(82, 6, 92, 44, |x, y| x == 82 || x == 173 || y == 6 || y == 49);
                s.rect(98, 52, 60, 4, |_, _| true);
                s.cells(12, 2, 8, 4, at(YELLOW | BR, BLUE));
                let seed = (t * 50.0) as u32;
                let mut y = 16;
                let mut on = false;
                while y < 48 {
                    let w = 1 + (hash(seed + y as u32) % 4) as i32;
                    if on {
                        s.rect(96, y, 64, w.min(48 - y), |_, _| true);
                    }
                    on = !on;
                    y += w;
                }
                s.rect(112, 24, 32, 16, |_, _| false);
                s.cells(14, 3, 4, 2, at(BLACK, WHITE | BR));
                s.print(14, 3, "UFO!", None);
                let prog = ((t * 3.0) as i32 % 32).max(1);
                s.rect(112, 34, prog, 3, |x, _| x % 2 == 0);
                // keyboard
                s.rect(16, 66, 88, 20, |x, y| x == 16 || x == 103 || y == 66 || y == 85);
                for ky in 0..3 {
                    for kx in 0..10 {
                        let (x, y) = (20 + kx * 8 + ky * 2, 69 + ky * 5);
                        s.rect(x, y, 6, 3, |_, _| true);
                    }
                }
                s.cells(2, 8, 11, 3, at(WHITE | BR, BLACK));
                // tape deck with turning reels
                s.rect(136, 64, 80, 28, |x, y| x == 136 || x == 215 || y == 64 || y == 91);
                s.cells(17, 8, 10, 4, at(WHITE | BR, BLACK));
                s.rect(150, 68, 52, 14, |x, y| x == 150 || x == 201 || y == 68 || y == 81);
                let ang = bt * PI * 0.5;
                for cx in [164.0f32, 188.0] {
                    s.oval(cx, 75.0, 5.0, 5.0, |_, _| true);
                    s.oval(cx, 75.0, 3.5, 3.5, |_, _| false);
                    for k in 0..3 {
                        let q = ang + k as f32 * TAU / 3.0;
                        s.line((cx, 75.0), (cx + q.cos() * 4.0, 75.0 + q.sin() * 4.0), false);
                    }
                }
                for k in 0..5 {
                    s.rect(142 + k * 14, 85, 10, 4, |_, _| true);
                }
                // LEDs blink on the beat
                let on = c_beats(bt) % 2 == 0;
                s.rect(222, 74, 4, 3, |_, _| on);
                s.rect(232, 74, 4, 3, |_, _| !on);
                s.cell(27, 9, at(RED | BR, BLACK));
                s.cell(29, 9, at(GREEN | BR, BLACK));
                // a poster and a window
                s.rect(8, 8, 56, 32, |_, _| false);
                s.cells(1, 1, 7, 4, at(BLACK, YELLOW | BR));
                s.rect(8, 8, 56, 32, |x, y| x == 8 || x == 63 || y == 8 || y == 39);
                s.oval(36.0, 20.0, 14.0, 3.0, |_, _| true);
                s.oval(36.0, 16.0, 5.0, 4.0, |x, y| y < 17 && (x + y) % 2 == 0);
                s.print(3, 3, "UFO", None);
                s.rect(192, 8, 56, 40, |_, _| false);
                s.cells(24, 1, 7, 5, at(WHITE | BR, BLACK));
                s.rect(192, 8, 56, 40, |x, y| x == 192 || x == 247 || y == 8 || y == 47 || x == 219 || y == 27);
                for i in 0..12u32 {
                    let h = hash(i * 31 + 7);
                    s.plot(194 + (h % 52) as i32, 10 + ((h >> 8) % 36) as i32);
                }
            }
        }
        s.frame(0, 0, 256, 96);
    }
}

fn c_beats(bt: f32) -> usize {
    bt.max(0.0) as usize
}

impl Effect for Adventure {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, _) = self.clock.tick(c);
        self.accent = accent(c.pal);
        let place = (bt / 16.0).floor() as i64;
        let db = c.dt / a.period.max(0.1);
        if place != self.place {
            self.place = place;
            self.typed = 0.0;
            self.desc_done = -1.0;
            self.resp_typed = 0.0;
            self.tp = 0.0;
        }
        self.tp += db;
        let k = (place.rem_euclid(4) as usize + self.offset) % 4;
        let p = &PLACES[k];
        let rate = 16.0 * (0.8 + 0.5 * a.mid);
        let desc_len: usize = self.desc[k].iter().map(|l| l.chars().count()).sum();
        if self.tp > 0.6 {
            self.typed += db * rate;
        }
        if self.typed as usize >= desc_len && self.desc_done < 0.0 {
            self.desc_done = self.tp;
        }
        // the command types itself one letter per half beat, then the reply
        let cmd_len = p.cmd.chars().count();
        let since = if self.desc_done < 0.0 { -1.0 } else { self.tp - self.desc_done - 1.0 };
        let cmd_typed = if since < 0.0 { 0 } else { ((since * 2.0) as usize).min(cmd_len) };
        let resp_start = cmd_len as f32 / 2.0 + 1.0;
        if since > resp_start {
            self.resp_typed += db * rate;
        }

        self.picture(k, c.t, a, bt);
        // reveal the picture top-down as it is "drawn"
        let reveal = (self.tp / 1.2 * 96.0) as i32;
        if reveal < 96 {
            for y in reveal.max(0)..96 {
                self.scr.bits[(y * 32) as usize..(y * 32 + 32) as usize].fill(0);
            }
            self.scr.cells(0, (reveal / 8 + 1).min(12), 32, 12 - (reveal / 8 + 1).min(12), at(WHITE, BLACK));
        }

        let s = &mut self.scr;
        s.clear_rows(12, 24);
        s.cells(0, 12, 32, 12, at(WHITE | BR, BLACK));
        s.cells(0, 12, 32, 1, at(WHITE | BR, self.accent));
        let title = p.title;
        s.print((32 - title.chars().count() as i32) / 2, 12, title, None);
        let mut row = 13;
        let mut left = self.typed as usize;
        let mut cursor: Option<(i32, i32)> = None;
        for line in &self.desc[k] {
            let n = line.chars().count();
            let show = left.min(n);
            let part: &str = match line.char_indices().nth(show) {
                Some((b, _)) => &line[..b],
                None => line,
            };
            s.print(0, row, part, None);
            if show < n && cursor.is_none() {
                cursor = Some((show as i32, row));
            }
            left -= show;
            row += 1;
            if show < n {
                break;
            }
        }
        if self.desc_done >= 0.0 {
            row += 1;
            s.print(0, row, ">", Some(at(YELLOW | BR, BLACK)));
            let part: &str = match p.cmd.char_indices().nth(cmd_typed) {
                Some((b, _)) => &p.cmd[..b],
                None => p.cmd,
            };
            s.print(2, row, part, None);
            s.cells(2, row, cmd_len as i32, 1, at(YELLOW | BR, BLACK));
            cursor = Some((2 + cmd_typed as i32, row));
            if since > resp_start {
                let mut left = self.resp_typed as usize;
                let mut all = true;
                for line in &self.resp[k] {
                    row += 1;
                    let n = line.chars().count();
                    let show = left.min(n);
                    let part: &str = match line.char_indices().nth(show) {
                        Some((b, _)) => &line[..b],
                        None => line,
                    };
                    s.print(2, row, part, None);
                    s.cells(2, row, 30, 1, at(CYAN | BR, BLACK));
                    cursor = Some((2 + show as i32, row));
                    left -= show;
                    if show < n {
                        all = false;
                        break;
                    }
                }
                if all {
                    row += 2;
                    s.print(0, row.min(23), ">", Some(at(YELLOW | BR, BLACK)));
                    cursor = Some((2, row.min(23)));
                }
            }
        }
        if let Some((cx, cy)) = cursor {
            s.tile(cx, cy, &[0, 0x7E, 0x7E, 0x7E, 0x7E, 0x7E, 0x7E, 0]);
            s.cell(cx, cy, at(WHITE | BR, BLACK) | FLASH);
        }
        let bc = zx(self.accent & 7);
        border(fb, |_| bc);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}

// ================================================================ 7. ROAD RACER

const LAP: f32 = 4200.0;
const CAR_Y: f32 = 150.0;

/// Top-down vertical racer: a winding road with kerbs, roadside trees and
/// houses, traffic to overtake on the beat, speed from the bass, a lap counter.
struct RoadRacer {
    scr: Scr,
    clock: Clock,
    rng: Rng,
    car: Spr,
    saloon: Spr,
    bend: (f32, f32),
    dist: f32,
    lane: f32,
    target: i32,
    cars: [(f32, i32, f32, u8); 5],
    passed: u32,
    lap_time: f32,
    best: f32,
    lap: u32,
}

impl RoadRacer {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let bend = (rng.range(28.0, 48.0), rng.range(12.0, 26.0));
        let mut cars = [(0.0, 0, 0.0, 0u8); 5];
        for (i, car) in cars.iter_mut().enumerate() {
            *car = (60.0 + i as f32 * 70.0, rng.below(3) as i32 - 1, rng.range(40.0, 75.0), [CYAN, MAGENTA, GREEN, WHITE, RED][i] | BR);
        }
        RoadRacer {
            scr: Scr::new(),
            clock: Clock::default(),
            rng,
            car: spr(&art::RACER),
            saloon: spr(&art::SALOON),
            bend,
            dist: 0.0,
            lane: 0.0,
            target: 0,
            cars,
            passed: 0,
            lap_time: 0.0,
            best: 0.0,
            lap: 0,
        }
    }

    fn lane_free(&self, lane: i32, from: f32, to: f32) -> bool {
        (-1..=1).contains(&lane) && self.cars.iter().all(|c| c.1 != lane || c.0 - self.dist < from || c.0 - self.dist > to)
    }
}

impl Effect for RoadRacer {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, new_beat) = self.clock.tick(c);
        let speed = 95.0 + 175.0 * a.bass;
        self.dist += speed * c.dt;
        self.lap_time += c.dt;
        let lap = (self.dist / LAP) as u32;
        if lap != self.lap {
            self.lap = lap;
            if self.best == 0.0 || self.lap_time < self.best {
                self.best = self.lap_time;
            }
            self.lap_time = 0.0;
        }
        for i in 0..5 {
            let (mut wy, mut lane, mut sp, mut ink) = self.cars[i];
            wy += sp * c.dt;
            if wy - self.dist < -70.0 {
                self.passed += 1;
                wy = self.dist + 200.0 + self.rng.range(0.0, 160.0);
                lane = self.rng.below(3) as i32 - 1;
                sp = self.rng.range(40.0, 80.0);
                ink = [CYAN, MAGENTA, GREEN, WHITE, RED, BLUE][self.rng.below(6) as usize] | BR;
            }
            self.cars[i] = (wy, lane, sp, ink);
        }
        // overtake on the beat: leave a lane that has a car ahead
        if new_beat {
            let blocked = !self.lane_free(self.target, -10.0, 110.0);
            if blocked || self.rng.below(3) == 0 {
                let order = if self.rng.below(2) == 0 { [-1, 1] } else { [1, -1] };
                for d in order {
                    if self.lane_free(self.target + d, -34.0, 100.0) {
                        self.target += d;
                        break;
                    }
                }
            }
        }
        self.lane += (self.target as f32 - self.lane) * (1.0 - (-c.dt * 7.0).exp());

        let s = &mut self.scr;
        s.clear(at(BLACK, GREEN));
        let d = self.dist;
        let centre = |wy: f32| 128.0 + self.bend.0 * (wy * 0.0045).sin() + self.bend.1 * (wy * 0.0113 + 1.3).sin();
        for y in 16..SH {
            let wy = d + (CAR_Y - y as f32);
            let wyi = wy as i32;
            let cx = centre(wy);
            let (l, r) = ((cx - 40.0) as i32, (cx + 40.0) as i32);
            let stripe = (wyi >> 2) & 1 == 0;
            let dash = (wyi >> 3) & 1 == 0;
            let start = wy.rem_euclid(LAP) < 10.0;
            for x in 0..SW {
                let on = if x < l - 5 || x > r + 5 {
                    hash(x as u32 ^ (wyi as u32).wrapping_mul(0x9E37)) % 29 == 0
                } else if x < l || x > r {
                    stripe
                } else if start {
                    ((x - l) / 4 + (wyi >> 2)) % 2 == 0
                } else {
                    let m = (x as f32 - cx).abs();
                    ((m - 13.0).abs() < 1.0 && dash) || x == l + 2 || x == r - 2
                };
                if on {
                    s.plot(x, y);
                }
            }
        }
        for cy in 2..24 {
            let wy = d + (CAR_Y - (cy * 8 + 4) as f32);
            let cx = centre(wy);
            let (l, r) = (cx - 40.0, cx + 40.0);
            for col in 0..32 {
                let (x0, x1) = ((col * 8) as f32, (col * 8 + 8) as f32);
                let a = if x1 <= l - 5.0 || x0 > r + 5.0 {
                    at(BLACK, GREEN)
                } else if x0 >= l && x1 <= r + 1.0 {
                    at(WHITE | BR, BLACK)
                } else {
                    at(RED | BR, BLACK)
                };
                s.cell(col, cy, a);
                if a == at(RED | BR, BLACK) {
                    // the verge part of a kerb cell stays clear
                    for yy in cy * 8..cy * 8 + 8 {
                        for xx in col * 8..col * 8 + 8 {
                            if (xx as f32) < l - 5.0 || (xx as f32) > r + 5.0 {
                                s.set(xx, yy, false);
                            }
                        }
                    }
                }
            }
        }
        // roadside trees, pines and houses
        let k0 = ((d - 60.0) / 26.0).floor() as i32;
        for k in k0..k0 + 10 {
            let h = hash(k as u32 ^ 0xBEEF);
            let wy = k as f32 * 26.0 + (h % 10) as f32;
            let sy = CAR_Y - (wy - d);
            let cx = centre(wy);
            let off = 16.0 + ((h >> 4) % 44) as f32;
            let x = if h & 1 == 0 { cx - 40.0 - off } else { cx + 40.0 + off };
            if !(-10.0..266.0).contains(&x) || !(10.0..200.0).contains(&sy) {
                continue;
            }
            match (h >> 12) % 6 {
                0..=2 => {
                    s.oval(x, sy, 8.0, 8.0, |_, _| false);
                    s.oval(x, sy, 7.0, 7.0, |px, py| (px * 3 + py * 5) % 7 != 0 && (px + py) % 5 != 0);
                }
                3 | 4 => {
                    for ring in 0..3 {
                        let r = 7.0 - ring as f32 * 2.0;
                        for t in 0..16 {
                            let ang = t as f32 / 16.0 * TAU + ring as f32 * 0.2;
                            let rr = if t % 2 == 0 { r } else { r * 0.6 };
                            s.line((x, sy), (x + ang.cos() * rr, sy + ang.sin() * rr), false);
                        }
                    }
                }
                _ => {
                    let (hx, hy) = (x as i32 - 9, sy as i32 - 7);
                    s.rect(hx - 1, hy - 1, 20, 16, |_, _| false);
                    s.rect(hx, hy, 18, 14, |xx, yy| xx == hx || xx == hx + 17 || yy == hy || yy == hy + 13 || yy == hy + 7 || (xx - hx + yy) % 3 == 0);
                    s.tint(hx, hy, 18, 14, RED);
                }
            }
        }
        // traffic and the player
        for &(wy, lane, _, ink) in &self.cars {
            let sy = CAR_Y - (wy - d);
            if sy < 0.0 || sy > 200.0 {
                continue;
            }
            let x = centre(wy) + lane as f32 * 26.0;
            s.sprite(&self.saloon, x as i32 - 8, sy as i32 - 9, false, ink);
        }
        let px = centre(d) + self.lane * 26.0;
        let wobble = if a.pulse > 0.6 { 1 } else { 0 };
        s.sprite(&self.car, px as i32 - 8 + wobble, CAR_Y as i32 - 10, false, YELLOW | BR);

        let acc = accent(c.pal);
        s.clear_rows(0, 2);
        s.cells(0, 0, 32, 2, at(WHITE | BR, BLACK));
        s.print(0, 0, "LAP", None);
        s.num(4, 0, lap % 5 + 1, 1, Some(at(acc, BLACK)));
        s.print(5, 0, "/5", Some(at(acc, BLACK)));
        s.print(9, 0, "POS", None);
        s.num(13, 0, 8 - (self.passed / 3).min(7), 1, Some(at(YELLOW | BR, BLACK)));
        s.print(16, 0, "TIME", None);
        let t = self.lap_time as u32;
        s.num(21, 0, t / 60 % 10, 1, Some(at(CYAN | BR, BLACK)));
        s.print(22, 0, ":", Some(at(CYAN | BR, BLACK)));
        s.num(23, 0, t % 60, 2, Some(at(CYAN | BR, BLACK)));
        s.print(25, 0, ".", Some(at(CYAN | BR, BLACK)));
        s.num(26, 0, (fract(self.lap_time) * 10.0) as u32, 1, Some(at(CYAN | BR, BLACK)));
        s.print(0, 1, "SPEED", None);
        s.num(6, 1, (speed * 0.8) as u32, 3, Some(at(GREEN | BR, BLACK)));
        s.print(9, 1, "KM/H", None);
        meter(s, 14, 1, 10, (speed - 95.0) / 175.0);
        s.print(25, 1, "GEAR", None);
        s.num(30, 1, 1 + (a.bass * 4.99) as u32, 1, Some(at(YELLOW | BR, BLACK)));
        let _ = bt;
        let bc = zx(GREEN);
        border(fb, |_| bc);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}

// ================================================================ 8. WAR MAP

const HEX_W: i32 = 21;
const HEX_H: i32 = 11;

/// Centre of hex (q, r): flat-topped hexes in odd-q offset rows.
fn hex_xy(q: i32, r: i32) -> (f32, f32) {
    (10.0 + q as f32 * 12.0, 9.0 + r as f32 * 14.0 + (q & 1) as f32 * 7.0)
}

fn hex_dist(a: (i32, i32), b: (i32, i32)) -> i32 {
    let cube = |q: i32, r: i32| {
        let z = r - (q - (q & 1)) / 2;
        (q, -q - z, z)
    };
    let (x0, y0, z0) = cube(a.0, a.1);
    let (x1, y1, z1) = cube(b.0, b.1);
    (x0 - x1).abs().max((y0 - y1).abs()).max((z0 - z1).abs())
}

fn hex_neighbours(q: i32, r: i32) -> [(i32, i32); 6] {
    if q & 1 == 0 {
        [(q + 1, r - 1), (q + 1, r), (q, r - 1), (q, r + 1), (q - 1, r - 1), (q - 1, r)]
    } else {
        [(q + 1, r), (q + 1, r + 1), (q, r - 1), (q, r + 1), (q - 1, r), (q - 1, r + 1)]
    }
}

#[derive(Clone, Copy)]
struct Unit {
    side: u8,
    kind: u8,
    num: u8,
    at: (i32, i32),
    from: (i32, i32),
    strength: u8,
    hit: f32,
    dead: f32,
}

const UNIT_KINDS: [&str; 4] = ["ARMOUR", "RIFLES", "GUNS", "SCOUTS"];
/// Army names by counter colour.
fn side_name(colour: u8) -> &'static str {
    match colour {
        BLUE => "AZURE",
        RED => "EMBER",
        MAGENTA => "ORCHID",
        _ => "JADE",
    }
}

/// A turn-based desert war game: hex grid over sand with an oasis, a road and
/// ridges; two invented armies take turns moving a hex per beat, fight on the
/// kick and the artillery puffs with the treble.
struct WarMap {
    scr: Scr,
    clock: Clock,
    rng: Rng,
    palm: Spr,
    units: [Unit; 12],
    terrain: [u8; (HEX_W * HEX_H) as usize],
    oasis: (i32, i32),
    road: (f32, f32),
    colours: [u8; 2],
    puffs: [(f32, f32, f32); 6],
    fight: ((f32, f32), (f32, f32), f32),
    msg: String,
    msg2: String,
}

impl WarMap {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let oasis = (8 + rng.below(5) as i32, 3 + rng.below(4) as i32);
        let mut terrain = [0u8; (HEX_W * HEX_H) as usize];
        // a ridge running diagonally
        let (rq, rdir) = (4 + rng.below(12) as i32, if rng.below(2) == 0 { 1 } else { -1 });
        for k in 0..7 {
            let (q, r) = (rq + k * rdir / 2, 1 + k);
            if (0..HEX_W).contains(&q) && (0..HEX_H).contains(&r) {
                terrain[(r * HEX_W + q) as usize] = 3;
            }
        }
        terrain[(oasis.1 * HEX_W + oasis.0) as usize] = 1;
        let road = (rng.range(105.0, 130.0), rng.range(0.0, TAU));
        let colours = [[BLUE, RED], [BLUE, MAGENTA], [GREEN, RED]][rng.below(3) as usize];
        let mut units = [Unit { side: 0, kind: 0, num: 0, at: (0, 0), from: (0, 0), strength: 3, hit: -99.0, dead: -99.0 }; 12];
        for (i, u) in units.iter_mut().enumerate() {
            let side = (i / 6) as u8;
            let row = (i % 6) as i32 * 2 - 1 + (i / 6) as i32;
            let q = if side == 0 { 1 + (i % 2) as i32 } else { HEX_W - 2 - (i % 2) as i32 };
            u.side = side;
            u.kind = (i % 4) as u8;
            u.num = 1 + (rng.below(9) as u8) + (i as u8 % 3) * 10;
            u.at = (q, row.clamp(0, HEX_H - 1));
            u.from = u.at;
        }
        WarMap {
            scr: Scr::new(),
            clock: Clock::default(),
            rng,
            palm: spr(&art::PALM),
            units,
            terrain,
            oasis,
            road,
            colours,
            puffs: [(0.0, 0.0, -99.0); 6],
            fight: ((0.0, 0.0), (0.0, 0.0), -99.0),
            msg: "BOTH ARMIES DIG IN AT DAWN.".into(),
            msg2: "THE DESERT WAITS.".into(),
        }
    }

    fn place(&self, h: (i32, i32)) -> &'static str {
        if hex_dist(h, self.oasis) <= 2 {
            "SALTBRUNN OASIS"
        } else if self.terrain[(h.1 * HEX_W + h.0) as usize] == 3 {
            "KORP RIDGE"
        } else if (hex_xy(h.0, h.1).1 - self.road_y(hex_xy(h.0, h.1).0)).abs() < 10.0 {
            "THE DUNE ROAD"
        } else {
            "THE OPEN SAND"
        }
    }

    fn road_y(&self, x: f32) -> f32 {
        self.road.0 + 16.0 * (x * 0.017 + self.road.1).sin() + 5.0 * (x * 0.05).sin()
    }

    fn occupied(&self, h: (i32, i32)) -> bool {
        self.units.iter().any(|u| u.dead < -50.0 && u.at == h)
    }

    /// One side moves: every unit steps one hex toward its nearest enemy.
    fn move_side(&mut self, side: u8, bt: f32) {
        for i in 0..12 {
            let u = self.units[i];
            self.units[i].from = u.at;
            if u.side != side || u.dead > -50.0 {
                continue;
            }
            let enemy = self
                .units
                .iter()
                .filter(|e| e.side != side && e.dead < -50.0)
                .min_by_key(|e| hex_dist(e.at, u.at))
                .map(|e| e.at);
            let goal = enemy.unwrap_or(self.oasis);
            if hex_dist(goal, u.at) <= 1 {
                continue;
            }
            let mut best = (hex_dist(goal, u.at), u.at);
            for n in hex_neighbours(u.at.0, u.at.1) {
                if !(0..HEX_W).contains(&n.0) || !(0..HEX_H).contains(&n.1) || self.occupied(n) {
                    continue;
                }
                let d = hex_dist(goal, n) * 4 + self.rng.below(3) as i32;
                if d < best.0 * 4 {
                    best = (d / 4, n);
                }
            }
            self.units[i].at = best.1;
        }
        // the fallen come back as reinforcements on their own edge
        for i in 0..12 {
            let u = self.units[i];
            if u.dead > -50.0 && bt - u.dead > 6.0 {
                let q = if u.side == 0 { 0 } else { HEX_W - 1 };
                let h = (q, self.rng.below(HEX_H as u32) as i32);
                if !self.occupied(h) {
                    self.units[i] = Unit { at: h, from: h, strength: 3, dead: -99.0, hit: -99.0, ..u };
                }
            }
        }
    }

    fn attack(&mut self, side: u8, bt: f32, turn: u32) {
        let mut pairs = [(0usize, 0usize); 12];
        let mut n = 0;
        for i in 0..12 {
            for j in 0..12 {
                let (u, e) = (self.units[i], self.units[j]);
                if u.side == side && e.side != side && u.dead < -50.0 && e.dead < -50.0 && hex_dist(u.at, e.at) <= if u.kind == 2 { 2 } else { 1 } && n < 12 {
                    pairs[n] = (i, j);
                    n += 1;
                }
            }
        }
        if n == 0 {
            return;
        }
        let (i, j) = pairs[self.rng.below(n as u32) as usize];
        let (u, e) = (self.units[i], self.units[j]);
        self.units[i].hit = bt;
        self.units[j].hit = bt;
        let dmg = 1 + (self.rng.below(3) == 0) as u8;
        self.units[j].strength = e.strength.saturating_sub(dmg);
        self.fight = (hex_xy(u.at.0, u.at.1), hex_xy(e.at.0, e.at.1), bt);
        let place = self.place(e.at);
        self.msg = format!("TURN {turn}: {} {} {} HITS", side_name(self.colours[u.side as usize]), UNIT_KINDS[u.kind as usize], u.num);
        if self.units[j].strength == 0 {
            self.units[j].dead = bt;
            self.msg2 = format!("{} {} {} AT {place}. ROUTED!", side_name(self.colours[e.side as usize]), UNIT_KINDS[e.kind as usize], e.num);
        } else {
            self.msg2 = format!("{} {} {} AT {place}.", side_name(self.colours[e.side as usize]), UNIT_KINDS[e.kind as usize], e.num);
        }
    }
}

impl Effect for WarMap {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, new_beat) = self.clock.tick(c);
        let side = (c.beats % 2) as u8;
        let turn = c.beats / 2 + 1;
        if new_beat {
            self.move_side(side, bt);
            if a.bass > 0.45 || a.pulse > 0.8 {
                self.attack(side, bt, turn);
            }
        }
        // artillery puffs with the treble
        if a.treble > 0.4 && self.rng.f() < a.treble * c.dt * 8.0 {
            if let Some(slot) = self.puffs.iter().position(|p| bt - p.2 > 0.8) {
                let u = self.units[self.rng.below(12) as usize];
                let (x, y) = hex_xy(u.at.0, u.at.1);
                self.puffs[slot] = (x + self.rng.range(-14.0, 14.0), y + self.rng.range(-10.0, 10.0), bt);
            }
        }

        let s = &mut self.scr;
        s.clear(at(BLACK, YELLOW));
        s.rect(0, 0, 256, 168, |x, y| (hash((x * 389 + y * 1013) as u32) & 127) == 0);
        // hex grid
        for q in 0..HEX_W {
            for r in 0..HEX_H {
                let (x, y) = hex_xy(q, r);
                let v = [(x - 8.0, y), (x - 4.0, y - 7.0), (x + 4.0, y - 7.0), (x + 8.0, y), (x + 4.0, y + 7.0), (x - 4.0, y + 7.0)];
                for e in 0..6 {
                    s.line(v[e], v[(e + 1) % 6], true);
                }
                if self.terrain[(r * HEX_W + q) as usize] == 3 {
                    for k in -1..=1 {
                        let bx = x + k as f32 * 5.0;
                        s.line((bx - 3.0, y + 2.0), (bx, y - 2.0), false);
                        s.line((bx, y - 2.0), (bx + 3.0, y + 2.0), false);
                    }
                    s.ink(x as i32 - 8, y as i32 - 4, 16, 8, RED);
                }
            }
        }
        // the road
        for x in 0..SW {
            let y = self.road.0 + 16.0 * (x as f32 * 0.017 + self.road.1).sin() + 5.0 * (x as f32 * 0.05).sin();
            s.plot(x, y as i32 - 2);
            s.plot(x, y as i32 + 2);
            if x % 4 < 2 {
                s.plot(x, y as i32);
            }
        }
        // the oasis
        let (ox, oy) = hex_xy(self.oasis.0, self.oasis.1);
        s.oval(ox, oy, 15.0, 8.0, |x, y| !((y as f32 - oy).abs() < 0.5 && x % 5 < 2));
        s.ink(ox as i32 - 15, oy as i32 - 8, 30, 16, BLUE);
        for (dx, dy) in [(-22.0, -10.0), (18.0, -12.0), (-16.0, 6.0), (20.0, 4.0)] {
            s.sprite(&self.palm, (ox + dx) as i32 - 4, (oy + dy) as i32 - 4, dx > 0.0, GREEN);
        }
        // a combat line between attacker and target
        let (fa, fd, ft) = self.fight;
        let fight_age = bt - ft;
        if fight_age < 0.6 {
            s.line(fa, fd, true);
            s.burst(fd.0, fd.1, fight_age / 0.6, ft as u32);
        }
        // unit counters
        let e = smoothstep(fract(bt).min(1.0) * 1.5);
        for u in &self.units {
            if u.dead > -50.0 {
                if bt - u.dead < 1.0 {
                    let (x, y) = hex_xy(u.at.0, u.at.1);
                    s.burst(x, y, bt - u.dead, u.num as u32);
                }
                continue;
            }
            let (x0, y0) = hex_xy(u.from.0, u.from.1);
            let (x1, y1) = hex_xy(u.at.0, u.at.1);
            let (x, y) = (lerp(x0, x1, e) as i32, lerp(y0, y1, e) as i32);
            let flash = bt - u.hit < 0.5 && (c.t * 16.0) as i32 % 2 == 0;
            let (bx, by) = (x - 6, y - 5);
            s.rect(bx - 1, by - 1, 14, 12, |_, _| false);
            s.rect(bx, by, 12, 10, |xx, yy| {
                let (lx, ly) = (xx - bx, yy - by);
                let edge = lx == 0 || lx == 11 || ly == 0 || ly == 9;
                let inner = (2..10).contains(&lx) && (2..7).contains(&ly);
                let sym = match u.kind {
                    0 => inner && (((lx as f32 - 5.5) / 3.5).powi(2) + ((ly as f32 - 4.0) / 2.0).powi(2) - 1.0).abs() < 0.45,
                    1 => inner && ((lx - 2) * 5 / 8 == ly - 2 || (9 - lx) * 5 / 8 == ly - 2),
                    2 => inner && (lx - 6).abs() <= 1 && (ly - 4).abs() <= 1,
                    _ => inner && (lx - 2) * 5 / 8 == ly - 2,
                };
                let pips = ly == 8 && (2..2 + u.strength as i32 * 3).contains(&lx) && (lx - 2) % 3 != 2;
                let solid = !(edge || sym || pips);
                if flash { !solid } else { solid }
            });
            s.ink(bx, by, 12, 10, self.colours[u.side as usize]);
        }
        for p in &self.puffs {
            let age = (bt - p.2) / 0.8;
            if (0.0..1.0).contains(&age) {
                let r = 2.0 + age * 7.0;
                for k in 0..10 {
                    let ang = k as f32 / 10.0 * TAU;
                    s.plot((p.0 + ang.cos() * r) as i32, (p.1 + ang.sin() * r * 0.7) as i32);
                }
                if age < 0.4 {
                    s.oval(p.0, p.1, 2.0, 2.0, |_, _| true);
                }
            }
        }
        // the report bar
        s.clear_rows(21, 24);
        s.cells(0, 21, 32, 1, at(YELLOW | BR, BLUE));
        s.print(0, 21, "TURN", None);
        s.num(5, 21, turn % 1000, 3, None);
        let alive = |sd: u8| self.units.iter().filter(|u| u.side == sd && u.dead < -50.0).count() as u32;
        let (n0, n1) = (side_name(self.colours[0]), side_name(self.colours[1]));
        s.cells(9, 21, 8, 1, at(WHITE | BR, self.colours[0]));
        s.print(9, 21, n0, None);
        s.num(16, 21, alive(0), 1, None);
        s.cells(18, 21, 8, 1, at(WHITE | BR, self.colours[1]));
        s.print(18, 21, n1, None);
        s.num(25, 21, alive(1), 1, None);
        s.print(27, 21, if side == 0 { "<MOVE" } else { "MOVE>" }, None);
        s.cells(0, 22, 32, 2, at(WHITE | BR, BLACK));
        s.print(0, 22, &self.msg, None);
        s.print(0, 23, &self.msg2, None);
        if fight_age < 0.6 {
            s.cells(0, 22, 32, 2, at(YELLOW | BR, BLACK));
        }
        let bc = zx(if fight_age < 0.12 { WHITE } else { BLACK });
        border(fb, |_| bc);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}

// ================================================================ 9. PANORAMA

const DIRS: [&str; 8] = ["NORTH", "NORTH-EAST", "EAST", "SOUTH-EAST", "SOUTH", "SOUTH-WEST", "WEST", "NORTH-WEST"];
const DIRS_SV: [&str; 8] = ["NORR", "NORDOST", "ÖSTER", "SYDOST", "SÖDER", "SYDVÄST", "VÄSTER", "NORDVÄST"];
const PLAINS: [&str; 8] = ["VARGHEM", "ISBERGA", "MÅNEDAL", "KORPMARK", "DIMMALUND", "STJÄRNHED", "ULVÅSA", "SNÖTORP"];
const PAN_W: i32 = 2048;

/// A first-person fantasy panorama: mountains, forests, towers and keeps
/// around a full circle; the view turns every 8 beats while a column of
/// banner riders crosses the plain at a speed from the bass.
struct Panorama {
    scr: Scr,
    clock: Clock,
    horse: [Spr; 2],
    banner: [Spr; 2],
    peaks: Vec<u8>,
    hills: Vec<u8>,
    towers: [(i32, i32); 4],
    keeps: [i32; 3],
    dir0: usize,
    view: i64,
    from: f32,
    to: f32,
    turned_at: f32,
    caption: Vec<String>,
    riders: f32,
    sky: f32,
}

impl Panorama {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let ph: Vec<f32> = (0..8).map(|_| r.range(0.0, TAU)).collect();
        let k = |x: i32, f: f32| x as f32 * TAU * f / PAN_W as f32;
        let peaks = (0..PAN_W)
            .map(|x| {
                let ridge = 1.0 - (k(x, 7.0) + ph[0]).sin().abs();
                let ridge2 = 1.0 - (k(x, 17.0) + ph[2]).sin().abs();
                let h = 10.0 + 26.0 * ridge.powi(3) + 8.0 * (k(x, 3.0) + ph[1]).sin() + 9.0 * ridge2.powi(2) + 2.0 * (k(x, 41.0) + ph[3]).sin();
                h.clamp(4.0, 46.0) as u8
            })
            .collect();
        let hills = (0..PAN_W).map(|x| (5.0 + 3.0 * (k(x, 11.0) + ph[4]).sin() + 2.0 * (k(x, 27.0) + ph[5]).sin()).max(0.0) as u8).collect();
        let base = r.below(PAN_W as u32) as i32;
        Panorama {
            scr: Scr::new(),
            clock: Clock::default(),
            horse: [spr(&art::RIDER), spr(&art::RIDER_B)],
            banner: [spr(&art::BANNER), spr(&art::BANNER_B)],
            peaks,
            hills,
            towers: [(base + 140, 46), ((base + 900) % PAN_W, 38), ((base + 1500) % PAN_W, 52), ((base + 1830) % PAN_W, 30)],
            keeps: [(base + 560) % PAN_W, (base + 1250) % PAN_W, (base + 330) % PAN_W],
            dir0: r.below(8) as usize,
            view: i64::MIN,
            from: 0.0,
            to: 0.0,
            turned_at: -99.0,
            caption: Vec::new(),
            riders: r.range(0.0, 400.0),
            sky: 0.0,
        }
    }

    /// Screen x of world column `wx` for the current view offset, if visible.
    fn sx(wx: i32, off: i32, half: i32) -> Option<i32> {
        let x = (wx - off).rem_euclid(PAN_W);
        if x < 256 + half {
            Some(x)
        } else if x > PAN_W - half {
            Some(x - PAN_W)
        } else {
            None
        }
    }
}

impl Effect for Panorama {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, _) = self.clock.tick(c);
        let view = (bt / 8.0).floor() as i64;
        if view != self.view {
            let dir = (view.rem_euclid(8) as usize + self.dir0) % 8;
            let first = self.view == i64::MIN;
            self.from = if first { dir as f32 * 256.0 } else { self.to };
            self.to = dir as f32 * 256.0;
            if self.to < self.from - 1024.0 {
                self.to += PAN_W as f32;
            }
            self.turned_at = if first { bt - 2.0 } else { bt };
            self.view = view;
            let plain = PLAINS[((view.rem_euclid(64) / 8) as usize + self.dir0) % 8];
            let text = if view.rem_euclid(2) == 0 {
                format!("YOU STAND ON THE PLAIN OF {plain}, LOOKING {}.", DIRS[dir])
            } else {
                format!("DU STÅR PÅ SLÄTTEN VID {plain} OCH BLICKAR MOT {}.", DIRS_SV[dir])
            };
            self.caption = wrap(&text, 32);
        }
        let k = smoothstep((bt - self.turned_at).clamp(0.0, 1.0));
        let off = lerp(self.from, self.to, k) as i32;
        self.sky += (a.bass + a.mid * 0.5 - self.sky) * (1.0 - (-c.dt * 1.5).exp());
        self.riders += c.dt * (18.0 + 70.0 * a.bass);

        let s = &mut self.scr;
        // sky colours follow the music: night, blue hour, dawn
        let lvl = (self.sky * 2.6) as usize;
        let (top, low) = [(BLACK, BLUE), (BLUE, MAGENTA), (BLUE, CYAN), (MAGENTA, RED)][lvl.min(3)];
        s.clear(at(WHITE, top));
        let mink = if low == CYAN { BLUE } else { WHITE };
        s.cells(0, 4, 32, 6, at(mink | BR, low));
        s.cells(0, 10, 32, 4, at(BLACK, GREEN));
        s.cells(0, 14, 32, 8, at(BLACK, GREEN | BR));
        s.cells(0, 22, 32, 2, at(YELLOW | BR, BLACK));
        for i in 0..70u32 {
            let h = hash(i * 7919 + 1);
            let wx = (h % PAN_W as u32) as i32;
            if let Some(x) = Self::sx(wx, off, 0) {
                let y = ((h >> 12) % 40) as i32;
                if (h >> 24) % 4 != 0 || a.treble > 0.5 {
                    s.plot(x, y);
                }
            }
        }
        if let Some(x) = Self::sx(self.towers[0].0 + 300, off, 12) {
            s.oval(x as f32, 18.0, 7.0, 7.0, |_, _| true);
            s.oval(x as f32 + 3.0, 16.0, 6.0, 6.0, |_, _| false);
            s.ink(x - 8, 10, 16, 16, YELLOW | BR);
        }
        // the far mountains
        for x in 0..SW {
            let wx = (x + off).rem_euclid(PAN_W) as usize;
            let top = 80 - self.peaks[wx] as i32;
            for y in top..80 {
                let d = y - top;
                let on = if d < 3 { true } else if d < 9 { (x + y) % 2 == 0 } else { x % 2 == 0 && y % 2 == 0 };
                s.set(x, y, on);
            }
        }
        // a forest band on the green, and grass tufts on the near plain
        for x in 0..SW {
            let wx = (x + off).rem_euclid(PAN_W);
            let h = self.hills[wx as usize] as i32;
            let tree = (hash((wx / 5) as u32) % 3) as i32;
            for y in 80..84 + h + tree {
                if y < 84 + h - 2 || (x + y) % 2 == 0 {
                    s.plot(x, y);
                }
            }
            if hash(wx as u32 * 31) % 9 == 0 {
                let y = 116 + (hash(wx as u32) % 56) as i32;
                s.plot(x, y);
                s.plot(x - 1, y - 1);
                s.plot(x + 1, y - 1);
            }
        }
        // boulders and bushes on the near plain
        for i in 0..24u32 {
            let h = hash(i * 2654 + 77);
            let wx = (h % PAN_W as u32) as i32;
            if let Some(x) = Self::sx(wx, off, 12) {
                let y = 118.0 + ((h >> 11) % 50) as f32;
                let r = 3.0 + (y - 118.0) / 12.0;
                if (h >> 20) % 2 == 0 {
                    s.oval(x as f32, y, r * 1.4, r, |px, py| (px + py) % 3 != 0);
                } else {
                    for k in 0..5 {
                        let ang = PI + k as f32 * PI / 4.0;
                        s.line((x as f32, y), (x as f32 + ang.cos() * r * 1.6, y + ang.sin() * r * 1.6), false);
                    }
                }
            }
        }
        // towers and keeps on the skyline
        for &(wx, h) in &self.towers {
            if let Some(x) = Self::sx(wx, off, 12) {
                let base = 96;
                s.rect(x - 5, base - h, 10, h, |_, _| true);
                s.poly(&[(x as f32 - 7.0, (base - h) as f32), (x as f32, (base - h - 14) as f32), (x as f32 + 7.0, (base - h) as f32)], false, |_, _| true);
                s.rect(x - 1, base - h + 8, 2, 4, |_, _| false);
                s.rect(x - 1, base - h + 22, 2, 4, |_, _| false);
                let wave = if (bt * 2.0) as i32 % 2 == 0 { 1 } else { 0 };
                s.ln(x, base - h - 14, x, base - h - 22);
                s.rect(x + 1, base - h - 22 + wave, 5, 3, |_, _| true);
                s.tint(x - 7, base - h - 22, 14, h + 22, BLACK);
            }
        }
        for &wx in &self.keeps {
            if let Some(x) = Self::sx(wx, off, 30) {
                let base = 100;
                s.rect(x - 22, base - 22, 44, 22, |_, _| true);
                for t in [-22, 14] {
                    s.rect(x + t, base - 36, 8, 36, |_, _| true);
                    for m in 0..3 {
                        s.rect(x + t + m * 3, base - 39, 2, 3, |_, _| true);
                    }
                }
                for m in 0..7 {
                    s.rect(x - 14 + m * 4, base - 25, 2, 3, |_, _| true);
                }
                s.rect(x - 4, base - 12, 8, 12, |_, _| false);
                s.oval(x as f32, (base - 12) as f32, 4.0, 3.0, |_, _| false);
                s.rect(x - 19, base - 30, 2, 3, |_, _| false);
                s.rect(x + 17, base - 30, 2, 3, |_, _| false);
                s.tint(x - 22, base - 39, 44, 39, BLACK);
            }
        }
        // the column of riders
        let banners = [RED | BR, BLUE | BR, MAGENTA | BR, WHITE | BR, YELLOW | BR];
        for i in 0..5 {
            let x = (self.riders - i as f32 * 30.0).rem_euclid(420.0) as i32 - 40;
            let gallop = (x / 6).rem_euclid(2) as usize;
            let bob = if gallop == 1 { 1 } else { 0 };
            s.draw(&self.horse[gallop], x, 144 + bob, false);
            s.tint(x, 144, 16, 16, BLACK);
            s.draw(&self.banner[((bt * 2.0) as usize + i) % 2], x, 136 + bob, false);
            s.ink(x, 136, 16, 7, banners[i]);
        }
        // the caption types itself after every turn
        s.clear_rows(22, 24);
        let mut left = ((bt - self.turned_at) * 24.0).max(0.0) as usize;
        for (i, line) in self.caption.iter().take(2).enumerate() {
            let n = line.chars().count();
            let show = left.min(n);
            let part: &str = match line.char_indices().nth(show) {
                Some((b, _)) => &line[..b],
                None => line,
            };
            s.print(0, 22 + i as i32, part, None);
            left -= show;
        }
        let bc = zx(BLACK);
        border(fb, |_| bc);
        self.scr.render(fb, c.beats % 2 == 1);
    }
}
