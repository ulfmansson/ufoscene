//! A home-made 5x7 pixel font (A-Z, ÅÄÖ, digits, punctuation) plus text helpers:
//! plain text, a pre-rendered chrome logo and a sine scroller.

use crate::gfx::{Fb, HI, WI, chrome, mix, rainbow, rgb, scale};

pub const GH: i32 = 7;

#[rustfmt::skip]
pub fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        'Å' | 'å' => [0b00100, 0b01010, 0b00100, 0b01110, 0b10001, 0b11111, 0b10001],
        'Ä' | 'ä' => [0b01010, 0b00000, 0b01110, 0b10001, 0b11111, 0b10001, 0b10001],
        'Ö' | 'ö' => [0b01010, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        ',' => [0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b00100, 0b01000],
        '\'' => [0b00100, 0b00100, 0b01000, 0b00000, 0b00000, 0b00000, 0b00000],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        ':' => [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        '+' => [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000],
        '*' => [0b00000, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0b00000],
        '=' => [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000],
        '<' => [0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010],
        '>' => [0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000],
        '&' => [0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101],
        '#' => [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010],
        _ => [0; 7],
    }
}

/// Upper-case `s` and replace what the font cannot draw: accents become their
/// base letter, anything else a space. At most `max` characters.
pub fn clean(s: &str, max: usize) -> String {
    s.to_uppercase()
        .chars()
        .map(|c| match c {
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'Á' | 'À' | 'Â' | 'Ã' => 'A',
            'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'Ó' | 'Ò' | 'Ô' | 'Õ' => 'O',
            'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
            'Ø' => 'Ö',
            'Æ' => 'Ä',
            'Ñ' => 'N',
            'Ç' => 'C',
            '"' | '’' | '‘' | '´' | '`' => '\'',
            '–' | '—' => '-',
            c if c == ' ' || glyph(c) != [0; 7] => c,
            _ => ' ',
        })
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Is pixel (gx, gy) of `c` set? The bold variant is one pixel wider (6 columns).
#[inline]
pub fn bit(c: char, gx: i32, gy: i32, bold: bool) -> bool {
    if !(0..GH).contains(&gy) {
        return false;
    }
    let row = glyph(c)[gy as usize] as u32;
    if bold {
        // smear each row one pixel to the right: 6 columns, bit 5 is the left edge
        let wide = (row << 1) | row;
        (0..6).contains(&gx) && (wide >> (5 - gx)) & 1 == 1
    } else {
        (0..5).contains(&gx) && (row >> (4 - gx)) & 1 == 1
    }
}

#[inline]
pub fn advance(bold: bool) -> i32 {
    if bold { 7 } else { 6 }
}

pub fn text_width(s: &str, sc: i32, bold: bool) -> i32 {
    s.chars().count() as i32 * advance(bold) * sc - sc
}

/// Plain text, one colour.
pub fn text(fb: &mut Fb, s: &str, x: i32, y: i32, sc: i32, bold: bool, c: u32) {
    let adv = advance(bold) * sc;
    let gw = advance(bold) - 1;
    for (i, ch) in s.chars().enumerate() {
        let ox = x + i as i32 * adv;
        for gy in 0..GH {
            for gx in 0..gw {
                if bit(ch, gx, gy, bold) {
                    fb.fill_rect(ox + gx * sc, y + gy * sc, sc, sc, c);
                }
            }
        }
    }
}

/// A logo rendered once into a mask, drawn each frame with chrome colours,
/// a bevel, an outline, a drop shadow, a moving shine and a per-line wobble.
pub struct Logo {
    pub w: i32,
    pub h: i32,
    mask: Vec<u8>, // 0 empty, 1 outline, 2 body, 3 highlight edge, 4 shadow edge
}

impl Logo {
    pub fn new(s: &str, sc: i32) -> Self {
        let pad = 3;
        let w = text_width(s, sc, true) + pad * 2;
        let h = GH * sc + pad * 2;
        let mut mask = vec![0u8; (w * h) as usize];
        let adv = advance(true) * sc;
        for (i, ch) in s.chars().enumerate() {
            for gy in 0..GH {
                for gx in 0..6 {
                    if bit(ch, gx, gy, true) {
                        for yy in 0..sc {
                            for xx in 0..sc {
                                let x = pad + i as i32 * adv + gx * sc + xx;
                                let y = pad + gy * sc + yy;
                                mask[(y * w + x) as usize] = 2;
                            }
                        }
                    }
                }
            }
        }
        let body = |m: &Vec<u8>, x: i32, y: i32| {
            x >= 0 && y >= 0 && x < w && y < h && m[(y * w + x) as usize] >= 2
        };
        let bevel = (sc / 4).max(1);
        let src = mask.clone();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if src[i] == 2 {
                    let top = (1..=bevel).any(|d| !body(&src, x, y - d) || !body(&src, x - d, y));
                    let bot = (1..=bevel).any(|d| !body(&src, x, y + d) || !body(&src, x + d, y));
                    mask[i] = if top { 3 } else if bot { 4 } else { 2 };
                } else {
                    let near = (-2..=2).any(|dy| (-2..=2).any(|dx| body(&src, x + dx, y + dy)));
                    if near {
                        mask[i] = 1;
                    }
                }
            }
        }
        Logo { w, h, mask }
    }

    pub fn draw(&self, fb: &mut Fb, cx: i32, y0: i32, t: f32, wobble: f32) {
        let x0 = cx - self.w / 2;
        let period = (self.w + self.h + 260) as f32;
        let shine_pos = (t * 150.0) % period - 60.0;
        for pass in 0..2 {
            for ly in 0..self.h {
                let off = ((ly as f32 * 0.09 + t * 3.2).sin() * wobble) as i32;
                let v = ly as f32 / self.h as f32;
                let base = chrome(v);
                for lx in 0..self.w {
                    let m = self.mask[(ly * self.w + lx) as usize];
                    if m == 0 {
                        continue;
                    }
                    let (x, y) = (x0 + lx + off, y0 + ly);
                    if pass == 0 {
                        fb.blend(x + 4, y + 4, rgb(0, 0, 0), 0.55);
                        continue;
                    }
                    let mut c = match m {
                        1 => rgb(10, 8, 30),
                        3 => mix(base, rgb(255, 255, 255), 0.6),
                        4 => scale(base, 0.45),
                        _ => base,
                    };
                    if m >= 2 {
                        let d = (lx + ly) as f32 - shine_pos;
                        if (0.0..9.0).contains(&d) {
                            c = mix(c, rgb(255, 255, 255), 0.75);
                        }
                    }
                    fb.put(x, y, c);
                }
            }
        }
    }
}

/// Sine scroller: one long line of text moving right to left, every pixel
/// column displaced by a sine wave, coloured with a moving copper rainbow.
pub struct Scroller {
    text: Vec<char>,
    pub pos: f32,
}

pub struct ScrollStyle {
    pub y: f32,
    pub sc: i32,
    pub amp: f32,
    pub freq: f32,
    pub speed: f32,
}

impl Scroller {
    pub fn new(s: &str) -> Self {
        Scroller { text: s.chars().collect(), pos: 0.0 }
    }

    fn total(&self, sc: i32) -> f32 {
        (self.text.len() as i32 * advance(true) * sc + WI) as f32
    }

    pub fn advance(&mut self, dt: f32, px_per_s: f32, sc: i32) {
        self.pos = (self.pos + dt * px_per_s) % self.total(sc);
    }

    pub fn draw(&self, fb: &mut Fb, st: &ScrollStyle, t: f32) {
        let adv = advance(true) * st.sc;
        for pass in 0..2 {
            let shadow = pass == 0;
            for sx in 0..WI {
                let tx = sx as f32 + self.pos - WI as f32;
                if tx < 0.0 {
                    continue;
                }
                let txi = tx as i32;
                let ci = (txi / adv) as usize;
                if ci >= self.text.len() {
                    continue;
                }
                let gx = (txi % adv) / st.sc;
                if gx >= 6 {
                    continue;
                }
                let ch = self.text[ci];
                let yoff = (sx as f32 * st.freq + t * st.speed).sin() * st.amp;
                let ytop = (st.y + yoff) as i32;
                for gy in 0..GH {
                    if !bit(ch, gx, gy, true) {
                        continue;
                    }
                    let (x, y) = if shadow { (sx + 2, ytop + 2) } else { (sx, ytop) };
                    for k in 0..st.sc {
                        let yy = y + gy * st.sc + k;
                        if !(0..HI).contains(&yy) {
                            continue;
                        }
                        let c = if shadow {
                            rgb(0, 0, 0)
                        } else {
                            let row = (gy * st.sc + k) as f32 / (GH * st.sc) as f32;
                            let c = rainbow(sx as f32 / WI as f32 * 0.6 + row * 0.25 - t * 0.35);
                            if row < 0.15 { mix(c, rgb(255, 255, 255), 0.6) } else { c }
                        };
                        fb.put(x, yy, c);
                    }
                }
            }
        }
    }
}
