//! Effects that show the artist picture from Spotify in Amiga style: reduced to a
//! few colours with ordered dithering, waved, tunnelled, rotozoomed and halftoned.
//! Without a picture they show a stand-in made from the palette.

use std::f32::consts::{PI, TAU};

use super::*;
use crate::font;

pub fn register(v: &mut Vec<Def>) {
    v.push(Def { name: "PORTRAIT", flags: IMAGE | OWN_TEXT, make: |s| Box::new(Portrait::new(s)) });
    v.push(Def { name: "IMAGE FLAG", flags: IMAGE | OWN_TEXT, make: |s| Box::new(Flag::new(s)) });
    v.push(Def { name: "IMAGE TUNNEL", flags: IMAGE | OWN_TEXT, make: |s| Box::new(ImgTunnel::new(s)) });
    v.push(Def { name: "IMAGE ROTO", flags: IMAGE | OWN_TEXT, make: |s| Box::new(Roto::new(s)) });
    v.push(Def { name: "HALFTONE", flags: IMAGE | OWN_TEXT, make: |s| Box::new(Halftone::new(s)) });
}

const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// The picture this frame: the artist, or a stand-in built from the palette.
struct Source {
    px: Vec<u32>,
    id: u32,
    caption: String,
    credit: String,
}

impl Source {
    fn new() -> Self {
        Source { px: Vec::new(), id: u32::MAX, caption: String::new(), credit: String::new() }
    }

    /// Refresh from the context; returns true when the picture changed.
    fn update(&mut self, c: &Ctx, levels: u32) -> bool {
        let id = c.image.map_or(0, |i| i.id);
        if id == self.id && !self.px.is_empty() {
            return false;
        }
        self.id = id;
        let raw = match c.image {
            Some(img) if img.px.len() == 256 * 256 => {
                self.caption = img.caption.clone();
                self.credit = img.credit.clone();
                img.px.clone()
            }
            _ => {
                self.caption = c.name.to_string();
                self.credit.clear();
                stand_in(c.pal)
            }
        };
        // Amiga look: few levels per channel, 4x4 ordered dither
        self.px = raw
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let (x, y) = (i % 256, i / 256);
                let d = (BAYER[y & 3][x & 3] as f32 + 0.5) / 16.0 - 0.5;
                let q = |v: f32| ((v * levels as f32 + d).round() / levels as f32).clamp(0.0, 1.0);
                let (r, g, b) = unpack(p);
                rgbf(q(r), q(g), q(b))
            })
            .collect();
        true
    }

    #[inline]
    fn at(&self, u: i32, v: i32) -> u32 {
        self.px[((v & 255) * 256 + (u & 255)) as usize]
    }

    /// The artist's name in dancing letters, centred on row `y`.
    fn caption(&self, fb: &mut Fb, c: &Ctx, y: i32) {
        let cap = if self.caption.is_empty() { "UFO" } else { &self.caption };
        let n = cap.chars().count().max(1) as f32;
        let sc = ((W as f32 - 20.0) / (n * 7.0)).clamp(1.0, 3.0);
        let w = n * 7.0 * sc;
        let y = (y as f32 + 10.0 - 3.5 * sc).min(H as f32 - 7.0 * sc - 6.0);
        dancing_text(fb, cap, (W as f32 - w) / 2.0, y, sc, c.time, c.a, c.beats, c.pal);
        // the photographer / licence / source, small in the bottom-right corner
        if !self.credit.is_empty() {
            let cw = font::text_width(&self.credit, 1, false);
            fb.fill_rect(WI - cw - 6, HI - 11, cw + 6, 11, rgb(0, 0, 0));
            font::text(fb, &self.credit, WI - cw - 3, HI - 9, 1, false, rgb(190, 190, 190));
        }
    }
}

/// Concentric palette rings with a sphere in the middle, when there is no artist picture.
fn stand_in(pal: &Palette) -> Vec<u32> {
    let mut v = vec![0; 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            let (dx, dy) = (x as f32 - 128.0, y as f32 - 128.0);
            let r = (dx * dx + dy * dy).sqrt();
            let a = dy.atan2(dx);
            let mut c = pal.at(r / 90.0 + (a * 3.0).sin() * 0.05);
            if r < 60.0 {
                let nz = (1.0 - (r / 60.0).powi(2)).sqrt();
                let l = (-0.4 * dx / 60.0 - 0.5 * dy / 60.0 + 0.75 * nz).max(0.0);
                c = scale(pal.hi, 0.2 + l * 0.9);
            }
            v[y * 256 + x] = c;
        }
    }
    v
}

fn background(fb: &mut Fb, c: &Ctx) {
    for y in 0..HI {
        let k = (y as f32 / HI as f32 * PI).sin();
        fb.row(y, mix(rgb(0, 0, 0), scale(c.pal.at(c.time * 0.03 + y as f32 * 0.002), 0.35), k));
    }
}

// ---------------------------------------------------------------- portrait

/// The picture framed in the middle, scanlines wobbling, copper bars on both sides.
struct Portrait {
    src: Source,
    zoom: f32,
    since: f32,
}

impl Portrait {
    fn new(_seed: u32) -> Self {
        Portrait { src: Source::new(), zoom: 0.0, since: 0.0 }
    }
}

impl Effect for Portrait {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        if self.src.update(c, 5) {
            self.since = 0.0;
        }
        self.since += c.dt;
        background(fb, c);
        // copper bars sweeping behind the frame
        for i in 0..6 {
            let y = H as f32 / 2.0 + (c.time * 1.3 + i as f32 * 1.05).sin() * 80.0;
            fb.copper_bar(y, 5.0 + c.a.bass * 4.0, c.pal.at(i as f32 / 6.0 + c.time * 0.1));
        }
        self.zoom = self.zoom * 0.9 + c.a.pulse * 0.1;
        // the picture appears with a growing size for the first second
        let appear = smoothstep(self.since / 1.0);
        let size = (150.0 + self.zoom * 20.0) * appear;
        if size > 2.0 {
            let (cx, cy) = (W as f32 / 2.0, 82.0);
            let half = size / 2.0;
            let frame = c.pal.hi;
            let y0 = (cy - half) as i32;
            for y in (y0 - 3)..(cy + half) as i32 + 3 {
                let wob = ((y as f32 * 0.09 + c.time * 4.0).sin() * c.a.mid * 4.0) as i32;
                let x0 = (cx - half) as i32 + wob;
                fb.hspan(x0 - 3, x0 + size as i32 + 3, y, frame);
                if y < y0 || y >= y0 + size as i32 {
                    continue;
                }
                let v = ((y - y0) as f32 / size * 256.0) as i32;
                for x in 0..size as i32 {
                    let u = (x as f32 / size * 256.0) as i32;
                    fb.put(x0 + x, y, self.src.at(u, v));
                }
            }
        }
        self.src.caption(fb, c, 170);
    }
}

// ---------------------------------------------------------------- flag

/// The picture waving like cloth, lit by the slope of the wave.
struct Flag {
    src: Source,
    freq: f32,
}

impl Flag {
    fn new(seed: u32) -> Self {
        Flag { src: Source::new(), freq: [1.0, 1.4, 0.7][Rng::new(seed).below(3) as usize] }
    }
}

impl Effect for Flag {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        self.src.update(c, 6);
        background(fb, c);
        let t = c.time;
        let amp = 6.0 + c.a.bass * 10.0 + c.a.pulse * 6.0;
        let (w, h) = (230.0, 150.0);
        let (x0, y0) = ((W as f32 - w) / 2.0, 14.0);
        for sy in 0..(h as i32) {
            for sx in 0..(w as i32) {
                let (fx, fy) = (sx as f32, sy as f32);
                let ph = fx * 0.045 * self.freq + fy * 0.02 - t * 3.0;
                let k = fx / w; // the pole side (left) moves less
                let dy = ph.sin() * amp * k;
                let slope = ph.cos() * k;
                let u = (fx / w * 256.0) as i32;
                let v = (fy / h * 256.0) as i32;
                let col = scale(self.src.at(u, v), 0.75 + slope * 0.35);
                fb.put((x0 + fx) as i32, (y0 + fy + dy) as i32, col);
            }
        }
        // flag pole
        fb.fill_rect(x0 as i32 - 5, y0 as i32 - 6, 4, h as i32 + 30, rgb(200, 200, 210));
        self.src.caption(fb, c, 180);
    }
}

// ---------------------------------------------------------------- tunnel

/// Flying through a tunnel lined with the picture.
struct ImgTunnel {
    src: Source,
    ang: Vec<u8>,
    depth: Vec<u16>,
    z: f32,
    spin: f32,
}

impl ImgTunnel {
    fn new(seed: u32) -> Self {
        let tiles = [2.0, 3.0, 4.0][Rng::new(seed).below(3) as usize];
        let mut ang = vec![0; W * H];
        let mut depth = vec![0; W * H];
        for y in 0..H {
            for x in 0..W {
                let (dx, dy) = (x as f32 - W as f32 / 2.0, y as f32 - H as f32 / 2.0);
                let r = (dx * dx + dy * dy).sqrt().max(1.0);
                ang[y * W + x] = ((dy.atan2(dx) / TAU + 0.5) * 256.0 * tiles) as i32 as u8;
                depth[y * W + x] = (6000.0 / r) as u16;
            }
        }
        ImgTunnel { src: Source::new(), ang, depth, z: 0.0, spin: 0.0 }
    }
}

impl Effect for ImgTunnel {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        self.src.update(c, 5);
        self.z += c.dt * (50.0 + c.a.bass * 120.0);
        self.spin += c.dt * (15.0 + c.a.mid * 25.0);
        let bright = 0.75 + c.a.pulse * 0.5;
        for i in 0..W * H {
            let d = self.depth[i] as i32;
            let u = self.ang[i] as i32 + self.spin as i32;
            let v = d + self.z as i32;
            let fog = (1.0 - d as f32 / 400.0).clamp(0.0, 1.0);
            fb.px[i] = scale(self.src.at(u, v), fog * bright);
        }
        self.src.caption(fb, c, 186);
    }
}

// ---------------------------------------------------------------- rotozoom

/// The picture tiled (mirrored) on a rotating, zooming plane.
struct Roto {
    src: Source,
    angle: f32,
}

impl Roto {
    fn new(_seed: u32) -> Self {
        Roto { src: Source::new(), angle: 0.0 }
    }
}

impl Effect for Roto {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        self.src.update(c, 4);
        self.angle += c.dt * (0.3 + c.a.mid * 0.6);
        let zoom = 1.2 + (c.time * 0.4).sin() * 0.6 - c.a.pulse * 0.3;
        let (s, co) = self.angle.sin_cos();
        let (du, dv) = (co * zoom * 256.0, s * zoom * 256.0);
        let (cx, cy) = (W as f32 / 2.0, H as f32 / 2.0);
        for y in 0..H {
            let fy = y as f32 - cy;
            let mut u = (-cx * co - fy * s) * zoom * 256.0 + c.time * 9000.0;
            let mut v = (-cx * s + fy * co) * zoom * 256.0;
            let row = y * W;
            for x in 0..W {
                let (ui, vi) = ((u as i32) >> 8, (v as i32) >> 8);
                // mirror every other tile so the edges meet
                let mu = if (ui >> 8) & 1 == 0 { ui & 255 } else { 255 - (ui & 255) };
                let mv = if (vi >> 8) & 1 == 0 { vi & 255 } else { 255 - (vi & 255) };
                fb.px[row + x] = self.src.at(mu, mv);
                u += du;
                v += dv;
            }
        }
        // dark band behind the caption
        for y in 168..HI {
            fb.hspan_blend(0, WI, y, rgb(0, 0, 0), 0.55);
        }
        self.src.caption(fb, c, 178);
    }
}

// ---------------------------------------------------------------- halftone

/// The picture as a grid of shaded balls whose size follows brightness and the bass.
struct Halftone {
    src: Source,
    cell: i32,
}

impl Halftone {
    fn new(seed: u32) -> Self {
        Halftone { src: Source::new(), cell: [6, 8, 5][Rng::new(seed).below(3) as usize] }
    }
}

impl Effect for Halftone {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        self.src.update(c, 8);
        fb.clear(scale(c.pal.bg, 0.6));
        let cell = self.cell;
        let size = 168;
        let (x0, y0) = ((WI - size) / 2, 4);
        let t = c.time;
        for gy in 0..size / cell {
            for gx in 0..size / cell {
                let u = (gx * cell * 256 / size) as i32;
                let v = (gy * cell * 256 / size) as i32;
                let p = self.src.at(u + 2, v + 2);
                let (r, g, b) = unpack(p);
                let lum = 0.3 * r + 0.59 * g + 0.11 * b;
                let wave = ((gx as f32 * 0.4 + gy as f32 * 0.3 - t * 3.0).sin() * 0.5 + 0.5) * c.a.bass;
                let rad = (0.3 + lum * 0.8 + wave * 0.3 + c.a.pulse * 0.15) * cell as f32 * 0.55;
                let cx = (x0 + gx * cell) as f32 + cell as f32 / 2.0;
                let cy = (y0 + gy * cell) as f32 + cell as f32 / 2.0;
                fb.ball(cx, cy, rad, (r.max(0.15), g.max(0.15), b.max(0.15)));
            }
        }
        self.src.caption(fb, c, 180);
    }
}
