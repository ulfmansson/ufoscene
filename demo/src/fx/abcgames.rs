//! Effect: an original game scene in the style of the Luxor ABC 80 (Sweden, 1978).
//!
//! The ABC 80 has no bitmap. Its screen is 40×24 bytes of video memory read by
//! a teletext character generator and shown white on black on a monochrome
//! monitor. [`Abc`] emulates that memory and the generator, from Luxor's *ABC 80
//! bruksanvisning* (2.20 Grafisk mod, 4.7 Specialtecken and the table of codes
//! in text and graphics mode):
//!
//! - `CHR$(151)` START GRAFIK and `CHR$(135)` SLUT GRAFIK switch a row between
//!   text and graphics. They work along one row only, and each one takes a
//!   character cell that shows as blank. Every row starts in text mode.
//! - In graphics mode a character is a 2×3 block of dots: bits 1, 2, 4, 8, 16
//!   and 64 of the code light the top-left, top-right, middle-left,
//!   middle-right, bottom-left and bottom-right dots. Capital letters (codes
//!   64–95) stay letters in graphics mode.
//! - A screen of graphics rows is 78×72 dots. Rows run 0–71 and columns 2–79,
//!   because START GRAFIK holds the first cell: BASIC's `SETDOT R,K`,
//!   `CLRDOT R,K` and `DOT(R,K)`.
//! - The Swedish letters sit at ¤ 36, É 64, Ä Ö Å Ü 91–94, é 96 and
//!   ä ö å ü 123–126; 127 is a solid block. So `$` prints as `¤`, which is why
//!   the manual writes `CHR¤(151)`.
//!
//! The control codes are the teletext set of the Mullard SAA5050 family, whose
//! Swedish member is the SAA5052. Flashing (`CHR$(136)`, steady `CHR$(137)`)
//! comes from that chip; the ABC 80 manual itself documents only 151 and 135.
//! The chip draws each character in a 6×10-dot cell. This demo's framebuffer is
//! 216 lines tall, so each cell here is 8×9 pixels, 320×216 for the screen.
//!
//! The game is a 1970s BASIC classic: landing a craft on the moon. Its terrain,
//! craft, pads and texts are new. The engine burns on the beat, the flame grows
//! with the bass, the stars twinkle with the treble and the target pad's beacon
//! blinks in time.

use std::f32::consts::TAU;

use super::zxgames::{Clock, hash};
use super::*;
use crate::font;

pub fn register(v: &mut Vec<Def>) {
    v.push(Def { name: "ABC 80 LANDER", flags: RETRO8 | OWN_TEXT, make: |s| Box::new(Lander::new(s)) });
}

// ================================================================ the ABC 80 screen

const COLS: usize = 40;
const ROWS: usize = 24;
/// One character cell in framebuffer pixels.
const CW: i32 = 8;
const CH: i32 = 9;
const SW: i32 = COLS as i32 * CW;
const SH: i32 = ROWS as i32 * CH;
const X0: i32 = (WI - SW) / 2;
const Y0: i32 = (HI - SH) / 2;

/// `CHR$(151)`: the rest of the row is graphics.
const START_GRAFIK: u8 = 151;
/// `CHR$(135)`: the rest of the row is text again.
const SLUT_GRAFIK: u8 = 135;
/// `CHR$(136)` / `CHR$(137)`: flashing / steady text (an SAA5050 code).
const FLASH: u8 = 136;
const STEADY: u8 = 137;

/// The white phosphor, the lit-but-black raster and the bezel around it.
const INK: u32 = 0xFFF0_F0F0;
const PAPER: u32 = 0xFF08_0808;
const BEZEL: u32 = 0xFF00_0000;

/// The six mosaic bits by (row, column) inside a cell.
const DOT_BITS: [[u8; 2]; 3] = [[1, 2], [4, 8], [16, 64]];

/// Code for a character, by the ABC 80's Swedish 7-bit set. Other printable
/// ASCII keeps its code, so `$` and `[` come out as `¤` and `Ä`, as they did.
fn code(ch: char) -> u8 {
    match ch {
        '¤' => 36,
        'É' => 64,
        'Ä' => 91,
        'Ö' => 92,
        'Å' => 93,
        'Ü' => 94,
        'é' => 96,
        'ä' => 123,
        'ö' => 124,
        'å' => 125,
        'ü' => 126,
        ' '..='~' => ch as u8,
        _ => b'?',
    }
}

/// The 5×7 shape of a text character. Letters come from the demo's own font;
/// the scene prints capitals only, so lower case reuses the capital shapes.
#[rustfmt::skip]
fn glyph(c: u8) -> [u8; 7] {
    match c {
        36 => [0b00000, 0b10001, 0b01110, 0b01010, 0b01110, 0b10001, 0b00000],
        64 => [0b00010, 0b00100, 0b11111, 0b10000, 0b11110, 0b10000, 0b11111],
        94 | 126 => [0b01010, 0b00000, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        95 => [0, 0, 0, 0, 0, 0, 0b11111],
        96 => [0b00010, 0b00100, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110],
        91 | 123 => font::glyph('Ä'),
        92 | 124 => font::glyph('Ö'),
        93 | 125 => font::glyph('Å'),
        127 => [0b11111; 7],
        _ => font::glyph(c as char),
    }
}

/// The ABC 80's video memory: one byte per character cell.
struct Abc {
    mem: [u8; COLS * ROWS],
}

impl Abc {
    fn new() -> Self {
        Abc { mem: [b' '; COLS * ROWS] }
    }

    fn clear(&mut self) {
        self.mem.fill(b' ');
    }

    fn put(&mut self, row: i32, col: i32, c: u8) {
        if (0..ROWS as i32).contains(&row) && (0..COLS as i32).contains(&col) {
            self.mem[row as usize * COLS + col as usize] = c;
        }
    }

    #[cfg(test)]
    fn get(&self, row: i32, col: i32) -> u8 {
        self.mem[row as usize * COLS + col as usize]
    }

    /// `PRINT CUR(row, col); s`. Returns the column after the text.
    fn print(&mut self, row: i32, col: i32, s: &str) -> i32 {
        let mut x = col;
        for ch in s.chars() {
            self.put(row, x, code(ch));
            x += 1;
        }
        x
    }

    /// A zero-padded number. Returns the column after it.
    fn num(&mut self, row: i32, col: i32, n: u32, digits: i32) -> i32 {
        let mut n = n;
        for i in (0..digits).rev() {
            self.put(row, col + i, b'0' + (n % 10) as u8);
            n /= 10;
        }
        col + digits
    }

    /// A sign and a zero-padded number.
    fn signed(&mut self, row: i32, col: i32, v: i32, digits: i32) -> i32 {
        self.put(row, col, if v < 0 { b'-' } else { b'+' });
        self.num(row, col + 1, v.unsigned_abs(), digits)
    }

    /// Text centred on the row.
    fn centre(&mut self, row: i32, s: &str) {
        self.print(row, (COLS as i32 - s.chars().count() as i32) / 2, s);
    }

    /// Where dot (r, k) lives: its cell and its bit. Columns 0 and 1 belong to
    /// START GRAFIK, so `SETDOT` starts at column 2.
    fn dot_at(r: i32, k: i32) -> Option<(usize, u8)> {
        if !(0..ROWS as i32 * 3).contains(&r) || !(2..COLS as i32 * 2).contains(&k) {
            return None;
        }
        Some(((r / 3) as usize * COLS + (k / 2) as usize, DOT_BITS[(r % 3) as usize][(k % 2) as usize]))
    }

    /// `SETDOT r, k`. A control character is never overwritten; any other
    /// character that is not already a mosaic becomes an empty one first.
    fn setdot(&mut self, r: i32, k: i32) {
        if let Some((i, bit)) = Self::dot_at(r, k) {
            let c = self.mem[i];
            if c & 0x7F >= 0x20 {
                self.mem[i] = if c & 0x20 != 0 { c | bit } else { 0x20 | bit };
            }
        }
    }

    /// `CLRDOT r, k`.
    #[cfg(test)]
    fn clrdot(&mut self, r: i32, k: i32) {
        if let Some((i, bit)) = Self::dot_at(r, k) {
            let c = self.mem[i];
            if c & 0x7F >= 0x20 && c & 0x20 != 0 {
                self.mem[i] = c & !bit;
            }
        }
    }

    /// `DOT(r, k)`.
    #[cfg(test)]
    fn dot(&self, r: i32, k: i32) -> bool {
        Self::dot_at(r, k).is_some_and(|(i, bit)| {
            let c = self.mem[i];
            c & 0x7F >= 0x20 && c & 0x20 != 0 && c & bit != 0
        })
    }

    /// The character generator: walk each row like the chip does, keeping its
    /// text/graphics and flash state, which every row starts without.
    fn render(&self, fb: &mut Fb, flash_on: bool, ink: u32) {
        fb.clear(BEZEL);
        for row in 0..ROWS {
            let (mut graphics, mut flash) = (false, false);
            for col in 0..COLS {
                let c = self.mem[row * COLS + col] & 0x7F;
                // 0: blank, 1: mosaic, 2: text
                let mut kind = 0;
                if c < 0x20 {
                    // the control cell shows as blank; the new state starts after it
                    match c {
                        0x01..=0x07 => graphics = false,
                        0x11..=0x17 => graphics = true,
                        0x08 => flash = true,
                        0x09 => flash = false,
                        _ => {}
                    }
                } else if !flash || flash_on {
                    kind = if graphics && c & 0x20 != 0 { 1 } else { 2 };
                }
                let g = if kind == 2 { glyph(c) } else { [0; 7] };
                for py in 0..CH {
                    let o = ((Y0 + row as i32 * CH + py) * WI + X0 + col as i32 * CW) as usize;
                    for px in 0..CW {
                        let on = match kind {
                            1 => c & DOT_BITS[(py / 3) as usize][(px / 4) as usize] != 0,
                            2 => (1..6).contains(&px) && (1..8).contains(&py) && (g[py as usize - 1] >> (5 - px)) & 1 == 1,
                            _ => false,
                        };
                        fb.px[o + px as usize] = if on { ink } else { PAPER };
                    }
                }
            }
        }
        // the monitor's beam trails a little to the right of every lit pixel
        let glow = mix(PAPER, ink, 0.3);
        for y in Y0..Y0 + SH {
            let o = (y * WI + X0) as usize;
            for x in (1..SW as usize).rev() {
                if fb.px[o + x] == PAPER && fb.px[o + x - 1] == ink {
                    fb.px[o + x] = glow;
                }
            }
        }
    }
}

// ================================================================ the game

/// The craft, bottom row on the ground, 6×6 dots.
const CRAFT: [&str; 6] = ["..XX..", ".XXXX.", "XX..XX", "XXXXXX", ".X..X.", "X....X"];

/// Graphics rows 3..=20 are the moon: dot rows 9..=62.
const SKY: i32 = 9;
const FLOOR: i32 = 62;
/// Beats per attempt: arrive, descend for ten beats, then land or crash.
const ROUND: f32 = 16.0;
/// Metres per dot, for the instruments.
const METRES: f32 = 2.0;

struct Lander {
    scr: Abc,
    base: [u8; COLS * ROWS],
    clock: Clock,
    seed: u32,
    round: i64,
    /// Top dot row of the ground in each dot column.
    ground: [i32; COLS * 2],
    /// Landing pads: (first dot column, surface row, score multiplier).
    pads: [(i32, i32, u32); 2],
    target: usize,
    crash: bool,
    start: (f32, f32),
    aim: (f32, f32),
    stars: Vec<(i32, i32)>,
    pos: (f32, f32),
    vel: (f32, f32),
    fuel: f32,
    score: u32,
    record: u32,
    tries: u32,
    scored: bool,
}

impl Lander {
    fn new(seed: u32) -> Self {
        Lander {
            scr: Abc::new(),
            base: [b' '; COLS * ROWS],
            clock: Clock::default(),
            seed,
            round: i64::MIN,
            ground: [FLOOR; COLS * 2],
            pads: [(10, 45, 2), (50, 45, 5)],
            target: 0,
            crash: false,
            start: (20.0, 14.0),
            aim: (40.0, 44.0),
            stars: Vec::new(),
            pos: (20.0, 14.0),
            vel: (0.0, 0.0),
            fuel: 100.0,
            score: 0,
            record: 250 + (hash(seed) % 8) * 50,
            tries: 0,
            scored: false,
        }
    }

    /// The lowest ground row under a craft centred at `x`.
    fn under(&self, x: f32) -> i32 {
        let left = (x - 2.5).round() as i32;
        (left..left + 6).map(|k| self.ground[k.clamp(0, COLS as i32 * 2 - 1) as usize]).min().unwrap_or(FLOOR)
    }

    /// A new stretch of moon, two pads and a new attempt.
    fn enter(&mut self, round: i64) {
        let mut r = Rng::new(self.seed ^ (round as u32).wrapping_mul(0x9E37_79B1) ^ 0x5A);
        self.round = round;
        let (a, b, f) = (r.range(0.0, TAU), r.range(0.0, TAU), r.range(0.09, 0.16));
        let mut walk = 0.0f32;
        for k in 0..COLS * 2 {
            walk = (walk + r.range(-1.6, 1.6)).clamp(-5.0, 5.0);
            let h = 46.0 + 8.0 * (k as f32 * f + a).sin() + 4.0 * (k as f32 * 0.37 + b).sin() + walk;
            self.ground[k] = (h.round() as i32).clamp(27, FLOOR - 1);
        }
        let cells = [3 + r.below(13) as i32, 22 + r.below(14) as i32];
        let mut mults = [2u32, 3, 5];
        for i in 0..2 {
            let k0 = cells[i] * 2;
            let mean = (k0..k0 + 8).map(|k| self.ground[k as usize]).sum::<i32>() / 8;
            let level = (mean / 3 * 3).clamp(33, 54);
            for k in k0..k0 + 8 {
                self.ground[k as usize] = level;
            }
            let m = r.below(3 - i as u32) as usize;
            self.pads[i] = (k0, level, mults[m]);
            mults.swap(m, 2 - i);
        }
        self.target = r.below(2) as usize;
        self.crash = round > 0 && r.below(4) == 0;
        let (k0, level, _) = self.pads[self.target];
        let pad_x = k0 as f32 + 3.5;
        self.start = (if pad_x > 40.0 { r.range(8.0, 22.0) } else { r.range(58.0, 72.0) }, SKY as f32 + 5.0);
        self.aim = if self.crash {
            let x = (pad_x + if r.below(2) == 0 { -9.0 } else { 9.0 }).clamp(6.0, 74.0);
            (x, self.under(x) as f32 - 1.0)
        } else {
            (pad_x, level as f32 - 1.0)
        };
        self.stars.clear();
        for _ in 0..40 {
            let k = 2 + r.below(78) as i32;
            let row = SKY + r.below(30) as i32;
            if row < self.ground[k as usize] - 4 {
                self.stars.push((row, k));
            }
        }
        self.pos = self.start;
        self.vel = (0.0, 0.0);
        self.fuel = 100.0;
        self.scored = false;
        self.tries += 1;

        // the still part of the screen, as a BASIC program would PRINT it
        let s = &mut self.scr;
        s.clear();
        // row 2 stays text for the messages
        for row in (1..=21).filter(|&r| r != 2) {
            s.put(row, 0, START_GRAFIK);
        }
        for col in 1..COLS as i32 {
            s.put(1, col, 0x70);
            s.put(21, col, 0x23);
        }
        for k in 2..COLS as i32 * 2 {
            for row in self.ground[k as usize]..=FLOOR {
                s.setdot(row, k);
            }
        }
        for &(k0, level, mult) in &self.pads {
            let (row, col) = (level / 3 + 1, k0 / 2);
            s.put(row, col, SLUT_GRAFIK);
            s.print(row, col + 1, "X");
            s.num(row, col + 2, mult, 1);
            s.put(row, col + 3, START_GRAFIK);
        }
        self.base = s.mem;
    }
}

impl Effect for Lander {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        let (bt, _) = self.clock.tick(c);
        let round = (bt / ROUND).floor() as i64;
        if round != self.round {
            self.enter(round);
        }
        let tb = bt - round as f32 * ROUND;
        let frac = fract(bt);
        let u = ((tb - 1.0) / 10.0).clamp(0.0, 1.0);
        let down = tb >= 11.0;

        // the autopilot: across to the pad first, then down in steps, one burn a beat
        let x = lerp(self.start.0, self.aim.0, smoothstep((u * 1.35).min(1.0)));
        let ey = if self.crash {
            u * u
        } else if u >= 1.0 {
            1.0
        } else {
            0.6 * ((u * 10.0).floor() + smoothstep(fract(u * 10.0))) / 10.0 + 0.4 * smoothstep(u)
        };
        let y = lerp(self.start.1, self.aim.1, ey).min(self.under(x) as f32 - 1.0);
        if c.dt > 0.0 {
            let v = ((x - self.pos.0) / c.dt, (y - self.pos.1) / c.dt);
            self.vel = (lerp(self.vel.0, v.0, 0.2), lerp(self.vel.1, v.1, 0.2));
        }
        self.pos = (x, y);

        let burn = if down {
            0.0
        } else if tb < 1.0 {
            0.35 + a.bass * 0.4
        } else if self.crash {
            if hash((bt * 6.0) as u32 ^ self.seed) % 3 == 0 { 0.6 * (1.0 - u) } else { 0.0 }
        } else {
            ((1.0 - frac).powi(3) * (0.6 + a.bass) + a.pulse * 0.5).min(1.5)
        };
        self.fuel = (self.fuel - burn * c.dt / a.period.max(0.1) * 6.0).max(0.0);
        let (k0, level, mult) = self.pads[self.target];
        if down && !self.crash && !self.scored {
            self.scored = true;
            self.score += mult * 10 + self.fuel as u32 / 2;
            self.record = self.record.max(self.score);
        }

        let under = self.under(x);
        let s = &mut self.scr;
        s.mem = self.base;

        // stars: each one blinks out now and then, faster with the treble, all lit on a kick
        let twinkle = (c.t * (3.0 + a.treble * 14.0)) as u32;
        for (i, &(row, k)) in self.stars.iter().enumerate() {
            if a.pulse > 0.7 || hash(i as u32 ^ twinkle.wrapping_mul(0x51ED)) % 6 != 0 {
                s.setdot(row, k);
            }
        }
        // the target pad's beacon blinks on the beat
        if frac < 0.5 {
            s.setdot(level - 1, k0);
            s.setdot(level - 1, k0 + 7);
        }

        let (left, bottom) = ((x - 2.5).round() as i32, y.round() as i32);
        if !(self.crash && down) {
            for (sy, line) in CRAFT.iter().enumerate() {
                for (sx, b) in line.bytes().enumerate() {
                    if b == b'X' {
                        s.setdot(bottom - 5 + sy as i32, left + sx as i32);
                    }
                }
            }
        }
        if burn > 0.15 {
            let len = ((burn * 3.0).round() as i32).clamp(1, 4);
            let flick = (c.t * (10.0 + a.treble * 30.0)) as i32;
            for i in 1..=len {
                s.setdot(bottom + i, left + 2 + (flick + i) % 2);
                if i == 1 {
                    s.setdot(bottom + 1, left + 3 - (flick + i) % 2);
                }
            }
        }
        if down && !self.crash && tb < 12.0 {
            // dust kicked up on touchdown
            let d = ((tb - 11.0) * 8.0) as i32;
            for side in [-1, 1] {
                let kx = if side < 0 { left - 1 - d } else { left + 6 + d };
                s.setdot(level - 1 - d / 3, kx);
                s.setdot(level - 2 - d / 2, kx + side);
            }
        }
        if down && self.crash {
            // the wreck: a flash of random blocks, then debris flying and falling
            let tt = (tb - 11.0) * 0.6;
            let (ix, iy) = (self.aim.0, self.aim.1);
            if tb < 11.5 {
                for dr in -2..=1 {
                    for dc in -2..=2 {
                        let (row, col) = ((iy as i32) / 3 + dr, (ix as i32) / 2 + dc);
                        let i = (row * COLS as i32 + col) as usize;
                        // control characters stay, or the rest of the row would change mode
                        if (3..=20).contains(&row) && (1..COLS as i32).contains(&col) && s.mem[i] & 0x7F >= 0x20 {
                            let h = hash(i as u32 ^ (c.t * 20.0) as u32);
                            s.mem[i] = 0x20 | (h as u8 & 0x1F) | if h & 0x100 != 0 { 0x40 } else { 0 };
                        }
                    }
                }
            }
            for i in 0..14u32 {
                let h = hash(self.seed ^ i.wrapping_mul(0x2F6B) ^ round as u32);
                let vx = ((h & 255) as f32 / 255.0 - 0.5) * 30.0 * (1.0 + a.bass * 0.5);
                let vy = -(10.0 + ((h >> 8) & 255) as f32 / 255.0 * 22.0);
                let (px, py) = (ix + vx * tt, iy - 2.0 + vy * tt + 20.0 * tt * tt);
                let k = px.round() as i32;
                if (2..COLS as i32 * 2).contains(&k) && (py as i32) < self.ground[k as usize] && py >= SKY as f32 {
                    s.setdot(py as i32, k);
                }
            }
            for dk in [-3, -1, 2, 4] {
                let k = (ix as i32 + dk).clamp(2, COLS as i32 * 2 - 1);
                s.setdot(self.ground[k as usize] - 1, k);
            }
        }

        // text rows
        s.print(0, 0, "MÅNLANDAREN");
        s.print(0, 14, "POÄNG ");
        s.num(0, 20, self.score % 100_000, 5);
        s.print(0, 28, "REKORD ");
        s.num(0, 35, self.record % 100_000, 5);

        if tb < 1.0 {
            let col = s.print(2, 5, "BANA ");
            let col = s.num(2, col, (round.rem_euclid(99) + 1) as u32, 2);
            let col = s.print(2, col, "  SIKTA PÅ PLATTA X");
            s.num(2, col, mult, 1);
        } else if !down {
            if self.crash {
                s.put(2, 8, FLASH);
                s.print(2, 9, "VARNING! MOTORN HACKAR");
            } else {
                s.centre(2, if burn > 0.35 { "MOTOR PÅ" } else { "FRITT FALL" });
            }
        } else if tb >= 14.0 {
            // typed out like a BASIC INPUT, answer included
            let typed = ((tb - 14.0) * 12.0) as usize + 1;
            for (i, ch) in "NYTT FÖRSÖK (J/N)? J".chars().take(typed).enumerate() {
                s.put(2, 10 + i as i32, code(ch));
            }
        } else if self.crash {
            s.put(2, 10, FLASH);
            s.print(2, 11, "KRASCH!");
            s.put(2, 18, STEADY);
            s.print(2, 19, "FÄRJAN ÄR SKROT");
        } else {
            s.put(2, 7, FLASH);
            let col = s.print(2, 8, "LANDNING OK!  +");
            let col = s.num(2, col, mult * 10 + self.fuel as u32 / 2, 3);
            s.print(2, col, " POÄNG");
        }

        let alt = ((under - 1) as f32 - y).max(0.0) * METRES;
        let col = s.print(22, 0, "HÖJD ");
        let col = s.num(22, col, alt.round() as u32, 3);
        let col = s.print(22, col, " M   FART ");
        let col = s.signed(22, col, (-self.vel.1 * METRES).round() as i32, 2);
        let col = s.print(22, col, " M/S  SIDA ");
        s.signed(22, col, (self.vel.0 * METRES).round() as i32, 2);

        s.print(23, 0, "BRÄNSLE");
        s.put(23, 7, START_GRAFIK);
        let n = (self.fuel / 100.0 * 32.0).round() as i32;
        for i in 0..16 {
            let cell = if 2 * i + 2 <= n {
                0x7F
            } else if 2 * i + 1 == n {
                0x35
            } else {
                0x70
            };
            s.put(23, 8 + i, cell);
        }
        s.put(23, 24, SLUT_GRAFIK);
        let col = s.print(23, 26, "FÖRSÖK ");
        s.num(23, col, self.tries % 100, 2);

        let ink = scale(INK, 0.86 + 0.14 * a.pulse);
        self.scr.render(fb, frac < 0.65, ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swedish_letters_sit_where_the_manual_puts_them() {
        let table = [('¤', 36), ('É', 64), ('Ä', 91), ('Ö', 92), ('Å', 93), ('Ü', 94), ('é', 96), ('ä', 123), ('ö', 124), ('å', 125), ('ü', 126)];
        for (ch, c) in table {
            assert_eq!(code(ch), c, "{ch}");
            assert_ne!(glyph(c), [0; 7], "{ch} has no shape");
        }
        // CHR$ prints as CHR¤
        assert_eq!(glyph(code('$')), glyph(code('¤')));
        assert_eq!(glyph(127), [0b11111; 7]);
    }

    #[test]
    fn setdot_builds_mosaic_codes_like_basic() {
        let mut s = Abc::new();
        for row in 0..24 {
            s.put(row, 0, START_GRAFIK);
        }
        // SETDOT 0,2 lights the top-left dot: code 33 in the manual's table
        s.setdot(0, 2);
        assert_eq!(s.get(0, 1), 33);
        for (r, k) in [(0, 3), (1, 2), (1, 3), (2, 2), (2, 3)] {
            s.setdot(r, k);
        }
        assert_eq!(s.get(0, 1), 127);
        s.clrdot(2, 3);
        assert_eq!(s.get(0, 1), 127 - 64);
        assert!(s.dot(2, 2) && !s.dot(2, 3));
        // columns 0 and 1 belong to START GRAFIK, which is never overwritten
        s.setdot(5, 1);
        assert_eq!(s.get(1, 0), START_GRAFIK);
        // 78×72 dots: rows 0-71, columns 2-79
        s.setdot(71, 79);
        assert!(s.dot(71, 79));
        assert!(Abc::dot_at(72, 79).is_none() && Abc::dot_at(0, 80).is_none() && Abc::dot_at(0, 1).is_none());
    }

    #[test]
    fn graphics_last_until_slut_grafik_and_capitals_stay_letters() {
        let mut s = Abc::new();
        for (col, c) in [START_GRAFIK, b'#', b'A', SLUT_GRAFIK, b'#'].into_iter().enumerate() {
            s.put(0, col as i32, c);
        }
        let mut fb = Fb::new();
        s.render(&mut fb, true, INK);
        let lit = |col: i32, px: i32, py: i32| fb.px[((Y0 + py) * WI + X0 + col * CW + px) as usize] == INK;
        // the control characters show as blanks
        assert!((0..CW).all(|x| (0..CH).all(|y| !lit(0, x, y) && !lit(3, x, y))));
        // '#' (35 = 32 + 1 + 2) in graphics mode is the top pair of dots
        assert!(lit(1, 0, 0) && lit(1, 7, 2) && !lit(1, 0, 3));
        // 'A' stays a letter: its top row is .XXX.
        assert!(lit(2, 2, 1) && !lit(2, 1, 1) && !lit(2, 0, 0));
        // after SLUT GRAFIK '#' is text again, not a block
        assert!(!lit(4, 0, 0) && lit(4, 2, 1));
    }

    #[test]
    fn flashing_text_comes_and_goes() {
        let mut s = Abc::new();
        s.put(0, 0, FLASH);
        s.put(0, 1, b'A');
        s.put(0, 2, STEADY);
        s.put(0, 3, b'A');
        let count = |on: bool| {
            let mut fb = Fb::new();
            s.render(&mut fb, on, INK);
            fb.px.iter().filter(|&&p| p == INK).count()
        };
        let a = glyph(b'A').iter().map(|r| r.count_ones() as usize).sum::<usize>();
        assert_eq!(count(true), 2 * a);
        assert_eq!(count(false), a);
    }
}
