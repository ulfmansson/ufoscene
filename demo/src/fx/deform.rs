//! Deform effects: texture mapping and plane deformation. A rotozoomer, flower and
//! square tunnels, a swirl, floor/ceiling planes, a mode 7 floor, a voxel landscape,
//! bump mapping, a magnifying lens, a neon grid, a planet, a waving flag and a
//! Droste spiral.
//!
//! Textures are 256×256 `u32` buffers ([`Tex`]) generated procedurally in the
//! demo's palette; [`PalTex`] rebuilds one whenever the palette changes. The
//! mapping helpers ([`map_span`], [`map_affine`], [`plane_row`]) take any [`Tex`],
//! so other 256×256 pictures can be mapped the same way.

use std::f32::consts::{PI, TAU};

use super::{
    Ctx, DEFORM, Def, Effect, LOGO_TOP, Palette, Rng, SCROLL_BOTTOM, fract, lerp, norm, rot3, tri,
    xor_texture,
};
use crate::gfx::*;

pub fn register(v: &mut Vec<Def>) {
    let mut add = |name: &'static str, flags: u32, make: fn(u32) -> Box<dyn Effect>| {
        v.push(Def { name, flags: DEFORM | flags, make });
    };
    add("ROTOZOOM", SCROLL_BOTTOM, |s| Box::new(Rotozoom::new(s)));
    add("FLOWER TUNNEL", 0, |s| Box::new(FlowerTunnel::new(s)));
    add("SQUARE TUNNEL", 0, |s| Box::new(SquareTunnel::new(s)));
    add("SWIRL", 0, |s| Box::new(Swirl::new(s)));
    add("PLANES", 0, |s| Box::new(Planes::new(s)));
    add("MODE7", LOGO_TOP, |s| Box::new(Mode7::new(s)));
    add("VOXEL", LOGO_TOP, |s| Box::new(Voxel::new(s)));
    add("BUMPMAP", 0, |s| Box::new(Bump::new(s)));
    add("LENS", 0, |s| Box::new(Lens::new(s)));
    add("OUTRUN", LOGO_TOP | SCROLL_BOTTOM, |s| Box::new(Outrun::new(s)));
    add("PLANET", LOGO_TOP | SCROLL_BOTTOM, |s| Box::new(Planet::new(s)));
    add("FLAG", LOGO_TOP | SCROLL_BOTTOM, |s| Box::new(Flag::new(s)));
    add("DROSTE", 0, |s| Box::new(Droste::new(s)));
}

const WF: f32 = W as f32;
const HF: f32 = H as f32;
const WHITE: u32 = 0xFFFF_FFFF;

// ---------------------------------------------------------------- fast colour maths

/// `c · k / 256` per channel, `k` in 0..=256.
#[inline]
fn shade(c: u32, k: u32) -> u32 {
    let k = k.min(256);
    let rb = (((c & 0x00FF_00FF) * k) >> 8) & 0x00FF_00FF;
    let g = (((c & 0x0000_FF00) * k) >> 8) & 0x0000_FF00;
    0xFF00_0000 | rb | g
}

/// Like [`shade`] but `k` above 256 brightens, saturating each channel.
#[inline]
fn bright(c: u32, k: u32) -> u32 {
    if k <= 256 {
        return shade(c, k);
    }
    let ch = |s: u32| ((((c >> s) & 0xFF) * k) >> 8).min(255) << s;
    0xFF00_0000 | ch(0) | ch(8) | ch(16)
}

/// From `a` towards `b` by `k`/256.
#[inline]
fn blend(a: u32, b: u32, k: u32) -> u32 {
    let k = k.min(256);
    let ik = 256 - k;
    let rb = (((a & 0x00FF_00FF) * ik + (b & 0x00FF_00FF) * k) >> 8) & 0x00FF_00FF;
    let g = (((a & 0x0000_FF00) * ik + (b & 0x0000_FF00) * k) >> 8) & 0x0000_FF00;
    0xFF00_0000 | rb | g
}

/// Per-channel saturating add.
#[inline]
fn add_sat(a: u32, b: u32) -> u32 {
    let ch = |s: u32| (((a >> s) & 0xFF) + ((b >> s) & 0xFF)).min(255) << s;
    0xFF00_0000 | ch(0) | ch(8) | ch(16)
}

/// 0..1 as a 0..=256 weight.
#[inline]
fn k256(x: f32) -> u32 {
    (x.clamp(0.0, 1.0) * 256.0) as u32
}

// ---------------------------------------------------------------- noise

#[inline]
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8DA6_B343)
        ^ (y as u32).wrapping_mul(0xD816_3841)
        ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFF_FFFF) as f32 / 16_777_216.0
}

/// Value noise on a `cells`×`cells` lattice that wraps (x, y in lattice units).
fn vnoise(x: f32, y: f32, cells: i32, seed: u32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let s = |f: f32| f * f * (3.0 - 2.0 * f);
    let (sx, sy) = (s(x - xf), s(y - yf));
    let (x0, y0) = ((xf as i32).rem_euclid(cells), (yf as i32).rem_euclid(cells));
    let (x1, y1) = ((x0 + 1) % cells, (y0 + 1) % cells);
    let top = lerp(hash(x0, y0, seed), hash(x1, y0, seed), sx);
    let bot = lerp(hash(x0, y1, seed), hash(x1, y1, seed), sx);
    lerp(top, bot, sy)
}

/// Fractal noise in 0..1 over texture coordinates 0..256 that tiles every 256;
/// the first octave has `cells` lattice cells across.
fn fbm(x: f32, y: f32, cells: i32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut total, mut n) = (0.0, 1.0, 0.0, cells);
    for o in 0..octaves {
        let k = n as f32 / 256.0;
        sum += vnoise(x * k, y * k, n, seed.wrapping_add(o * 1013)) * amp;
        total += amp;
        amp *= 0.5;
        n *= 2;
    }
    sum / total
}

/// Stretch noise (which clusters around 0.5) to use more of 0..1.
fn contrast(n: f32, k: f32) -> f32 {
    ((n - 0.5) * k + 0.5).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------- textures and mapping

/// Texture side length.
pub const TS: usize = 256;

/// A 256×256 texture of packed pixels (`gfx` layout); coordinates wrap around.
pub struct Tex {
    pub px: Vec<u32>,
}

impl Tex {
    /// Wrap a row-major 256×256 buffer, e.g. a picture handed over by the page.
    pub fn new(px: Vec<u32>) -> Self {
        assert_eq!(px.len(), TS * TS, "a texture is 256×256");
        Tex { px }
    }

    pub fn from_fn(mut f: impl FnMut(usize, usize) -> u32) -> Self {
        let mut px = vec![0; TS * TS];
        for y in 0..TS {
            for x in 0..TS {
                px[y * TS + x] = f(x, y);
            }
        }
        Tex { px }
    }

    /// Texel at integer coordinates, wrapping.
    #[inline]
    pub fn at(&self, u: i32, v: i32) -> u32 {
        self.px[(((v & 255) << 8) | (u & 255)) as usize]
    }
}

/// Fill one row with texels stepping linearly through `tex` from `uv` by `step`
/// per pixel (16.16 fixed point inside; wrapping is exact since 2^16 is a
/// multiple of 256). `post` adjusts each texel (shade, fog, ...).
pub fn map_span(
    row: &mut [u32],
    tex: &Tex,
    uv: (f32, f32),
    step: (f32, f32),
    post: impl Fn(u32) -> u32,
) {
    let fx = |f: f32| (f as f64 * 65536.0) as i64 as i32;
    let (mut u, mut v) = (fx(uv.0), fx(uv.1));
    let (du, dv) = (fx(step.0), fx(step.1));
    for p in row {
        *p = post(tex.at(u >> 16, v >> 16));
        u = u.wrapping_add(du);
        v = v.wrapping_add(dv);
    }
}

/// Map `tex` over the whole screen with an affine transform: the screen centre
/// shows texel `centre`, one pixel right moves by `ex`, one pixel down by `ey`.
/// `shift(y)` moves a row sideways (in pixels) for wobbles.
pub fn map_affine(
    fb: &mut Fb,
    tex: &Tex,
    centre: (f32, f32),
    ex: (f32, f32),
    ey: (f32, f32),
    mut shift: impl FnMut(usize) -> f32,
    post: impl Fn(u32) -> u32,
) {
    for y in 0..H {
        let sx = -WF / 2.0 - shift(y);
        let sy = y as f32 - HF / 2.0;
        let uv = (centre.0 + sx * ex.0 + sy * ey.0, centre.1 + sx * ex.1 + sy * ey.1);
        map_span(&mut fb.px[y * W..(y + 1) * W], tex, uv, ex, &post);
    }
}

/// One screen row of a perspective plane `depth` texels in front of a camera at
/// `cam` heading `yaw` (given as sin, cos); `fov` is the focal length in pixels.
pub fn plane_row(
    row: &mut [u32],
    tex: &Tex,
    cam: (f32, f32),
    yaw: (f32, f32),
    depth: f32,
    fov: f32,
    post: impl Fn(u32) -> u32,
) {
    let (s, c) = yaw;
    let step = depth / fov;
    let lat = -WF / 2.0 * step;
    let uv = (cam.0 + depth * s + lat * c, cam.1 + depth * c - lat * s);
    map_span(row, tex, uv, (step * c, -step * s), post);
}

pub type TexGen = fn(&Palette, u32) -> Tex;

/// A procedural texture in palette colours, rebuilt whenever the palette changes.
pub struct PalTex {
    generate: TexGen,
    arg: u32,
    key: Option<u32>,
    tex: Tex,
}

impl PalTex {
    pub fn new(generate: TexGen, arg: u32) -> Self {
        PalTex { generate, arg, key: None, tex: Tex { px: Vec::new() } }
    }

    pub fn get(&mut self, pal: &Palette) -> &Tex {
        let k = pal_key(pal);
        if self.key != Some(k) {
            self.tex = (self.generate)(pal, self.arg);
            self.key = Some(k);
        }
        &self.tex
    }
}

/// A fingerprint of a palette, to notice when it changes.
fn pal_key(p: &Palette) -> u32 {
    [p.at8(0), p.at8(37), p.at8(101), p.at8(170), p.at8(230), p.ramp8(128), p.bg, p.hi]
        .iter()
        .fold(0x811C_9DC5, |h, &c| (h ^ c).wrapping_mul(0x0100_0193))
}

/// The tiling patterns effects choose between.
const PATTERNS: [TexGen; 4] = [tex_xor, tex_bricks, tex_tiles, tex_marble];

fn pick_pattern(r: &mut Rng) -> TexGen {
    PATTERNS[r.below(PATTERNS.len() as u32) as usize]
}

/// The library's XOR pattern.
pub fn tex_xor(pal: &Palette, _: u32) -> Tex {
    Tex::new(xor_texture(pal))
}

/// Bevelled bricks, each its own palette shade, with mortar lines.
pub fn tex_bricks(pal: &Palette, seed: u32) -> Tex {
    let mortar = scale(pal.at(0.8), 0.4);
    Tex::from_fn(|x, y| {
        let row = y / 32;
        let xs = (x + row % 2 * 32) % 256;
        let (col, xx, yy) = (xs / 64, xs % 64, y % 32);
        if xx < 3 || yy < 3 {
            return mortar;
        }
        let id = hash(col as i32, row as i32, seed);
        let grain = hash(x as i32, y as i32, seed ^ 99) * 0.2;
        let c = scale(pal.at(0.05 + id * 0.4), 0.75 + grain);
        if xx < 6 || yy < 6 {
            mix(c, WHITE, 0.35)
        } else if xx > 59 || yy > 28 {
            scale(c, 0.6)
        } else {
            c
        }
    })
}

/// Checkered bevelled tiles with a raised disc and a groove in each.
pub fn tex_tiles(pal: &Palette, seed: u32) -> Tex {
    let h = hash(1, 2, seed);
    let (a, b) = (pal.at(0.05 + h * 0.2), pal.at(0.5 + h * 0.2));
    Tex::from_fn(|x, y| {
        let (cx, cy) = (x % 32, y % 32);
        let base = if (x / 32 + y / 32) % 2 == 0 { a } else { b };
        let edge = cx.min(cy).min(31 - cx).min(31 - cy);
        let (dx, dy) = (cx as f32 - 15.5, cy as f32 - 15.5);
        let r = (dx * dx + dy * dy).sqrt();
        if edge < 2 {
            scale(base, 0.35)
        } else if edge < 5 {
            if cx.min(cy) == edge { mix(base, WHITE, 0.45) } else { scale(base, 0.55) }
        } else if r < 8.0 {
            let lit = (-dx - dy) / 16.0;
            mix(base, pal.hi, 0.45 + lit * 0.5)
        } else if (r - 10.5).abs() < 1.3 {
            scale(base, 0.45)
        } else {
            base
        }
    })
}

/// Marble: fractal noise through the palette with bright veins.
pub fn tex_marble(pal: &Palette, seed: u32) -> Tex {
    Tex::from_fn(|x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let n = contrast(fbm(fx, fy, 4, 5, seed), 1.8);
        let vein = (fx / 256.0 * TAU * 2.0 + fy / 256.0 * TAU + n * 9.0).sin().abs();
        let c = scale(pal.at(0.15 + n * 0.6), 0.55 + n * 0.6);
        mix(c, pal.hi, (1.0 - vein).powi(6) * 0.9)
    })
}

/// A landscape picture: sky with stars and a moon, three mountain ridges and a
/// lake mirroring them. Tiles horizontally; `arg & 1` adds a bevelled frame.
pub fn tex_picture(pal: &Palette, arg: u32) -> Tex {
    let mut r = Rng::new(arg.wrapping_mul(0x9E37) ^ 0x5EED);
    let ph: Vec<f32> = (0..15).map(|_| r.f() * TAU).collect();
    let (mx, my, mr) = (40.0 + r.f() * 176.0, 70.0, 24.0);
    let horizon = 168.0;
    let top = scale(pal.at(0.62), 0.6);
    let low = mix(pal.at(0.92), WHITE, 0.35);
    let moon = mix(pal.hi, WHITE, 0.5);
    let layers = [
        mix(low, pal.at(0.25), 0.45),
        scale(pal.at(0.3), 0.8),
        scale(pal.at(0.15), 0.5),
    ];
    let ridge = |x: f32, l: usize| -> f32 {
        (0..5)
            .map(|k| {
                let f = (k + 1 + l) as f32;
                (f * TAU * x / 256.0 + ph[l * 5 + k]).sin() / f
            })
            .sum()
    };
    let top_of = |x: f32, l: usize| {
        let (base, amp) = [(50.0, 22.0), (32.0, 16.0), (14.0, 10.0)][l];
        let mut t = horizon - base - amp * ridge(x, l);
        if l == 2 {
            // a row of pines on the near ridge
            let col = (x / 7.0).floor();
            if hash(col as i32 & 255, 9, arg) > 0.35 {
                t -= tri(x / 7.0) * (6.0 + hash(col as i32 & 255, 10, arg) * 8.0);
            }
        }
        t
    };
    let upper = |x: f32, y: f32| -> u32 {
        for l in (0..3).rev() {
            let t = top_of(x, l);
            if y >= t {
                let slope = (top_of(x + 2.0, l) - top_of(x - 2.0, l)) * 0.25;
                let n = fbm(x, y, 16, 3, arg + l as u32);
                let mut c = bright(layers[l], k256((0.55 + n * 0.5 + slope * 0.5).clamp(0.3, 1.4) / 1.4) * 14 / 10);
                if l == 0 && y - t < 5.0 + n * 6.0 {
                    c = mix(c, WHITE, 0.6);
                }
                return if y - t < 1.5 { mix(c, pal.hi, 0.5) } else { c };
            }
        }
        let mut c = blend(top, low, k256((y / horizon).powf(1.3)));
        let dx = (x - mx + 128.0).rem_euclid(256.0) - 128.0;
        let d = (dx * dx + (y - my) * (y - my)).sqrt();
        if d < mr {
            let crater = contrast(fbm(x, y, 32, 3, arg ^ 7), 2.0);
            let k = (0.65 + crater * 0.45) * (1.0 - (d / mr).powi(4) * 0.35);
            c = scale(moon, k);
        } else if d < mr * 2.4 {
            c = mix(c, moon, (1.0 - (d - mr) / (mr * 1.4)).powi(2) * 0.45);
        } else if y < 130.0 && hash(x as i32, y as i32, arg) > 0.992 {
            c = mix(c, WHITE, 1.0 - y / 150.0);
        }
        c
    };
    Tex::from_fn(|x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let mut c = if fy < horizon {
            upper(fx, fy)
        } else {
            let depth = fy - horizon;
            let wob = (fy * 0.9).sin() * depth * 0.06;
            let refl = upper((fx + wob).rem_euclid(256.0), 2.0 * horizon - fy - 1.0);
            let c = blend(scale(refl, 0.7), scale(pal.at(0.6), 0.6), 50);
            if (fy * 1.7 + (fx * 0.05).sin() * 2.0).sin() > 0.93 { mix(c, low, 0.45) } else { c }
        };
        if arg & 1 == 1 {
            let e = x.min(y).min(255 - x).min(255 - y);
            let fc = pal.ramp(0.7);
            if e < 2 || (10..12).contains(&e) {
                c = scale(fc, 0.3);
            } else if e < 10 {
                c = if x.min(y) == e { mix(fc, WHITE, 0.3) } else { scale(fc, 0.65) };
            }
        }
        c
    })
}

/// Planet surfaces, u = longitude, v = latitude: 0 oceans and continents with ice
/// caps and clouds, 1 a banded gas giant with a storm, 2 a cracked lava world.
pub fn tex_planet(pal: &Palette, arg: u32) -> Tex {
    let (kind, seed) = (arg % 3, arg / 3 + 17);
    let ice = mix(pal.hi, WHITE, 0.6);
    Tex::from_fn(|x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let lat = (fy - 127.5).abs() / 128.0;
        match kind {
            0 => {
                let n = contrast(fbm(fx, fy, 4, 6, seed), 2.0);
                let mut c = if n < 0.5 {
                    scale(pal.at(0.6), 0.35 + n * 0.8)
                } else {
                    scale(pal.at(0.15 + (n - 0.5) * 0.8), 0.7 + (n - 0.5) * 0.6)
                };
                if lat > 0.8 - n * 0.15 {
                    c = ice;
                }
                let cl = contrast(fbm(fx, fy, 8, 4, seed + 5), 2.2);
                if cl > 0.6 { mix(c, rgb(240, 245, 255), ((cl - 0.6) * 3.0).min(0.85)) } else { c }
            }
            1 => {
                let n = fbm(fx, fy, 4, 4, seed);
                let band = (fy / 256.0 * TAU * 6.0 + n * 5.0).sin();
                let c = scale(pal.at(0.1 + band * 0.12 + n * 0.3), 0.8 + band * 0.2);
                let (dx, dy) = ((fx - 170.0) / 22.0, (fy - 165.0) / 10.0);
                let d = dx * dx + dy * dy;
                if d < 1.0 { mix(c, pal.ramp(0.85), (1.0 - d) * 0.85) } else { c }
            }
            _ => {
                let n = fbm(fx, fy, 6, 5, seed);
                let crack = (1.0 - (n - 0.5).abs() * 2.0).powi(12);
                let rock = scale(pal.at(0.75), 0.25 + n * 0.4);
                mix(rock, pal.ramp(0.95), crack)
            }
        }
    })
}

// ---------------------------------------------------------------- 1. ROTOZOOM

/// A tiled texture spinning and zooming: the screen as a window onto an endless rotated plane.
struct Rotozoom {
    tex: PalTex,
    angle: f32,
    dir: f32,
    wobble: f32,
    pos: (f32, f32),
    vel: (f32, f32),
}

impl Rotozoom {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        Rotozoom {
            tex: PalTex::new(pick_pattern(&mut r), seed),
            angle: r.f() * TAU,
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
            wobble: [0.0, 0.0, 1.0][r.below(3) as usize],
            pos: (0.0, 0.0),
            vel: (r.range(15.0, 40.0), r.range(-30.0, 30.0)),
        }
    }
}

impl Effect for Rotozoom {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let a = c.a;
        self.angle += c.dt * (0.15 + a.mid * 1.3) * self.dir;
        self.pos.0 = (self.pos.0 + self.vel.0 * c.dt).rem_euclid(256.0);
        self.pos.1 = (self.pos.1 + self.vel.1 * c.dt).rem_euclid(256.0);
        // texels per pixel: breathes slowly, bass and beats zoom in
        let s = (0.95 + 0.6 * (c.t * 0.4).sin()) * (1.0 - 0.4 * a.bass) * (1.0 - 0.2 * a.pulse) + 0.12;
        let (sn, cs) = self.angle.sin_cos();
        let k = 256 + (a.pulse * 100.0) as u32;
        let (wob, t, tr) = (self.wobble, c.t, a.treble);
        let tex = self.tex.get(c.pal);
        map_affine(
            fb,
            tex,
            self.pos,
            (cs * s, sn * s),
            (-sn * s, cs * s),
            |y| wob * (y as f32 * 0.05 + t * 3.0).sin() * (6.0 + tr * 14.0),
            |p| bright(p, k),
        );
    }
}

// ---------------------------------------------------------------- polar lookup table

const TW: usize = W * 2;
const TH: usize = H * 2;

/// Angle and distance for every pixel of a screen twice the framebuffer's size,
/// so a tunnel's centre can wander by looking at a window of it.
struct Polar {
    /// 0..65536 per turn.
    ang: Vec<u16>,
    dist: Vec<f32>,
}

impl Polar {
    fn new() -> Self {
        let mut p = Polar { ang: vec![0; TW * TH], dist: vec![0.0; TW * TH] };
        for ty in 0..TH {
            for tx in 0..TW {
                let dx = tx as f32 - WF + 0.5;
                let dy = ty as f32 - HF + 0.5;
                let i = ty * TW + tx;
                p.ang[i] = ((dy.atan2(dx) / TAU).rem_euclid(1.0) * 65536.0) as u32 as u16;
                p.dist[i] = (dx * dx + dy * dy).sqrt().max(1.0);
            }
        }
        p
    }

    /// Top-left of the window that puts the table's centre at screen (cx, cy).
    fn window(cx: f32, cy: f32) -> (usize, usize) {
        ((WF - cx).clamp(0.0, WF) as usize, (HF - cy).clamp(0.0, HF) as usize)
    }
}

// ---------------------------------------------------------------- 2. FLOWER TUNNEL

/// A texture tunnel whose radius is modulated by the angle, so its walls form petals.
struct FlowerTunnel {
    tex: PalTex,
    polar: Polar,
    /// Radius factor per angle (1024 bins), refreshed every frame.
    pf: Vec<f32>,
    petals: f32,
    twist: i32,
    dir: f32,
    depth: f32,
    spin: f32,
    rot: f32,
}

impl FlowerTunnel {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        FlowerTunnel {
            tex: PalTex::new(pick_pattern(&mut r), seed),
            polar: Polar::new(),
            pf: vec![1.0; 1024],
            petals: (3 + r.below(5)) as f32,
            twist: [0, 48, -48][r.below(3) as usize],
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
            depth: 0.0,
            spin: 0.0,
            rot: 0.0,
        }
    }
}

impl Effect for FlowerTunnel {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t) = (c.a, c.dt, c.t);
        self.depth = (self.depth + dt * (60.0 + 170.0 * a.bass)).rem_euclid(256.0);
        self.spin = (self.spin + dt * (12.0 + 30.0 * a.mid) * self.dir).rem_euclid(256.0);
        self.rot = (self.rot - dt * (0.6 + 2.2 * a.mid) * self.dir).rem_euclid(TAU);
        let amp = (0.2 + 0.3 * a.bass + 0.25 * a.pulse).min(0.75);
        for (i, p) in self.pf.iter_mut().enumerate() {
            let th = i as f32 / 1024.0 * TAU;
            *p = 1.0 + amp * (self.petals * th + self.rot).sin();
        }
        let (ox, oy) =
            Polar::window(WF / 2.0 + (t * 0.43).sin() * WF * 0.22, HF / 2.0 + (t * 0.31).cos() * HF * 0.22);
        let (depth, spin, twist) = (self.depth as i32, self.spin as i32, self.twist);
        let fog = 1.0 / (HF * 0.5);
        let lit = 230.0 + a.pulse * 130.0;
        let tex = self.tex.get(c.pal);
        for y in 0..H {
            let row = (y + oy) * TW + ox;
            let out = &mut fb.px[y * W..(y + 1) * W];
            let angs = &self.polar.ang[row..row + W];
            let dists = &self.polar.dist[row..row + W];
            for ((o, &an), &d) in out.iter_mut().zip(angs).zip(dists) {
                let de = d * self.pf[(an >> 6) as usize];
                let v = (4000.0 / de) as i32 + depth;
                let u = (an as i32 >> 7) + spin + ((v * twist) >> 8);
                *o = bright(tex.at(u, v), ((de * fog).min(1.0) * lit) as u32);
            }
        }
    }
}

// ---------------------------------------------------------------- 3. SQUARE TUNNEL

/// A tunnel with a square cross-section (Chebyshev distance), rotating, with
/// frames that flash on the beat.
struct SquareTunnel {
    tex: PalTex,
    dir: f32,
    /// log2 of the distance between rings, in texels.
    ring_shift: i32,
    depth: f32,
    spin: f32,
    angle: f32,
}

impl SquareTunnel {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        SquareTunnel {
            tex: PalTex::new(pick_pattern(&mut r), seed),
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
            ring_shift: 5 + r.below(2) as i32,
            depth: 0.0,
            spin: 0.0,
            angle: r.f() * TAU,
        }
    }
}

impl Effect for SquareTunnel {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t) = (c.a, c.dt, c.t);
        self.depth = (self.depth + dt * (70.0 + 180.0 * a.bass)).rem_euclid(2048.0);
        self.spin = (self.spin + dt * (10.0 + 20.0 * a.mid)).rem_euclid(256.0);
        self.angle += dt * (0.25 + 1.2 * a.mid) * self.dir;
        let (cx, cy) = (WF / 2.0 + (t * 0.5).sin() * 60.0, HF / 2.0 + (t * 0.7).cos() * 32.0);
        let (sn, cs) = self.angle.sin_cos();
        // eight ring colours; the one matching the beat count flashes hardest
        let hot = (c.beats & 7) as usize;
        let rings: [u32; 8] = std::array::from_fn(|j| {
            let k = if j == hot { 280.0 + 420.0 * a.pulse } else { 250.0 + 80.0 * a.pulse };
            bright(pal_ring(c, j), k as u32)
        });
        let (rs, rw) = (self.ring_shift, 4 + self.ring_shift - 5);
        let (depth, spin) = (self.depth, self.spin as i32);
        let fog = 1.0 / (HF * 0.5);
        let tex = self.tex.get(c.pal);
        for y in 0..H {
            let dy = y as f32 + 0.5 - cy;
            let (rx0, ry0) = ((0.5 - cx) * cs + dy * sn, -(0.5 - cx) * sn + dy * cs);
            let out = &mut fb.px[y * W..(y + 1) * W];
            for (x, o) in out.iter_mut().enumerate() {
                let (rx, ry) = (rx0 + x as f32 * cs, ry0 - x as f32 * sn);
                let (ax, ay) = (rx.abs(), ry.abs());
                let d = ax.max(ay).max(1.0);
                let inv = 1.0 / d;
                // walk the square's perimeter: right, bottom, left, top
                let (side, s) = if ax >= ay {
                    if rx > 0.0 { (0.0, ry * inv) } else { (2.0, -ry * inv) }
                } else if ry > 0.0 {
                    (1.0, -rx * inv)
                } else {
                    (3.0, rx * inv)
                };
                let z = (2400.0 * inv + depth) as i32;
                let k = ((d * fog).min(1.0) * 256.0) as u32;
                *o = if z & ((1 << rs) - 1) < rw {
                    shade(rings[((z >> rs) & 7) as usize], k)
                } else {
                    let u = ((side * 64.0 + (s + 1.0) * 32.0) * 2.0) as i32 + spin;
                    shade(tex.at(u, z), k)
                };
            }
        }
    }
}

fn pal_ring(c: &Ctx, j: usize) -> u32 {
    mix(c.pal.at(j as f32 / 8.0 + c.time * 0.05), WHITE, 0.2)
}

// ---------------------------------------------------------------- 4. SWIRL

/// A texture twisted around a wandering centre: rotation grows towards the middle.
struct Swirl {
    tex: PalTex,
    /// Per radius (pixels): rotation (cos, sin) times zoom, for one or two swirls.
    tab: [Vec<(f32, f32)>; 2],
    two: bool,
    dir: f32,
    falloff: f32,
    rot: f32,
    pos: (f32, f32),
}

impl Swirl {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        Swirl {
            tex: PalTex::new(pick_pattern(&mut r), seed),
            tab: [vec![(1.0, 0.0); 512], vec![(1.0, 0.0); 512]],
            two: r.below(3) == 0,
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
            falloff: r.range(70.0, 110.0),
            rot: 0.0,
            pos: (0.0, 0.0),
        }
    }
}

impl Effect for Swirl {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t) = (c.a, c.dt, c.t);
        self.rot += dt * 0.15 * self.dir;
        self.pos.0 = (self.pos.0 + dt * 20.0).rem_euclid(256.0);
        self.pos.1 = (self.pos.1 + dt * 13.0).rem_euclid(256.0);
        let twist = ((2.0 + 3.0 * a.mid) * (t * 0.35).sin() + 1.5 * a.pulse) * self.dir;
        let zoom = 0.8 - 0.3 * a.bass;
        for r in 0..512 {
            let rr = r as f32;
            let fall = (-rr / self.falloff).exp();
            let pinch = 1.0 - 0.35 * a.pulse * (-(rr / 60.0).powi(2)).exp();
            let th = self.rot + twist * fall;
            self.tab[0][r] = (th.cos() * zoom * pinch, th.sin() * zoom * pinch);
            let th2 = -twist * 0.8 * fall;
            self.tab[1][r] = (th2.cos(), th2.sin());
        }
        let c1 = (WF / 2.0 + (t * 0.37).sin() * WF * 0.3, HF / 2.0 + (t * 0.53 + 1.0).sin() * HF * 0.3);
        let c2 = (WF - c1.0, HF - c1.1);
        let (two, pos) = (self.two, self.pos);
        let k = 240 + (a.pulse * 90.0) as u32;
        let tex = self.tex.get(c.pal);
        let [tab0, tab1] = &self.tab;
        let swirl = |tab: &[(f32, f32)], x: f32, y: f32| {
            let r = (x * x + y * y).sqrt();
            let (cs, sn) = tab[(r as usize).min(511)];
            (x * cs - y * sn, x * sn + y * cs)
        };
        for y in 0..H {
            let out = &mut fb.px[y * W..(y + 1) * W];
            for (x, o) in out.iter_mut().enumerate() {
                let (mut u, mut v) = swirl(tab0, x as f32 - c1.0, y as f32 - c1.1);
                if two {
                    let (u2, v2) = swirl(tab1, u + c1.0 - c2.0, v + c1.1 - c2.1);
                    u = u2 + c2.0 - c1.0;
                    v = v2 + c2.1 - c1.1;
                }
                *o = bright(tex.at((u + pos.0) as i32, (v + pos.1) as i32), k);
            }
        }
    }
}

// ---------------------------------------------------------------- 5. PLANES

/// Infinite textured floor and ceiling (distance = 1/|y|) with fog, flying forward.
struct Planes {
    floor: PalTex,
    ceil: PalTex,
    cam: (f32, f32),
    yaw: f32,
    turn: f32,
    fog_at: f32,
}

impl Planes {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let f = r.below(4) as usize;
        let g = (f + 1 + r.below(3) as usize) % 4;
        Planes {
            floor: PalTex::new(PATTERNS[f], seed),
            ceil: PalTex::new(PATTERNS[g], seed + 1),
            cam: (0.0, 0.0),
            yaw: 0.0,
            turn: [0.0, 0.15, -0.15][r.below(3) as usize],
            fog_at: r.range(0.3, 0.9),
        }
    }
}

impl Effect for Planes {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t) = (c.a, c.dt, c.t);
        self.yaw += dt * (self.turn + (t * 0.21).cos() * 0.25 * (0.5 + a.mid));
        let (s, co) = self.yaw.sin_cos();
        let speed = 50.0 + 150.0 * a.bass;
        self.cam.0 = (self.cam.0 + s * speed * dt).rem_euclid(256.0);
        self.cam.1 = (self.cam.1 + co * speed * dt).rem_euclid(256.0);
        let bob = (t * 1.1).sin() * 0.12 + a.pulse * 0.12;
        let fogc = bright(mix(c.pal.at(self.fog_at), WHITE, 0.15), 200 + (a.pulse * 110.0) as u32);
        let (hor, fov) = (HF / 2.0, 170.0);
        self.floor.get(c.pal);
        self.ceil.get(c.pal);
        for y in 0..H {
            let dy = y as f32 + 0.5 - hor;
            let row = &mut fb.px[y * W..(y + 1) * W];
            if dy.abs() < 0.6 {
                row.fill(fogc);
                continue;
            }
            let (h, tex, off) = if dy > 0.0 { (1.0 + bob, &self.floor.tex, 0.0) } else { (1.0 - bob, &self.ceil.tex, 128.0) };
            let depth = 64.0 * h * fov / dy.abs();
            let k = k256(1.0 - (-depth / 700.0).exp());
            plane_row(row, tex, (self.cam.0 + off, self.cam.1), (s, co), depth, fov, |p| blend(p, fogc, k));
        }
    }
}

// ---------------------------------------------------------------- 6. MODE7

/// A rotating perspective floor under a gradient sky with a ridge on the horizon.
struct Mode7 {
    tex: PalTex,
    cam: (f32, f32),
    yaw: f32,
    dir: f32,
    hor: f32,
    ridge: [f32; 6],
    sun: Option<f32>,
}

impl Mode7 {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        Mode7 {
            tex: PalTex::new(pick_pattern(&mut r), seed),
            cam: (0.0, 0.0),
            yaw: r.f() * TAU,
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
            hor: r.range(66.0, 80.0),
            ridge: std::array::from_fn(|_| r.f() * TAU),
            sun: if r.below(3) > 0 { Some(r.f() * TAU) } else { None },
        }
    }
}

impl Effect for Mode7 {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, pal) = (c.a, c.dt, c.pal);
        self.yaw = (self.yaw + dt * (0.2 + 0.6 * a.mid) * self.dir).rem_euclid(TAU);
        let (s, co) = self.yaw.sin_cos();
        let speed = 40.0 + 100.0 * a.bass;
        self.cam.0 = (self.cam.0 + s * speed * dt).rem_euclid(256.0);
        self.cam.1 = (self.cam.1 + co * speed * dt).rem_euclid(256.0);
        let fov = 190.0;
        let hor = self.hor;
        let yh = hor as usize;
        // sky
        let top = scale(pal.at(0.65), 0.35);
        let low = bright(mix(pal.at(0.95), WHITE, 0.25), 220 + (a.pulse * 90.0) as u32);
        for y in 0..yh {
            fb.row(y as i32, blend(top, low, k256((y as f32 / hor).powf(1.4))));
        }
        if let Some(sa) = self.sun {
            let d = (sa - self.yaw + PI).rem_euclid(TAU) - PI;
            let sx = WF / 2.0 + d * fov;
            let sy = hor - 22.0;
            let sunc = mix(pal.hi, WHITE, 0.3);
            for y in (sy - 40.0).max(0.0) as i32..yh as i32 {
                for x in (sx - 40.0) as i32..(sx + 40.0) as i32 {
                    let dd = ((x as f32 - sx).powi(2) + (y as f32 - sy).powi(2)).sqrt();
                    if dd < 15.0 {
                        fb.put(x, y, sunc);
                    } else if dd < 40.0 && x >= 0 && x < WI {
                        let i = y as usize * W + x as usize;
                        fb.px[i] = add_sat(fb.px[i], shade(sunc, k256((1.0 - dd / 40.0).powi(2) * 0.5)));
                    }
                }
            }
        }
        // distant ridge, periodic in the view angle so it turns with the floor
        let rc = mix(scale(pal.at(0.3), 0.5), low, 0.3);
        for x in 0..W {
            let ang = self.yaw + (x as f32 - WF / 2.0) / fov;
            let h: f32 = (0..6).map(|k| ((k * 3 + 2) as f32 * ang + self.ridge[k]).sin() / (k + 1) as f32).sum();
            let ty = (hor - 3.0 - (h + 1.6) * 6.0) as i32;
            for y in ty.max(0)..yh as i32 {
                fb.px[y as usize * W + x] = if y == ty { mix(rc, pal.hi, 0.5) } else { rc };
            }
        }
        // floor
        let cam_h = 1.0 + 0.25 * a.pulse;
        let tex = self.tex.get(pal);
        for y in yh..H {
            let dy = y as f32 + 0.5 - hor;
            let depth = 64.0 * cam_h * fov / dy.max(0.5);
            let k = k256(1.0 - (-depth / 900.0).exp());
            plane_row(&mut fb.px[y * W..(y + 1) * W], tex, self.cam, (s, co), depth, fov, |p| blend(p, low, k));
        }
    }
}

// ---------------------------------------------------------------- 7. VOXEL

/// A heightmap landscape fly-over: each screen column marches a ray over the map
/// front to back and draws the parts that rise above what is already drawn.
struct Voxel {
    hmap: Vec<u8>,
    col: Vec<u32>,
    key: Option<u32>,
    kind: u32,
    pos: (f32, f32),
    yaw: f32,
    cam_h: f32,
    sky: Vec<u32>,
}

const WATER: f32 = 0.45;
/// World height per heightmap step.
const VOXEL_H: f32 = 0.22;

impl Voxel {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(3);
        let s = r.u32();
        let mut hmap = vec![0u8; TS * TS];
        for y in 0..TS {
            for x in 0..TS {
                let n = contrast(fbm(x as f32, y as f32, 4, 6, s), 2.2);
                let h = match kind {
                    0 => n.powf(1.5),
                    1 => (n * 7.0).floor() / 7.0 * 0.85 + n * 0.15,
                    _ => {
                        if n < WATER { WATER } else { (WATER + (n - WATER) * 1.5).min(1.0) }
                    }
                };
                hmap[y * TS + x] = (h * 255.0) as u8;
            }
        }
        Voxel {
            hmap,
            col: vec![0; TS * TS],
            key: None,
            kind,
            pos: (r.f() * 256.0, r.f() * 256.0),
            yaw: r.f() * TAU,
            cam_h: 120.0,
            sky: vec![0; H],
        }
    }

    fn recolour(&mut self, pal: &Palette) {
        let h = |x: usize, y: usize| self.hmap[(y & 255) * TS + (x & 255)] as f32 / 255.0;
        for y in 0..TS {
            for x in 0..TS {
                let v = h(x, y);
                let dh = h(x + 254, y) - h(x + 2, y) + h(x, y + 254) - h(x, y + 2);
                let light = (0.85 + dh * 5.0).clamp(0.35, 1.4);
                let base = if self.kind == 2 && v <= WATER + 0.004 {
                    scale(pal.at(0.6), 0.5 + 0.12 * hash(x as i32, y as i32, 3))
                } else if v > 0.8 {
                    mix(pal.at(0.5), WHITE, 0.6)
                } else {
                    pal.ramp(0.3 + v * 0.6)
                };
                self.col[y * TS + x] = bright(base, k256(light / 1.4) * 14 / 10);
            }
        }
        self.key = Some(pal_key(pal));
    }
}

impl Effect for Voxel {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t, pal) = (c.a, c.dt, c.t, c.pal);
        if self.key != Some(pal_key(pal)) {
            self.recolour(pal);
        }
        self.yaw += dt * ((t * 0.23).sin() * 0.35 + (a.mid - 0.5) * 0.3);
        let (s, co) = self.yaw.sin_cos();
        let speed = 30.0 + 60.0 * a.bass;
        self.pos.0 = (self.pos.0 + s * speed * dt).rem_euclid(256.0);
        self.pos.1 = (self.pos.1 + co * speed * dt).rem_euclid(256.0);
        let hk = VOXEL_H * (1.0 + 0.25 * a.pulse); // terrain jumps on the beat
        let ground = |z: f32| {
            let (x, y) = (self.pos.0 + s * z + 1024.0, self.pos.1 + co * z + 1024.0);
            self.hmap[((y as i32 & 255) << 8 | (x as i32 & 255)) as usize] as f32 * VOXEL_H
        };
        let ahead = [0.0, 12.0, 25.0, 40.0, 60.0].map(ground);
        let target = ahead.iter().fold(0.0f32, |m, &h| m.max(h)) + 24.0 + 14.0 * a.bass;
        self.cam_h += (target - self.cam_h) * (1.0 - (-dt * 3.0).exp());
        let hor = 62.0;
        let top = scale(pal.at(0.65), 0.4);
        let low = bright(mix(pal.at(0.9), WHITE, 0.3), 220 + (a.pulse * 80.0) as u32);
        for (y, p) in self.sky.iter_mut().enumerate() {
            *p = blend(top, low, k256((y as f32 / hor).powf(1.2)));
        }
        let (scr, zmax) = (120.0, 240.0);
        for col in 0..W / 2 {
            let f = (col as f32 + 0.5) / (W / 2) as f32 * 2.0 - 1.0;
            let (rx, ry) = (s + co * f * 0.8, co - s * f * 0.8);
            let mut ybuf = H as i32;
            let (mut z, mut dz) = (1.0f32, 0.4f32);
            while z < zmax && ybuf > 0 {
                let mx = (self.pos.0 + rx * z + 1024.0) as i32 & 255;
                let my = (self.pos.1 + ry * z + 1024.0) as i32 & 255;
                let i = (my << 8 | mx) as usize;
                let sy = ((self.cam_h - self.hmap[i] as f32 * hk) / z * scr + hor) as i32;
                if sy < ybuf {
                    let cc = blend(self.col[i], low, k256((z / zmax).powf(1.4)));
                    for y in sy.max(0)..ybuf {
                        let p = y as usize * W + col * 2;
                        fb.px[p] = cc;
                        fb.px[p + 1] = cc;
                    }
                    ybuf = sy.max(0);
                }
                z += dz;
                dz += 0.012;
            }
            for y in 0..ybuf.max(0) as usize {
                let p = y * W + col * 2;
                fb.px[p] = self.sky[y];
                fb.px[p + 1] = self.sky[y];
            }
        }
    }
}

// ---------------------------------------------------------------- 8. BUMPMAP

/// 2D bump mapping: a relief's slopes bend the lookup into a light spot table.
/// The light wanders and jumps to a new place on every beat.
struct Bump {
    nx: Vec<i8>,
    ny: Vec<i8>,
    /// Light intensity by offset from the light, in 2-pixel units, 256×256.
    light: Vec<u8>,
    lights: usize,
    /// Position and target per light.
    pos: [[f32; 4]; 2],
    rng: Rng,
    off: (f32, f32),
    lut: [[u32; 256]; 2],
}

impl Bump {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(4);
        let h = relief(kind, r.u32());
        let at = |x: usize, y: usize| h[(y & 255) * TS + (x & 255)];
        let mut nx = vec![0i8; TS * TS];
        let mut ny = vec![0i8; TS * TS];
        for y in 0..TS {
            for x in 0..TS {
                let gx = (at(x + 1, y) - at(x + 255, y)) * 220.0;
                let gy = (at(x, y + 1) - at(x, y + 255)) * 220.0;
                nx[y * TS + x] = gx.clamp(-100.0, 100.0) as i8;
                ny[y * TS + x] = gy.clamp(-100.0, 100.0) as i8;
            }
        }
        let mut light = vec![0u8; 256 * 256];
        for ty in 0..256 {
            for tx in 0..256 {
                let (dx, dy) = ((tx as f32 - 128.0) * 2.0, (ty as f32 - 128.0) * 2.0);
                let d = (dx * dx + dy * dy).sqrt() / 200.0;
                light[ty * 256 + tx] = ((1.0 - d).max(0.0).powf(1.6) * 255.0) as u8;
            }
        }
        let lights = 1 + r.below(2) as usize;
        let p0 = [WF * 0.3, HF * 0.5, WF * 0.3, HF * 0.5];
        let p1 = [WF * 0.7, HF * 0.5, WF * 0.7, HF * 0.5];
        Bump { nx, ny, light, lights, pos: [p0, p1], rng: r, off: (0.0, 0.0), lut: [[0; 256]; 2] }
    }
}

/// Height fields 0..1 for the bump map (tiling 256×256): 0 domes, 1 riveted
/// plates, 2 ripples, 3 cracked rock.
fn relief(kind: u32, seed: u32) -> Vec<f32> {
    let mut h = vec![0.0f32; TS * TS];
    let wrap = |d: f32| (d + 128.0).rem_euclid(256.0) - 128.0;
    for y in 0..TS {
        for x in 0..TS {
            let (fx, fy) = (x as f32, y as f32);
            h[y * TS + x] = match kind {
                0 => {
                    let (cx, cy) = ((x / 32) as i32, (y / 32) as i32);
                    let mut best = 0.0f32;
                    for j in -1..=1 {
                        for i in -1..=1 {
                            let (gx, gy) = ((cx + i) & 7, (cy + j) & 7);
                            let px = gx as f32 * 32.0 + 8.0 + hash(gx, gy, seed) * 16.0;
                            let py = gy as f32 * 32.0 + 8.0 + hash(gx, gy, seed + 1) * 16.0;
                            let rr = 10.0 + hash(gx, gy, seed + 2) * 10.0;
                            let (dx, dy) = (wrap(fx - px), wrap(fy - py));
                            let q = 1.0 - (dx * dx + dy * dy) / (rr * rr);
                            if q > 0.0 {
                                best = best.max(q.sqrt() * rr / 20.0);
                            }
                        }
                    }
                    best
                }
                1 => {
                    let (px, py) = (x % 64, y % 64);
                    let e = px.min(py).min(63 - px).min(63 - py) as f32;
                    let mut v = smoothstep(e / 5.0) * 0.6;
                    for (rx, ry) in [(8.0, 8.0), (55.0, 8.0), (8.0, 55.0), (55.0, 55.0)] {
                        let d2 = (px as f32 - rx).powi(2) + (py as f32 - ry).powi(2);
                        if d2 < 16.0 {
                            v += (1.0 - d2 / 16.0).sqrt() * 0.35;
                        }
                    }
                    let r = ((px as f32 - 31.5).powi(2) + (py as f32 - 31.5).powi(2)).sqrt();
                    if (r - 16.0).abs() < 2.5 {
                        v -= 0.2 * (1.0 - (r - 16.0).abs() / 2.5);
                    }
                    v + fbm(fx, fy, 16, 3, seed) * 0.15
                }
                2 => {
                    let mut v = 0.0;
                    for k in 0..3 {
                        let (cx, cy) = (hash(k, 0, seed) * 256.0, hash(k, 1, seed) * 256.0);
                        let (dx, dy) = (wrap(fx - cx), wrap(fy - cy));
                        v += ((dx * dx + dy * dy).sqrt() * 0.3).cos() * 0.22;
                    }
                    v + fbm(fx, fy, 8, 3, seed) * 0.3
                }
                _ => {
                    let n = fbm(fx, fy, 8, 6, seed);
                    n * 0.8 - (1.0 - (n - 0.5).abs() * 2.0).powi(10) * 0.3
                }
            };
        }
    }
    h
}

impl Effect for Bump {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t, pal) = (c.a, c.dt, c.t, c.pal);
        let ease = 1.0 - (-dt * 14.0).exp();
        for (k, p) in self.pos.iter_mut().enumerate().take(self.lights) {
            if a.beat {
                p[2] = self.rng.range(WF * 0.15, WF * 0.85);
                p[3] = self.rng.range(HF * 0.2, HF * 0.8);
            }
            // drift around the target, jump towards it quickly
            let ph = t * 0.9 + k as f32 * PI;
            let (tx, ty) = (p[2] + ph.sin() * 30.0, p[3] + (ph * 1.3).cos() * 20.0);
            p[0] += (tx - p[0]) * ease;
            p[1] += (ty - p[1]) * ease;
        }
        self.off.0 = (self.off.0 + dt * 9.0 * (1.0 + a.bass)).rem_euclid(256.0);
        self.off.1 = (self.off.1 + dt * 5.0).rem_euclid(256.0);
        let gain = 0.9 + 0.3 * a.bass + 0.35 * a.pulse;
        for i in 0..256 {
            let f = i as f32 / 255.0 * gain;
            if self.lights == 1 {
                self.lut[0][i] = pal.ramp(0.1 + f * 0.92);
            } else {
                for (j, hue) in [0.05f32, 0.5].into_iter().enumerate() {
                    let cc = scale(pal.at(hue), (0.06 + f * 1.3).min(1.0));
                    self.lut[j][i] = mix(cc, WHITE, (f - 0.8) * 2.5);
                }
            }
        }
        let (ox, oy) = (self.off.0 as usize, self.off.1 as usize);
        let lp: [(i32, i32); 2] = std::array::from_fn(|k| (self.pos[k][0] as i32, self.pos[k][1] as i32));
        for y in 0..H {
            let trow = ((y + oy) & 255) * TS;
            let out = &mut fb.px[y * W..(y + 1) * W];
            for (x, o) in out.iter_mut().enumerate() {
                let ti = trow + ((x + ox) & 255);
                let (nx, ny) = (self.nx[ti] as i32, self.ny[ti] as i32);
                let mut col = 0xFF00_0000;
                for (k, &(lx, ly)) in lp.iter().enumerate().take(self.lights) {
                    let hx = ((x as i32 - lx + nx) >> 1) + 128;
                    let hy = ((y as i32 - ly + ny) >> 1) + 128;
                    let i = if (hx as u32) < 256 && (hy as u32) < 256 {
                        self.light[(hy << 8 | hx) as usize]
                    } else {
                        0
                    };
                    col = add_sat(col, self.lut[k][i as usize]);
                }
                *o = col;
            }
        }
    }
}

// ---------------------------------------------------------------- 9. LENS

/// One or two glass lenses bouncing over a landscape picture, magnifying it.
struct Lens {
    pic: PalTex,
    /// x, y, vx, vy, radius
    balls: Vec<[f32; 5]>,
    mag: f32,
    pan: f32,
    utab: Vec<i32>,
}

const LENS_S: f32 = 2.0 / 3.0;

impl Lens {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let n = 1 + r.below(2) as usize;
        let balls = (0..n)
            .map(|i| {
                let rad = if i == 0 { 50.0 } else { 36.0 };
                let vx = r.range(60.0, 110.0) * if r.below(2) == 0 { 1.0 } else { -1.0 };
                [r.range(rad, WF - rad), r.range(rad, HF - rad), vx, 0.0, rad]
            })
            .collect();
        Lens { pic: PalTex::new(tex_picture, seed * 2), balls, mag: r.range(0.45, 0.62), pan: 0.0, utab: vec![0; W] }
    }
}

impl Effect for Lens {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t) = (c.a, c.dt, c.t);
        self.pan = (self.pan + dt * (8.0 + 12.0 * a.mid)).rem_euclid(256.0);
        let pv = 56.0 + (t * 0.11).sin() * 52.0;
        for (x, u) in self.utab.iter_mut().enumerate() {
            *u = (self.pan + x as f32 * LENS_S) as i32 & 255;
        }
        let tex = self.pic.get(c.pal);
        for y in 0..H {
            let vrow = (((pv + y as f32 * LENS_S) as i32 & 255) << 8) as usize;
            for (o, &u) in fb.px[y * W..(y + 1) * W].iter_mut().zip(&self.utab) {
                *o = shade(tex.px[vrow | u as usize], 215);
            }
        }
        let rim = mix(c.pal.hi, WHITE, 0.3);
        for b in self.balls.iter_mut() {
            let r = b[4] * (1.0 + 0.12 * a.pulse + 0.08 * a.bass);
            b[0] += b[2] * dt;
            if b[0] < r {
                b[0] = r;
                b[2] = b[2].abs();
            } else if b[0] > WF - r {
                b[0] = WF - r;
                b[2] = -b[2].abs();
            }
            b[3] += 420.0 * dt;
            b[1] += b[3] * dt;
            if b[1] > HF - r {
                b[1] = HF - r;
                b[3] = -(300.0 + 80.0 * a.bass);
            }
            let (bx, by) = (b[0], b[1]);
            for py in ((by - r) as i32).max(0)..((by + r) as i32 + 1).min(HI) {
                for px in ((bx - r) as i32).max(0)..((bx + r) as i32 + 1).min(WI) {
                    let (dx, dy) = (px as f32 - bx, py as f32 - by);
                    let q2 = (dx * dx + dy * dy) / (r * r);
                    if q2 >= 1.0 {
                        continue;
                    }
                    // sample closer to the centre the further in: a glass bead
                    let f = 1.0 - self.mag * (1.0 - q2).sqrt();
                    let (sx, sy) = (bx + dx * f, by + dy * f);
                    let mut col =
                        bright(tex.at((self.pan + sx * LENS_S) as i32, (pv + sy * LENS_S) as i32), 280);
                    if q2 > 0.8 {
                        col = blend(col, rim, k256((q2 - 0.8) / 0.2 * 0.75));
                    }
                    let (hx, hy) = (dx / r + 0.38, dy / r + 0.42);
                    let hs = hx * hx + hy * hy;
                    if hs < 0.03 {
                        col = add_sat(col, shade(WHITE, k256((1.0 - hs / 0.03) * 0.9)));
                    }
                    fb.px[py as usize * W + px as usize] = col;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- 10. OUTRUN

/// A neon wireframe grid floor rolling towards a hazy horizon, mountain ridges
/// and a glowing gradient sun.
struct Outrun {
    hor: f32,
    scroll: f32,
    sun_x: f32,
    sun_r: f32,
    jagged: bool,
    reflect: bool,
    grid_hue: f32,
    ph: [f32; 8],
    stars: Vec<(i32, i32, f32)>,
}

impl Outrun {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        Outrun {
            hor: r.range(112.0, 126.0),
            scroll: 0.0,
            sun_x: WF / 2.0 + r.range(-70.0, 70.0),
            sun_r: r.range(30.0, 46.0),
            jagged: r.below(2) == 0,
            reflect: r.below(2) == 0,
            grid_hue: r.f(),
            ph: std::array::from_fn(|_| r.f()),
            stars: (0..70).map(|_| (r.below(W as u32) as i32, r.below(90) as i32, r.f())).collect(),
        }
    }

    fn ridge(&self, x: f32, layer: usize) -> f32 {
        let mut h = 0.0;
        for k in 0..4 {
            let f = (k + 1) as f32 * 0.004 * (1.0 + layer as f32 * 0.7);
            let p = x * f + self.ph[layer * 4 + k];
            h += if self.jagged { tri(p) } else { 0.5 + 0.5 * (p * TAU).sin() } / (k + 1) as f32;
        }
        h / 2.08
    }
}

/// A line with a 1-pixel additive halo, for neon wireframes.
fn glow_line(fb: &mut Fb, (x0, y0): (f32, f32), (x1, y1): (f32, f32), c: u32) {
    let halo = shade(c, 80);
    let (dx, dy) = (x1 - x0, y1 - y0);
    let n = dx.abs().max(dy.abs()).ceil().min(1500.0) as i32;
    if n == 0 {
        return;
    }
    let (sx, sy) = (dx / n as f32, dy / n as f32);
    let (hx, hy) = if dy.abs() > dx.abs() { (1, 0) } else { (0, 1) };
    for i in 0..=n {
        let (x, y) = ((x0 + sx * i as f32).floor() as i32, (y0 + sy * i as f32).floor() as i32);
        if x < -1 || x > WI || y < -1 || y > HI {
            continue;
        }
        for (px, py, cc) in [(x, y, c), (x - hx, y - hy, halo), (x + hx, y + hy, halo)] {
            if px >= 0 && py >= 0 && px < WI && py < HI {
                let p = &mut fb.px[py as usize * W + px as usize];
                *p = add_sat(*p, cc);
            }
        }
    }
}

impl Effect for Outrun {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t, pal) = (c.a, c.dt, c.t, c.pal);
        self.scroll = fract(self.scroll + dt * (1.5 + 5.0 * a.bass));
        let sway = (t * 0.37).sin() * 1.2 + (t * 0.13).sin() * 0.8;
        let hor = self.hor;
        let yh = hor as usize;
        // sky and stars
        let top = scale(pal.at(0.7), 0.22);
        let low = bright(pal.at(0.95), 230 + (a.pulse * 60.0) as u32);
        for y in 0..yh {
            fb.row(y as i32, blend(top, low, k256((y as f32 / hor).powf(1.6))));
        }
        for (i, &(x, y, b)) in self.stars.iter().enumerate() {
            let tw = 0.5 + 0.5 * (c.time * (3.0 + b * 5.0) + i as f32).sin() * (0.4 + a.treble);
            fb.plot_add(x, y, shade(WHITE, k256(tw * (1.0 - y as f32 / 100.0))));
        }
        // sun: vertical gradient disc with a halo
        let (sx, sy, sr) = (self.sun_x - sway * 6.0, hor - self.sun_r * 0.55, self.sun_r);
        let (st, sb) = (mix(pal.hi, WHITE, 0.4), bright(pal.at(0.02), 280));
        let gr = sr * 2.4;
        let glow = 0.45 + 0.4 * a.pulse;
        for y in ((sy - gr) as i32).max(0)..yh as i32 {
            let fy = y as f32 - sy;
            let sc = blend(st, sb, k256((fy + sr) / (2.0 * sr)));
            for x in ((sx - gr) as i32).max(0)..((sx + gr) as i32).min(WI) {
                let fx = x as f32 - sx;
                let d = (fx * fx + fy * fy).sqrt();
                let p = &mut fb.px[y as usize * W + x as usize];
                if d < sr {
                    *p = sc;
                } else if d < gr {
                    *p = add_sat(*p, shade(sb, k256((1.0 - (d - sr) / (gr - sr)).powi(2) * glow)));
                }
            }
        }
        // two mountain layers, the near one with a neon edge
        let edge = bright(pal.at(self.grid_hue + 0.3), 300);
        for (layer, (amp, k, sp)) in [(30.0, 0.45, 3.0), (16.0, 0.2, 9.0)].into_iter().enumerate() {
            let mc = blend(scale(pal.at(0.75), 0.25), low, k256(k));
            for x in 0..W {
                let h = self.ridge(x as f32 + sway * sp + t * sp * 0.5, layer);
                let ty = (hor - 3.0 - (8.0 + amp * h) * (1.0 + layer as f32 * 0.0)) as i32;
                for y in ty.max(0)..yh as i32 {
                    fb.px[y as usize * W + x] = if layer == 1 && y == ty { edge } else { mc };
                }
            }
        }
        // floor
        let (far, near) = (scale(pal.at(0.9), 0.45), scale(pal.at(0.75), 0.12));
        for y in yh..H {
            let f = (y as f32 - hor) / (HF - hor);
            let base = blend(near, far, k256((1.0 - f).powi(4)));
            let row = &mut fb.px[y * W..(y + 1) * W];
            row.fill(base);
            if self.reflect {
                for x in ((sx - sr) as i32).max(0)..((sx + sr) as i32).min(WI) {
                    let k = (1.0 - (x as f32 - sx).abs() / sr) * (1.0 - f) * 0.5;
                    row[x as usize] = add_sat(base, shade(sb, k256(k)));
                }
            }
        }
        let grid = bright(mix(pal.at(self.grid_hue), pal.hi, 0.3), 240 + (a.pulse * 220.0) as u32);
        let span = HF - hor;
        let cam_x = sway * 0.8;
        for k in -14..=14 {
            let xw = (k as f32 - fract(cam_x)) * 0.5;
            glow_line(fb, (WF / 2.0, hor + 1.0), (WF / 2.0 + xw * span, HF), grid);
        }
        let fz = span * 0.27;
        let mut hline = |z: f32, cc: u32| {
            let y = (hor + fz / z) as i32;
            if y > yh as i32 && y < HI {
                for (yy, k) in [(y, 256), (y - 1, 80), (y + 1, 80)] {
                    if yy > yh as i32 && yy < HI {
                        let halo = shade(cc, k);
                        for p in &mut fb.px[yy as usize * W..(yy as usize + 1) * W] {
                            *p = add_sat(*p, halo);
                        }
                    }
                }
            }
        };
        for n in 1..32 {
            let z = n as f32 - self.scroll;
            hline(z, shade(grid, k256(1.1 - z / 32.0)));
        }
        // a bright wave rolls in from the horizon on every beat
        if a.pulse > 0.05 {
            hline(0.3 + 25.0 * a.pulse * a.pulse, bright(pal.hi, k256(a.pulse) * 3 / 2));
        }
        // haze on the horizon
        for y in yh..(yh + 14).min(H) {
            let k = k256((1.0 - (y - yh) as f32 / 14.0) * 0.8);
            for p in &mut fb.px[y * W..(y + 1) * W] {
                *p = blend(*p, low, k);
            }
        }
    }
}

// ---------------------------------------------------------------- 11. PLANET

/// What the planet lookup table holds for one pixel near the planet.
#[derive(Clone, Copy, Default)]
struct PCell {
    /// Longitude, 8.8 fixed point texels.
    u: u16,
    /// Latitude texel.
    v: u8,
    /// Sunlight 0..255.
    lit: u8,
    /// Atmosphere glow 0..255.
    glow: u8,
    inside: bool,
}

const PR: usize = 64;
const PHALO: usize = 16;
const PS: usize = 2 * (PR + PHALO);

/// A textured sphere turning via a lookup table (view direction → latitude and
/// longitude), with an atmosphere, stars, and a ring or a moon.
struct Planet {
    tex: PalTex,
    lut: Vec<PCell>,
    rot: f32,
    dir: f32,
    stars: Vec<[f32; 3]>,
    ring: Option<Vec<f32>>,
    moon: f32,
}

impl Planet {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let kind = r.below(3);
        let tilt = [r.range(-0.45, 0.45), 0.0, r.range(-0.4, 0.4)];
        let l = norm([-0.55, -0.45, 0.7]);
        let rf = PR as f32;
        let mut lut = vec![PCell::default(); PS * PS];
        for j in 0..PS {
            for i in 0..PS {
                let (dx, dy) = (i as f32 - PS as f32 / 2.0 + 0.5, j as f32 - PS as f32 / 2.0 + 0.5);
                let d = (dx * dx + dy * dy).sqrt();
                let cell = &mut lut[j * PS + i];
                if d < rf {
                    let n = [dx / rf, dy / rf, (1.0 - (d / rf).powi(2)).max(0.0).sqrt()];
                    let p = rot3(n, tilt);
                    let lat = p[1].clamp(-1.0, 1.0).asin();
                    let lon = p[0].atan2(p[2]);
                    let ln = n[0] * l[0] + n[1] * l[1] + n[2] * l[2];
                    cell.inside = true;
                    cell.u = ((lon / TAU + 0.5) * 65535.0) as u16;
                    cell.v = ((lat / PI + 0.5) * 255.0) as u8;
                    cell.lit = ((smoothstep(ln * 2.5 + 0.15) * 0.94 + 0.06) * 255.0) as u8;
                    let rim = (1.0 - n[2]).powi(3) * (0.35 + 0.65 * (ln + 0.3).clamp(0.0, 1.0));
                    cell.glow = (rim * 200.0) as u8;
                } else if d < rf + PHALO as f32 {
                    let side = ((-dx * 0.55 - dy * 0.45) / d * 0.5 + 0.6).clamp(0.0, 1.0);
                    cell.glow = ((1.0 - (d - rf) / PHALO as f32).powi(2) * side * 230.0) as u8;
                }
            }
        }
        let ring = (kind == 1 || r.below(3) == 0).then(|| {
            (0..=12).map(|i| if hash(i, 0, seed) < 0.2 { 0.1 } else { 0.4 + hash(i, 1, seed) * 0.6 }).collect()
        });
        Planet {
            tex: PalTex::new(tex_planet, kind + 3 * (seed % 1000)),
            lut,
            rot: 0.0,
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
            stars: (0..180).map(|_| [r.f() * WF, r.f() * HF, r.range(0.2, 1.0)]).collect(),
            ring,
            moon: r.f() * TAU,
        }
    }

    /// Ring pixels behind (`front == false`) or in front of the planet.
    fn draw_ring(&self, fb: &mut Fb, c: &Ctx, (cx, cy): (f32, f32), front: bool) {
        let Some(bands) = &self.ring else { return };
        let rf = PR as f32;
        let (ratio, (sg, cg)) = (0.28, 0.12f32.sin_cos());
        let (inner, outer) = (1.35, 2.15);
        let ex = outer * rf + 2.0;
        let ey = ex * ratio + 8.0;
        for y in ((cy - ey) as i32).max(0)..((cy + ey) as i32).min(HI) {
            for x in ((cx - ex) as i32).max(0)..((cx + ex) as i32).min(WI) {
                let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                let (u, v) = (dx * cg + dy * sg, (-dx * sg + dy * cg) / ratio);
                if (v > 0.0) != front {
                    continue;
                }
                let rr = (u * u + v * v).sqrt() / rf;
                if rr < inner || rr >= outer {
                    continue;
                }
                let f = (rr - inner) / (outer - inner) * 11.999;
                let b = lerp(bands[f as usize], bands[f as usize + 1], fract(f));
                let col = bright(c.pal.at(0.1 + rr * 0.15), k256(b) + (c.a.pulse * 60.0) as u32);
                let p = &mut fb.px[y as usize * W + x as usize];
                *p = blend(*p, col, 200);
            }
        }
    }

    fn draw_moon(&self, fb: &mut Fb, c: &Ctx, (cx, cy): (f32, f32), front: bool) {
        if self.ring.is_some() || (self.moon.sin() > 0.0) != front {
            return;
        }
        let rf = PR as f32;
        let (mx, my) = (cx + self.moon.cos() * rf * 1.9, cy + self.moon.sin() * rf * 0.5);
        let (r, g, b) = unpack(mix(c.pal.at(0.4), WHITE, 0.3));
        fb.ball(mx, my, 11.0 * (1.0 + 0.2 * self.moon.sin()), (r, g, b));
    }
}

impl Effect for Planet {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, t, pal) = (c.a, c.dt, c.t, c.pal);
        self.rot = (self.rot + dt * (8.0 + 30.0 * a.mid) * self.dir).rem_euclid(256.0);
        self.moon = (self.moon + dt * (0.6 + 0.6 * a.mid)).rem_euclid(TAU);
        fb.clear(pal.bg);
        let star = mix(pal.hi, WHITE, 0.6);
        for (i, s) in self.stars.iter_mut().enumerate() {
            s[0] -= dt * (4.0 + 30.0 * a.bass) * s[2];
            if s[0] < 0.0 {
                s[0] += WF;
            }
            let tw = 0.6 + 0.4 * (c.time * 6.0 + i as f32).sin() * a.treble;
            fb.put(s[0] as i32, s[1] as i32, shade(star, k256(s[2] * tw)));
        }
        let centre = (WF / 2.0 + (t * 0.3).sin() * 40.0, HF / 2.0);
        self.draw_ring(fb, c, centre, false);
        self.draw_moon(fb, c, centre, false);
        let atmo = mix(pal.at(0.55), WHITE, 0.3);
        let (lk, gk) = (1.0 + 0.3 * a.pulse, 0.7 + 0.7 * a.pulse);
        let rot = (self.rot * 256.0) as i32;
        let (x0, y0) = (centre.0 as i32 - PS as i32 / 2, centre.1 as i32 - PS as i32 / 2);
        let tex = self.tex.get(pal);
        for j in 0..PS {
            let y = y0 + j as i32;
            if y < 0 || y >= HI {
                continue;
            }
            for i in 0..PS {
                let x = x0 + i as i32;
                if x < 0 || x >= WI {
                    continue;
                }
                let cell = self.lut[j * PS + i];
                let p = &mut fb.px[y as usize * W + x as usize];
                let g = shade(atmo, (cell.glow as f32 * gk) as u32);
                if cell.inside {
                    let tc = tex.at((cell.u as i32 + rot) >> 8, cell.v as i32);
                    *p = add_sat(bright(tc, (cell.lit as f32 * lk) as u32), g);
                } else if cell.glow > 0 {
                    *p = add_sat(*p, g);
                }
            }
        }
        self.draw_ring(fb, c, centre, true);
        self.draw_moon(fb, c, centre, true);
    }
}

// ---------------------------------------------------------------- 12. FLAG

/// A picture waving like cloth on a pole, shaded by the slope of the wave.
struct Flag {
    tex: PalTex,
    left: bool,
    kx: f32,
    phase: f32,
    sin: Vec<f32>,
}

const FX0: f32 = 80.0;
const FW: f32 = 230.0;
const FY0: f32 = 58.0;
const FH: f32 = 104.0;

impl Flag {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let tex = if r.below(2) == 0 {
            PalTex::new(tex_picture, seed * 2 + 1)
        } else {
            PalTex::new(pick_pattern(&mut r), seed)
        };
        Flag {
            tex,
            left: r.below(2) == 0,
            kx: r.range(1.3, 2.1),
            phase: 0.0,
            sin: (0..1024).map(|i| (i as f32 / 1024.0 * TAU).sin()).collect(),
        }
    }
}

impl Effect for Flag {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt, pal) = (c.a, c.dt, c.pal);
        self.phase = (self.phase + dt * (2.5 + 3.5 * a.mid)).rem_euclid(TAU);
        let x0 = if self.left { FX0 + 10.0 } else { FX0 + 8.0 };
        // sky behind
        let top = scale(pal.at(0.6), 0.5);
        let low = mix(pal.at(0.85), WHITE, 0.3);
        for y in 0..H {
            fb.row(y as i32, blend(top, low, k256(y as f32 / HF)));
        }
        let amp = 5.0 + 7.0 * a.bass + 6.0 * a.pulse;
        let ripple = 1.5 * a.treble;
        let to_i = 1024.0 / TAU;
        let tex = self.tex.get(pal);
        let pole = if self.left { x0 } else { x0 + FW };
        let ymin = (FY0 - amp - 4.0).max(0.0) as usize;
        let ymax = ((FY0 + FH + amp + 4.0) as usize).min(H);
        for y in ymin..ymax {
            let fy = (y as f32 - FY0) / FH;
            for x in (x0 - 12.0) as usize..((x0 + FW + 12.0) as usize).min(W) {
                let f = (if self.left { x as f32 - pole } else { pole - x as f32 } / FW).clamp(0.0, 1.0);
                let ph = TAU * (self.kx * f + 0.35 * fy) - self.phase;
                let i = (ph * to_i) as i32;
                let (s, co) = (self.sin[(i & 1023) as usize], self.sin[((i + 256) & 1023) as usize]);
                let s2 = self.sin[((i * 3 + 100) & 1023) as usize];
                let am = amp * f.powf(0.8);
                let dy = am * s + ripple * f * s2;
                let dx = am * 0.2 * co;
                let u = (x as f32 - x0 - dx) / FW * 256.0;
                let v = (y as f32 - FY0 - dy) / FH * 256.0;
                if !(0.0..256.0).contains(&u) || !(0.0..256.0).contains(&v) {
                    continue;
                }
                let slope = am * co * TAU * self.kx / FW * if self.left { 1.0 } else { -1.0 };
                let k = (1.0 + slope * 1.6).clamp(0.45, 1.5);
                fb.px[y * W + x] = bright(tex.at(u as i32, v as i32), (k * 256.0) as u32);
            }
        }
        // the pole and its knob
        let px = if self.left { pole - 3.0 } else { pole + 3.0 };
        let metal = mix(pal.at(0.1), WHITE, 0.25);
        for y in (FY0 - 14.0) as i32..HI {
            for dx in -3..=3 {
                let k = 1.0 - (dx as f32 + 1.0).abs() / 4.5;
                fb.put(px as i32 + dx, y, bright(metal, k256(0.3 + k * 0.7) + 60));
            }
        }
        let (r, g, b) = unpack(mix(pal.hi, WHITE, 0.2));
        fb.ball(px, FY0 - 17.0, 5.5, (r, g, b));
    }
}

// ---------------------------------------------------------------- 13. DROSTE

/// An endless log-polar spiral zoom into a framed picture that contains itself.
struct Droste {
    pic: PalTex,
    /// ln(r) and angle for every screen pixel around the centre.
    rho: Vec<f32>,
    theta: Vec<f32>,
    ln_s: f32,
    k: f32,
    /// e^a for a in [-ln s, 0), 1024 steps.
    exp_t: Vec<f32>,
    /// Unit "square circle" direction per angle (4096 steps).
    dir_t: Vec<(f32, f32)>,
    zoom: f32,
    spin: f32,
    dir: f32,
}

impl Droste {
    fn new(seed: u32) -> Self {
        let mut r = Rng::new(seed);
        let s: f32 = [2.2, 2.8, 3.5][r.below(3) as usize];
        let arms = 1.0 + r.below(2) as f32;
        let ln_s = s.ln();
        let mut rho = vec![0.0; W * H];
        let mut theta = vec![0.0; W * H];
        for y in 0..H {
            for x in 0..W {
                let (dx, dy) = (x as f32 - WF / 2.0 + 0.5, y as f32 - HF / 2.0 + 0.5);
                rho[y * W + x] = (dx.abs().max(dy.abs()).max(0.5) / 100.0).ln();
                theta[y * W + x] = dy.atan2(dx);
            }
        }
        Droste {
            pic: PalTex::new(tex_picture, seed * 2 + 1),
            rho,
            theta,
            ln_s,
            k: arms * ln_s / TAU,
            exp_t: (0..1024).map(|i| ((i as f32 + 0.5) / 1024.0 * ln_s - ln_s).exp()).collect(),
            dir_t: (0..4096)
                .map(|i| {
                    let (s, c) = (i as f32 / 4096.0 * TAU).sin_cos();
                    let m = s.abs().max(c.abs());
                    (c / m, s / m)
                })
                .collect(),
            zoom: 0.0,
            spin: 0.0,
            dir: if r.below(2) == 0 { 1.0 } else { -1.0 },
        }
    }
}

impl Effect for Droste {
    fn draw(&mut self, fb: &mut Fb, c: &Ctx) {
        let (a, dt) = (c.a, c.dt);
        self.zoom += dt * (0.25 + 0.7 * a.bass);
        self.spin = (self.spin + dt * (0.15 + 0.5 * a.mid) * self.dir).rem_euclid(TAU);
        let (ln_s, k, zoom, spin) = (self.ln_s, self.k, self.zoom, self.spin);
        let (to_e, to_d) = (1024.0 / ln_s, 4096.0 / TAU);
        let lit = 240 + (a.pulse * 110.0) as u32;
        let tex = self.pic.get(c.pal);
        for (i, o) in fb.px.iter_mut().enumerate() {
            let (rho, th) = (self.rho[i] - zoom, self.theta[i] + spin);
            // turn the log-polar plane so one revolution steps one picture level in
            let (u, v) = (rho + k * th, th - k * rho);
            let e = self.exp_t[((u.rem_euclid(ln_s) * to_e) as usize).min(1023)];
            let (dx, dy) = self.dir_t[((v * to_d).floor() as i32 & 4095) as usize];
            let (tx, ty) = (128.0 + e * dx * 127.0, 128.0 + e * dy * 127.0);
            *o = bright(tex.at(tx as i32, ty as i32), lit);
        }
    }
}
