//! The 384×216 picture blown up onto the TV with whole-number pixels, centred, with the same
//! CRT look as the web page's `#crt` layer: a dark band at the bottom of every source line
//! and a soft vignette. Output pixels are DRM's XRGB8888 (little-endian B, G, R, X).

use ufo_demo::gfx::{H, W};

/// Where the picture sits on a `width`×`height` screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub scale: usize,
    pub x: usize,
    pub y: usize,
}

pub fn fit(width: usize, height: usize) -> Fit {
    let scale = (width / W).min(height / H).max(1);
    Fit { scale, x: width.saturating_sub(W * scale) / 2, y: height.saturating_sub(H * scale) / 2 }
}

pub struct Crt {
    pub fit: Fit,
    pub scanlines: bool,
    /// 0..=256 brightness per source pixel: the vignette
    vignette: Vec<u16>,
    row: Vec<u32>,
    dark: Vec<u32>,
}

/// The page's ufo.js RGBA (little-endian R, G, B, A) to XRGB8888.
#[inline]
fn xrgb(p: u32) -> u32 {
    ((p & 0xFF) << 16) | (p & 0xFF00) | ((p >> 16) & 0xFF)
}

#[inline]
fn shade(p: u32, k: u32) -> u32 {
    let r = ((p >> 16) & 0xFF) * k >> 8;
    let g = ((p >> 8) & 0xFF) * k >> 8;
    let b = (p & 0xFF) * k >> 8;
    (r << 16) | (g << 8) | b
}

impl Crt {
    pub fn new(width: usize, height: usize, scanlines: bool) -> Self {
        let fit = fit(width, height);
        // radial-gradient(ellipse at center, transparent 62%, rgba(0,0,0,.55) 100%): an ellipse
        // through the corners, so 100% is a corner
        let vignette = (0..W * H)
            .map(|i| {
                let dx = ((i % W) as f32 + 0.5) / (W as f32 / 2.0) - 1.0;
                let dy = ((i / W) as f32 + 0.5) / (H as f32 / 2.0) - 1.0;
                let r = ((dx * dx + dy * dy) / 2.0).sqrt();
                let a = 0.55 * ((r - 0.62) / 0.38).clamp(0.0, 1.0);
                ((1.0 - a) * 256.0) as u16
            })
            .collect();
        Crt { fit, scanlines, vignette, row: vec![0; W * fit.scale], dark: vec![0; W * fit.scale] }
    }

    /// Draw `px` (W×H, RGBA) into `out`, a screen `pitch` pixels wide; the borders are left as
    /// they are (black, from when the buffer was made).
    pub fn draw(&mut self, px: &[u32], out: &mut [u32], pitch: usize) {
        let s = self.fit.scale;
        // linear-gradient(transparent 58%, rgba(0,0,0,.32) 58%) per source line
        let dark_from = (0..s).find(|&k| (k as f32 + 0.5) / s as f32 > 0.58).unwrap_or(s);
        for y in 0..H {
            let src = &px[y * W..(y + 1) * W];
            if self.scanlines {
                let vig = &self.vignette[y * W..(y + 1) * W];
                for x in 0..W {
                    let p = shade(xrgb(src[x]), vig[x] as u32);
                    let d = shade(p, 174); // 1 - 0.32
                    for k in 0..s {
                        self.row[x * s + k] = p;
                        self.dark[x * s + k] = d;
                    }
                }
            } else {
                for x in 0..W {
                    self.row[x * s..(x + 1) * s].fill(xrgb(src[x]));
                }
            }
            for k in 0..s {
                let line = if self.scanlines && s > 1 && k >= dark_from { &self.dark } else { &self.row };
                let at = (self.fit.y + y * s + k) * pitch + self.fit.x;
                out[at..at + W * s].copy_from_slice(line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ufo_demo::gfx::rgb;

    #[test]
    fn fits_common_screens() {
        assert_eq!(fit(1920, 1080), Fit { scale: 5, x: 0, y: 0 });
        assert_eq!(fit(1280, 720), Fit { scale: 3, x: 64, y: 36 });
        assert_eq!(fit(3840, 2160), Fit { scale: 10, x: 0, y: 0 });
        assert_eq!(fit(1024, 768), Fit { scale: 2, x: 128, y: 168 });
        assert_eq!(fit(320, 200), Fit { scale: 1, x: 0, y: 0 });
    }

    #[test]
    fn colours_and_scanlines() {
        let mut px = vec![rgb(0, 0, 0); W * H];
        // the middle pixel (no vignette there) is orange
        let (cx, cy) = (W / 2, H / 2);
        px[cy * W + cx] = rgb(0xFF, 0x88, 0x00);
        let (w, h) = (1920, 1080);
        let mut out = vec![0x1234_5678; w * h];

        let mut plain = Crt::new(w, h, false);
        plain.draw(&px, &mut out, w);
        for k in 0..5 {
            assert_eq!(out[(cy * 5 + k) * w + cx * 5 + 2], 0x00FF_8800);
        }
        assert_eq!(out[0], 0);

        let mut crt = Crt::new(w, h, true);
        crt.draw(&px, &mut out, w);
        let line: Vec<u32> = (0..5).map(|k| out[(cy * 5 + k) * w + cx * 5]).collect();
        assert_eq!(&line[..3], &[0x00FF_8800; 3]);
        assert_eq!(line[3], line[4]);
        assert!(line[3] & 0xFF_0000 < 0xC0_0000 && line[3] & 0xFF_0000 > 0x90_0000, "{:x}", line[3]);
        // the corners are darker than the middle
        px.fill(rgb(200, 200, 200));
        crt.draw(&px, &mut out, w);
        assert!((out[0] & 0xFF) < (out[(cy * 5) * w + cx * 5] & 0xFF) / 2);
    }

    #[test]
    fn letterbox_leaves_the_border() {
        let px = vec![rgb(255, 255, 255); W * H];
        let (w, h) = (1280, 720);
        let mut out = vec![0; w * h];
        Crt::new(w, h, false).draw(&px, &mut out, w);
        assert_eq!(out[35 * w + 640], 0);
        assert_eq!(out[36 * w + 64], 0x00FF_FFFF);
        assert_eq!(out[36 * w + 63], 0);
    }
}
