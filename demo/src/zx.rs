//! A ZX Spectrum-style platform room: 256x192 screen with 8x8 attribute blocks,
//! the 15-colour Spectrum palette, tape-loading stripes in the border, and a cast
//! of original sprites that move to the music (the hero jumps on every beat,
//! guardians patrol in step with the tempo, crystals flash with the treble).

use crate::audio::Audio;
use crate::font;
use crate::gfx::{Fb, H, W};

pub const SW: i32 = 256;
pub const SH: i32 = 192;
pub const X0: i32 = (W as i32 - SW) / 2;
pub const Y0: i32 = (H as i32 - SH) / 2;

/// Spectrum colour: 0..7 normal (black, blue, red, magenta, green, cyan, yellow, white), +8 bright.
pub fn zx(i: u8) -> u32 {
    let level: u32 = if i >= 8 { 0xFF } else { 0xD7 };
    let b = (i & 1) as u32 * level;
    let r = ((i >> 1) & 1) as u32 * level;
    let g = ((i >> 2) & 1) as u32 * level;
    0xFF00_0000 | (b << 16) | (g << 8) | r
}

const BLACK: u8 = 0;
const BLUE: u8 = 1;
const RED: u8 = 2;
const MAGENTA: u8 = 3;
const GREEN: u8 = 4;
const CYAN: u8 = 5;
const YELLOW: u8 = 6;
const WHITE: u8 = 7;
const BRIGHT: u8 = 8;

/// Room layout, 32x16 cells.
/// W wall, F platform, C crumbling floor, > conveyor, ^ spike, * crystal.
pub const ROOM: [&str; 16] = [
    "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW",
    "W         *            *       W",
    "W                              W",
    "W   *          ^^              W",
    "W FFFFFF    FFFFFFFF   CCCCCC  W",
    "W                            * W",
    "W                              W",
    "W        *          FFFF       W",
    "W  CCCCC FFFFF                 W",
    "W                  *     FFFFF W",
    "W     >>>>>>>>>>>              W",
    "W                     *        W",
    "W  *                FFFFFFFF   W",
    "W                              W",
    "W                       *      W",
    "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
];
const COLS: usize = 32;
const ROWS: usize = 16;

#[rustfmt::skip]
mod art {
    pub const HERO_WALK_A: [u8; 16] = [
        0b00011000, 0b00001000, 0b00111100, 0b01111110, 0b01011010, 0b01111110, 0b01100110, 0b00111100,
        0b00011000, 0b01111110, 0b10111101, 0b10111101, 0b00111100, 0b00100100, 0b00100100, 0b01100110,
    ];
    pub const HERO_WALK_B: [u8; 16] = [
        0b00011000, 0b00001000, 0b00111100, 0b01111110, 0b01011010, 0b01111110, 0b01100110, 0b00111100,
        0b00011000, 0b01111110, 0b10111101, 0b10111101, 0b00111100, 0b01000010, 0b10000001, 0b11000011,
    ];
    pub const HERO_JUMP: [u8; 16] = [
        0b00011000, 0b00001000, 0b00111100, 0b01111110, 0b01011010, 0b01111110, 0b01000010, 0b10111101,
        0b10011001, 0b01111110, 0b00111100, 0b00111100, 0b00111100, 0b01000010, 0b01000010, 0b00000000,
    ];
    pub const SAUCER_A: [u8; 8] = [
        0b00011000, 0b00111100, 0b01011010, 0b11111111, 0b10101011, 0b11111111, 0b01111110, 0b00100100,
    ];
    pub const SAUCER_B: [u8; 8] = [
        0b00011000, 0b00111100, 0b01011010, 0b11111111, 0b11010101, 0b11111111, 0b01111110, 0b01000010,
    ];
    pub const BLOB_A: [u8; 8] = [
        0b00111100, 0b01111110, 0b11011011, 0b11111111, 0b11111111, 0b11100111, 0b01111110, 0b10100101,
    ];
    pub const BLOB_B: [u8; 8] = [
        0b00000000, 0b00111100, 0b01111110, 0b11011011, 0b11111111, 0b11111111, 0b11100111, 0b01011010,
    ];
    pub const CRYSTAL: [u8; 8] = [
        0b00011000, 0b00111100, 0b01101110, 0b11011111, 0b01111110, 0b00111100, 0b00011000, 0b00000000,
    ];
    pub const BRICK: [u8; 8] = [
        0b11101110, 0b11101110, 0b00000000, 0b10111011, 0b10111011, 0b00000000, 0b11101110, 0b11101110,
    ];
    pub const FLOOR: [u8; 8] = [
        0b11111111, 0b11111111, 0b11011011, 0b10010010, 0, 0, 0, 0,
    ];
    pub const CRUMBLE: [u8; 8] = [
        0b11111111, 0b11011011, 0b10110110, 0b01101101, 0b11011011, 0b10010010, 0b01001001, 0b00100100,
    ];
    pub const SPIKE: [u8; 8] = [
        0b00000000, 0b00010000, 0b00010000, 0b00111000, 0b00111000, 0b01111100, 0b01111100, 0b11111110,
    ];
}
use art::*;

pub struct Zx {
    decay: [u8; COLS * ROWS],
    collected: Vec<(usize, f32)>, // (cell, time it comes back)
    hero_x: f32,
    hero_dir: f32,
    jump_t0: f32,
    jump_len: f32,
    walk: f32,
    beat_phase: f32,
    conveyor: f32,
    regen: f32,
    pub score: u32,
    hi: u32,
    rng: u32,
}

impl Default for Zx {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn tri(x: f32) -> f32 {
    let f = x.rem_euclid(1.0);
    if f < 0.5 { f * 2.0 } else { 2.0 - f * 2.0 }
}

fn cell(c: usize, r: usize) -> u8 {
    ROOM.get(r).and_then(|row| row.as_bytes().get(c)).copied().unwrap_or(b' ')
}

impl Zx {
    pub fn new() -> Self {
        Zx {
            decay: [0; COLS * ROWS],
            collected: Vec::new(),
            hero_x: 24.0,
            hero_dir: 1.0,
            jump_t0: -10.0,
            jump_len: 0.45,
            walk: 0.0,
            beat_phase: 0.0,
            conveyor: 0.0,
            regen: 0.0,
            score: 0,
            hi: 7500,
            rng: 0x1234_5678,
        }
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn hero_y(&self, t: f32, a: &Audio) -> f32 {
        let ground = 15.0 * 8.0 - 16.0;
        let p = (t - self.jump_t0) / self.jump_len;
        if (0.0..1.0).contains(&p) {
            ground - (20.0 + a.bass * 10.0) * 4.0 * p * (1.0 - p)
        } else {
            ground
        }
    }

    pub fn update(&mut self, t: f32, dt: f32, a: &Audio) {
        self.beat_phase += dt / a.period;

        // hero walks the bottom floor and jumps on every beat
        let speed = 28.0 + a.mid * 40.0;
        self.hero_x += self.hero_dir * speed * dt;
        self.walk += speed * dt;
        if self.hero_x > (SW - 16) as f32 {
            self.hero_x = (SW - 16) as f32;
            self.hero_dir = -1.0;
        }
        if self.hero_x < 8.0 {
            self.hero_x = 8.0;
            self.hero_dir = 1.0;
        }
        if a.beat && t - self.jump_t0 > self.jump_len {
            self.jump_t0 = t;
            self.jump_len = (a.period * 0.9).clamp(0.3, 0.7);
        }

        // crumbling floors give way on heavy beats and slowly grow back
        if a.beat && a.bass > 0.45 {
            let cells: Vec<usize> =
                (0..COLS * ROWS).filter(|&i| cell(i % COLS, i / COLS) == b'C').collect();
            if !cells.is_empty() {
                let i = cells[self.rand() as usize % cells.len()];
                self.decay[i] = (self.decay[i] + 3).min(7);
            }
        }
        self.regen += dt;
        if self.regen > 0.35 {
            self.regen = 0.0;
            for d in self.decay.iter_mut() {
                *d = d.saturating_sub(1);
            }
        }
        self.conveyor += dt * (10.0 + a.bass * 40.0);

        // collect crystals the hero touches
        self.collected.retain(|&(_, back)| back > t);
        let hy = self.hero_y(t, a);
        for r in 0..ROWS {
            for c in 0..COLS {
                if cell(c, r) != b'*' {
                    continue;
                }
                let i = r * COLS + c;
                if self.collected.iter().any(|&(j, _)| j == i) {
                    continue;
                }
                let (cx, cy) = ((c * 8) as f32, (r * 8) as f32);
                if self.hero_x + 7.0 > cx && self.hero_x < cx + 7.0 && hy + 15.0 > cy && hy < cy + 7.0 {
                    self.collected.push((i, t + 10.0));
                    self.score += 100;
                    self.hi = self.hi.max(self.score);
                }
            }
        }
    }

    pub fn draw(&mut self, fb: &mut Fb, t: f32, a: &Audio) {
        self.border(fb, a);
        // the 256x192 screen, black paper
        for y in 0..SH {
            fb.hspan(X0, X0 + SW, Y0 + y, zx(BLACK));
        }

        // room blocks
        let flash = a.beats % 2 == 1;
        for r in 0..ROWS {
            for c in 0..COLS {
                let (x, y) = ((c * 8) as i32, (r * 8) as i32);
                let i = r * COLS + c;
                match cell(c, r) {
                    b'W' => tile(fb, x, y, &BRICK, RED | BRIGHT, Some(RED)),
                    b'F' => tile(fb, x, y, &FLOOR, GREEN | BRIGHT, None),
                    b'C' => {
                        let d = self.decay[i] as usize;
                        let mut rows = [0u8; 8];
                        rows[d..].copy_from_slice(&CRUMBLE[..8 - d]);
                        tile(fb, x, y, &rows, MAGENTA | BRIGHT, None);
                    }
                    b'>' => {
                        let o = (self.conveyor as u32 % 8) as u32;
                        let rows = [
                            0xFF,
                            0b1100_1100u8.rotate_right(o),
                            0b0011_0011u8.rotate_left(o),
                            0xFF,
                            0,
                            0,
                            0,
                            0,
                        ];
                        tile(fb, x, y, &rows, CYAN | BRIGHT, None);
                    }
                    b'^' => tile(fb, x, y, &SPIKE, YELLOW | BRIGHT, None),
                    b'*' => {
                        if self.collected.iter().any(|&(j, _)| j == i) {
                            continue;
                        }
                        let ink = 9 + ((t * (4.0 + a.treble * 20.0)) as u32 + i as u32) % 7;
                        let ink = ink as u8;
                        if flash {
                            tile(fb, x, y, &CRYSTAL, BLACK, Some(ink));
                        } else {
                            tile(fb, x, y, &CRYSTAL, ink, None);
                        }
                    }
                    _ => {}
                }
            }
        }

        let bp = self.beat_phase;
        let frame = a.beats % 2 == 0;
        let saucer = if frame { &SAUCER_A } else { &SAUCER_B };
        let blob = if frame { &BLOB_A } else { &BLOB_B };
        // guardians: all positions are functions of the beat phase, so they step in time
        let hop = |o: f32| ((bp + o) * std::f32::consts::PI).sin().abs() * (3.0 + a.bass * 6.0);
        sprite8(fb, 8.0 * (4.0 + 10.0 * tri(bp / 8.0)), 16.0 - a.bass * 4.0, saucer, YELLOW | BRIGHT, false);
        sprite8(fb, 8.0 * (19.0 + 10.0 * tri(bp / 8.0 + 0.5)), 48.0 - a.mid * 4.0, saucer, MAGENTA | BRIGHT, false);
        sprite8(fb, 8.0 * 17.0, 8.0 + 88.0 * tri(bp / 6.0), saucer, RED | BRIGHT, false);
        sprite8(fb, 8.0 * (21.0 + (7.0 * tri(bp / 7.0)).round()), 88.0 - hop(0.0), blob, GREEN | BRIGHT, tri(bp / 7.0 + 0.25) > 0.5);
        sprite8(fb, 8.0 * (9.0 + (4.0 * tri(bp / 4.0)).round()), 56.0 - hop(0.5), blob, CYAN | BRIGHT, false);

        // the hero
        let hy = self.hero_y(t, a);
        let airborne = hy < 15.0 * 8.0 - 16.5;
        let art = if airborne {
            &HERO_JUMP
        } else if (self.walk / 6.0) as i32 % 2 == 0 {
            &HERO_WALK_A
        } else {
            &HERO_WALK_B
        };
        sprite(fb, self.hero_x, hy, art, WHITE | BRIGHT, self.hero_dir < 0.0);

        self.status(fb, t, a);
    }

    fn border(&mut self, fb: &mut Fb, a: &Audio) {
        let loud = a.autopilot || a.bass + a.mid > 0.35;
        if !loud {
            fb.clear(zx(BLUE));
            return;
        }
        // tape-loading stripes: random-height bars in two alternating colours
        let (c1, c2) = if (a.beats / 8) % 2 == 0 { (RED, CYAN) } else { (BLUE, YELLOW) };
        let mut y = 0;
        let mut which = false;
        while y < H as i32 {
            let len = 1 + (self.rand() % if a.pulse > 0.5 { 3 } else { 6 }) as i32;
            let c = zx(if which { c1 } else { c2 });
            for yy in y..(y + len).min(H as i32) {
                fb.row(yy, c);
            }
            y += len;
            which = !which;
        }
    }

    fn status(&self, fb: &mut Fb, t: f32, a: &Audio) {
        // room name band
        let band_y = 16 * 8;
        cells(fb, 0, band_y, 32, 8, YELLOW);
        cstr(fb, "THE HANGAR BELOW THE STARS", band_y, BLACK);

        // a spectrum analyser made of attribute blocks
        let top = band_y + 8;
        for i in 0..32 {
            let level = if a.autopilot {
                let f = i as f32 / 31.0;
                ((a.bass * (1.0 - f) + a.treble * f) * (0.6 + 0.4 * (t * 7.0 + i as f32).sin())).clamp(0.0, 1.0)
            } else {
                let b = (2.0 * (120.0f32).powf(i as f32 / 31.0)) as usize;
                a.bins[b.min(crate::audio::BINS - 1)] as f32 / 255.0
            };
            let hpx = (level * 24.0) as i32;
            for k in 0..hpx {
                let ink = if k < 10 { GREEN } else if k < 18 { YELLOW } else { RED } | BRIGHT;
                fb.hspan(X0 + i * 8, X0 + i * 8 + 7, Y0 + top + 23 - k, zx(ink));
            }
        }

        let score = format!("SCORE {:06}    HI-SCORE {:06}", self.score, self.hi);
        cstr(fb, &score, 21 * 8, YELLOW | BRIGHT);

        // lives dance on alternating beats
        for n in 0..4 {
            let up = (a.beats + n) % 2 == 0;
            let art = if up { &HERO_WALK_A } else { &HERO_WALK_B };
            let y = (22 * 8) as f32 - if up { a.pulse * 3.0 } else { 0.0 };
            sprite(fb, 8.0 + n as f32 * 16.0, y, art, CYAN | BRIGHT, n % 2 == 1);
        }
    }
}

fn tile(fb: &mut Fb, x: i32, y: i32, rows: &[u8; 8], ink: u8, paper: Option<u8>) {
    for (ry, &bits) in rows.iter().enumerate() {
        for bx in 0..8 {
            let set = (bits >> (7 - bx)) & 1 == 1;
            let c = if set { Some(ink) } else { paper };
            if let Some(c) = c {
                fb.put(X0 + x + bx, Y0 + y + ry as i32, zx(c));
            }
        }
    }
}

fn sprite_rows(fb: &mut Fb, x: f32, y: f32, rows: &[u8], ink: u8, mirror: bool) {
    let (x, y) = (x.round() as i32, y.round() as i32);
    for (ry, &bits) in rows.iter().enumerate() {
        let bits = if mirror { bits.reverse_bits() } else { bits };
        for bx in 0..8 {
            if (bits >> (7 - bx)) & 1 == 1 {
                let (px, py) = (x + bx, y + ry as i32);
                if (0..SW).contains(&px) && (0..SH).contains(&py) {
                    fb.put(X0 + px, Y0 + py, zx(ink));
                }
            }
        }
    }
}

fn sprite(fb: &mut Fb, x: f32, y: f32, rows: &[u8; 16], ink: u8, mirror: bool) {
    sprite_rows(fb, x, y, rows, ink, mirror);
}

fn sprite8(fb: &mut Fb, x: f32, y: f32, rows: &[u8; 8], ink: u8, mirror: bool) {
    sprite_rows(fb, x, y, rows, ink, mirror);
}

fn cells(fb: &mut Fb, col: i32, y: i32, ncols: i32, h: i32, paper: u8) {
    for yy in y..y + h {
        fb.hspan(X0 + col * 8, X0 + (col + ncols) * 8, Y0 + yy, zx(paper));
    }
}

/// Centred text on the 8-pixel character grid.
fn cstr(fb: &mut Fb, s: &str, y: i32, ink: u8) {
    let n = s.chars().count() as i32;
    let col = ((32 - n) / 2).max(0);
    for (i, ch) in s.chars().enumerate() {
        font::text(fb, &ch.to_string(), X0 + (col + i as i32) * 8 + 1, Y0 + y, 1, false, zx(ink));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_rows_are_32_cells() {
        for (i, r) in ROOM.iter().enumerate() {
            assert_eq!(r.len(), 32, "row {i}");
        }
    }
}
