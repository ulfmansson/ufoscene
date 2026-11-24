//! Framebuffer and drawing primitives.
//!
//! Pixels are `u32` laid out so that their little-endian bytes are R, G, B, A,
//! which is exactly what a browser `ImageData` expects.

use std::f32::consts::{PI, TAU};

pub const W: usize = 384;
pub const H: usize = 216;
pub const WI: i32 = W as i32;
pub const HI: i32 = H as i32;

#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    0xFF00_0000 | ((b as u32) << 16) | ((g as u32) << 8) | r as u32
}

#[inline]
pub fn unpack(c: u32) -> (f32, f32, f32) {
    (
        (c & 0xFF) as f32 / 255.0,
        ((c >> 8) & 0xFF) as f32 / 255.0,
        ((c >> 16) & 0xFF) as f32 / 255.0,
    )
}

#[inline]
pub fn rgbf(r: f32, g: f32, b: f32) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    rgb(q(r), q(g), q(b))
}

#[inline]
pub fn scale(c: u32, k: f32) -> u32 {
    let (r, g, b) = unpack(c);
    rgbf(r * k, g * k, b * k)
}

#[inline]
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let (ar, ag, ab) = unpack(a);
    let (br, bg, bb) = unpack(b);
    rgbf(ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t)
}

#[inline]
pub fn add(a: u32, b: u32) -> u32 {
    let (ar, ag, ab) = unpack(a);
    let (br, bg, bb) = unpack(b);
    rgbf(ar + br, ag + bg, ab + bb)
}

/// Cosine palette: `0.5 + 0.5 * cos(2π (t + phase))` per channel.
#[inline]
pub fn pal(t: f32, phase: (f32, f32, f32)) -> u32 {
    let f = |p: f32| 0.5 + 0.5 * (TAU * (t + p)).cos();
    rgbf(f(phase.0), f(phase.1), f(phase.2))
}

#[inline]
pub fn rainbow(t: f32) -> u32 {
    pal(t, (0.0, 0.33, 0.67))
}

/// The classic sky-over-ground chrome gradient used for logos. `v` runs 0..1 top to bottom.
pub fn chrome(v: f32) -> u32 {
    if v < 0.5 {
        mix(rgb(30, 50, 150), rgb(235, 245, 255), v / 0.5)
    } else {
        mix(rgb(70, 30, 10), rgb(255, 205, 90), (v - 0.5) / 0.5)
    }
}

#[inline]
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub struct Fb {
    pub px: Vec<u32>,
}

impl Default for Fb {
    fn default() -> Self {
        Self::new()
    }
}

impl Fb {
    pub fn new() -> Self {
        Fb { px: vec![rgb(0, 0, 0); W * H] }
    }

    pub fn clear(&mut self, c: u32) {
        self.px.fill(c);
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && x < WI && y < HI {
            self.px[y as usize * W + x as usize] = c;
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u32 {
        let x = x.clamp(0, WI - 1) as usize;
        let y = y.clamp(0, HI - 1) as usize;
        self.px[y * W + x]
    }

    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: u32, a: f32) {
        if x >= 0 && y >= 0 && x < WI && y < HI {
            let i = y as usize * W + x as usize;
            self.px[i] = mix(self.px[i], c, a);
        }
    }

    #[inline]
    pub fn plot_add(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && x < WI && y < HI {
            let i = y as usize * W + x as usize;
            self.px[i] = add(self.px[i], c);
        }
    }

    /// Fill `[x0, x1)` on row `y`.
    pub fn hspan(&mut self, x0: i32, x1: i32, y: i32, c: u32) {
        if y < 0 || y >= HI {
            return;
        }
        let (a, b) = (x0.max(0), x1.min(WI));
        if a < b {
            let row = y as usize * W;
            self.px[row + a as usize..row + b as usize].fill(c);
        }
    }

    pub fn hspan_blend(&mut self, x0: i32, x1: i32, y: i32, c: u32, alpha: f32) {
        if y < 0 || y >= HI {
            return;
        }
        let row = y as usize * W;
        for x in x0.max(0)..x1.min(WI) {
            let i = row + x as usize;
            self.px[i] = mix(self.px[i], c, alpha);
        }
    }

    pub fn row(&mut self, y: i32, c: u32) {
        self.hspan(0, WI, y, c);
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        for yy in y.max(0)..(y + h).min(HI) {
            self.hspan(x, x + w, yy, c);
        }
    }

    /// Flat triangle with optional alpha (1.0 = opaque).
    pub fn tri(&mut self, mut p: [(f32, f32); 3], c: u32, alpha: f32) {
        p.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let [(x0, y0), (x1, y1), (x2, y2)] = p;
        if !(y2 - y0 > 1e-4) {
            return;
        }
        let ys = (y0.ceil() as i32).max(0);
        let ye = (y2.ceil() as i32).min(HI);
        for y in ys..ye {
            let fy = y as f32;
            let xa = x0 + (x2 - x0) * (fy - y0) / (y2 - y0);
            let xb = if fy < y1 {
                if y1 - y0 < 1e-4 { x1 } else { x0 + (x1 - x0) * (fy - y0) / (y1 - y0) }
            } else if y2 - y1 < 1e-4 {
                x1
            } else {
                x1 + (x2 - x1) * (fy - y1) / (y2 - y1)
            };
            let (l, r) = if xa < xb { (xa, xb) } else { (xb, xa) };
            let (xs, xe) = (l.ceil() as i32, r.ceil() as i32);
            if alpha >= 0.999 {
                self.hspan(xs, xe, y, c);
            } else {
                self.hspan_blend(xs, xe, y, c, alpha);
            }
        }
    }

    /// Phong-ish shaded sphere sprite (vector balls).
    pub fn ball(&mut self, cx: f32, cy: f32, r: f32, col: (f32, f32, f32)) {
        if !(r >= 0.5) || r > 200.0 {
            return;
        }
        let y0 = ((cy - r).floor() as i32).max(0);
        let y1 = ((cy + r).ceil() as i32).min(HI);
        for y in y0..y1 {
            let dy = (y as f32 + 0.5 - cy) / r;
            if dy.abs() >= 1.0 {
                continue;
            }
            let half = (1.0 - dy * dy).sqrt() * r;
            let xs = ((cx - half).ceil() as i32).max(0);
            let xe = ((cx + half).ceil() as i32).min(WI);
            for x in xs..xe {
                let dx = (x as f32 + 0.5 - cx) / r;
                let nz2 = 1.0 - dx * dx - dy * dy;
                if nz2 <= 0.0 {
                    continue;
                }
                let nz = nz2.sqrt();
                let d = (-0.45 * dx - 0.55 * dy + 0.70 * nz).max(0.0);
                let spec = d.powi(20) * 0.9;
                let k = 0.12 + 0.88 * d;
                self.px[y as usize * W + x as usize] =
                    rgbf(col.0 * k + spec, col.1 * k + spec, col.2 * k + spec);
            }
        }
    }

    /// Horizontal copper bar: a cosine-shaped band of colour `c` centred on `cy`.
    pub fn copper_bar(&mut self, cy: f32, half: f32, c: u32) {
        let y0 = (cy - half).floor() as i32;
        let y1 = (cy + half).ceil() as i32;
        for y in y0..=y1 {
            let d = ((y as f32 - cy) / half).abs();
            if d >= 1.0 {
                continue;
            }
            let k = (d * PI * 0.5).cos();
            let col = add(scale(c, 0.2 + 0.8 * k), scale(rgb(255, 255, 255), k.powi(10) * 0.8));
            self.row(y, col);
        }
    }

    /// Multiply every pixel by `k`.
    pub fn darken(&mut self, k: f32) {
        let k = (k.clamp(0.0, 1.0) * 256.0) as u32;
        for p in self.px.iter_mut() {
            let r = ((*p & 0xFF) * k) >> 8;
            let g = (((*p >> 8) & 0xFF) * k) >> 8;
            let b = (((*p >> 16) & 0xFF) * k) >> 8;
            *p = 0xFF00_0000 | (b << 16) | (g << 8) | r;
        }
    }

    pub fn flash(&mut self, k: f32) {
        if k <= 0.004 {
            return;
        }
        let w = scale(rgb(255, 255, 255), k);
        for p in self.px.iter_mut() {
            *p = add(*p, w);
        }
    }

    /// Reduce to 12-bit colour (4 bits per channel) like the original Amiga chipset.
    pub fn quantize_ocs(&mut self) {
        for p in self.px.iter_mut() {
            let c = *p & 0x00F0_F0F0;
            *p = 0xFF00_0000 | c | (c >> 4);
        }
    }
}
