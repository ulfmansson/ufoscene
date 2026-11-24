//! A small QR code encoder: byte mode, error correction level M, versions 1–6 (up to 106
//! bytes), mask chosen by the standard penalty rules. Enough for a short URL on screen.
//! Follows ISO/IEC 18004; the structure mirrors Project Nayuki's well-known reference design.

/// A QR code: `size`×`size` modules, `true` = dark.
pub struct Qr {
    pub size: usize,
    modules: Vec<bool>,
}

impl Qr {
    pub fn get(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }
}

/// Error-correction codewords per block and number of blocks, level M, versions 1..=6.
const EC_PER_BLOCK: [usize; 7] = [0, 10, 16, 26, 18, 24, 16];
const BLOCKS: [usize; 7] = [0, 1, 1, 1, 2, 2, 4];
/// All codewords (data + error correction) per version.
const RAW: [usize; 7] = [0, 26, 44, 70, 100, 134, 172];
/// Centre of the second alignment pattern (the first is always 6).
const ALIGN: [usize; 7] = [0, 0, 18, 22, 26, 30, 34];

/// Encode `data` (for example a URL), or None if it is longer than 106 bytes.
pub fn encode(data: &[u8]) -> Option<Qr> {
    let version = (1..=6).find(|&v| data_capacity(v) * 8 >= 4 + 8 + data.len() * 8)?;
    let cap = data_capacity(version);

    // bit stream: byte mode, length, data, terminator, padding
    let mut bits: Vec<bool> = Vec::new();
    let push = |val: usize, n: usize, bits: &mut Vec<bool>| {
        for i in (0..n).rev() {
            bits.push((val >> i) & 1 == 1);
        }
    };
    push(0b0100, 4, &mut bits);
    push(data.len(), 8, &mut bits);
    for &b in data {
        push(b as usize, 8, &mut bits);
    }
    let room = cap * 8 - bits.len();
    push(0, room.min(4), &mut bits);
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut codewords: Vec<u8> = bits.chunks(8).map(|c| c.iter().fold(0u8, |a, &b| (a << 1) | b as u8)).collect();
    for pad in [0xEC, 0x11].iter().cycle() {
        if codewords.len() >= cap {
            break;
        }
        codewords.push(*pad);
    }

    let all = add_ecc_and_interleave(&codewords, version);
    let size = version * 4 + 17;
    let mut q = Grid { size, m: vec![false; size * size], func: vec![false; size * size] };
    q.function_patterns(version);
    q.draw_codewords(&all);

    // choose the mask with the lowest penalty
    let mut best = (u32::MAX, 0);
    for mask in 0..8 {
        q.apply_mask(mask);
        q.format_bits(mask);
        let p = q.penalty();
        if p < best.0 {
            best = (p, mask);
        }
        q.apply_mask(mask); // undo (XOR)
    }
    q.apply_mask(best.1);
    q.format_bits(best.1);
    Some(Qr { size, modules: q.m })
}

fn data_capacity(version: usize) -> usize {
    RAW[version] - EC_PER_BLOCK[version] * BLOCKS[version]
}

fn add_ecc_and_interleave(data: &[u8], version: usize) -> Vec<u8> {
    let (blocks, ec, raw) = (BLOCKS[version], EC_PER_BLOCK[version], RAW[version]);
    let short = blocks - raw % blocks;
    let short_len = raw / blocks;
    let div = rs_divisor(ec);
    let mut out_blocks: Vec<Vec<u8>> = Vec::new();
    let mut k = 0;
    for i in 0..blocks {
        let dlen = short_len - ec + if i < short { 0 } else { 1 };
        let d = &data[k..k + dlen];
        k += dlen;
        let mut b = d.to_vec();
        if i < short {
            b.push(0); // placeholder so all blocks interleave column by column
        }
        b.extend(rs_remainder(d, &div));
        out_blocks.push(b);
    }
    let mut out = Vec::with_capacity(raw);
    for col in 0..out_blocks[0].len() {
        for (i, b) in out_blocks.iter().enumerate() {
            if col != short_len - ec || i >= short {
                out.push(b[col]);
            }
        }
    }
    out
}

fn gf_mul(x: u8, y: u8) -> u8 {
    let mut z: u16 = 0;
    for i in (0..8).rev() {
        z = (z << 1) ^ ((z >> 7) * 0x11D);
        z ^= ((y as u16 >> i) & 1) * x as u16;
    }
    z as u8
}

fn rs_divisor(degree: usize) -> Vec<u8> {
    let mut r = vec![0u8; degree];
    r[degree - 1] = 1;
    let mut root = 1u8;
    for _ in 0..degree {
        for j in 0..degree {
            r[j] = gf_mul(r[j], root);
            if j + 1 < degree {
                r[j] ^= r[j + 1];
            }
        }
        root = gf_mul(root, 0x02);
    }
    r
}

fn rs_remainder(data: &[u8], div: &[u8]) -> Vec<u8> {
    let mut r = vec![0u8; div.len()];
    for &b in data {
        let f = b ^ r[0];
        r.remove(0);
        r.push(0);
        for (x, &y) in r.iter_mut().zip(div) {
            *x ^= gf_mul(y, f);
        }
    }
    r
}

struct Grid {
    size: usize,
    m: Vec<bool>,
    func: Vec<bool>,
}

impl Grid {
    fn set_func(&mut self, x: usize, y: usize, dark: bool) {
        let i = y * self.size + x;
        self.m[i] = dark;
        self.func[i] = true;
    }

    fn function_patterns(&mut self, version: usize) {
        let s = self.size;
        for i in 0..s {
            self.set_func(6, i, i % 2 == 0);
            self.set_func(i, 6, i % 2 == 0);
        }
        for (cx, cy) in [(3, 3), (s - 4, 3), (3, s - 4)] {
            self.finder(cx as i32, cy as i32);
        }
        if version >= 2 {
            let a = ALIGN[version];
            self.alignment(a, a);
        }
        self.format_bits(0); // reserve the format areas (real bits are drawn later)
    }

    fn finder(&mut self, cx: i32, cy: i32) {
        for dy in -4..=4 {
            for dx in -4..=4 {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= self.size as i32 || y >= self.size as i32 {
                    continue;
                }
                let d = dx.abs().max(dy.abs());
                self.set_func(x as usize, y as usize, d != 2 && d != 4);
            }
        }
    }

    fn alignment(&mut self, cx: usize, cy: usize) {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let d = dx.abs().max(dy.abs());
                self.set_func((cx as i32 + dx) as usize, (cy as i32 + dy) as usize, d != 1);
            }
        }
    }

    fn format_bits(&mut self, mask: usize) {
        let data = mask; // level M = 00
        let mut rem = data;
        for _ in 0..10 {
            rem = (rem << 1) ^ ((rem >> 9) * 0x537);
        }
        let bits = ((data << 10) | rem) ^ 0x5412;
        let bit = |i: usize| (bits >> i) & 1 == 1;
        let s = self.size;
        for i in 0..=5 {
            self.set_func(8, i, bit(i));
        }
        self.set_func(8, 7, bit(6));
        self.set_func(8, 8, bit(7));
        self.set_func(7, 8, bit(8));
        for i in 9..15 {
            self.set_func(14 - i, 8, bit(i));
        }
        for i in 0..8 {
            self.set_func(s - 1 - i, 8, bit(i));
        }
        for i in 8..15 {
            self.set_func(8, s - 15 + i, bit(i));
        }
        self.set_func(8, s - 8, true); // the dark module
    }

    fn draw_codewords(&mut self, data: &[u8]) {
        let s = self.size;
        let mut i = 0usize;
        let mut right = s as i32 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vert in 0..s {
                for j in 0..2 {
                    let x = (right - j) as usize;
                    let upward = (right + 1) & 2 == 0;
                    let y = if upward { s - 1 - vert } else { vert };
                    let k = y * s + x;
                    if !self.func[k] && i < data.len() * 8 {
                        self.m[k] = (data[i >> 3] >> (7 - (i & 7))) & 1 == 1;
                        i += 1;
                    }
                }
            }
            right -= 2;
        }
    }

    fn apply_mask(&mut self, mask: usize) {
        let s = self.size;
        for y in 0..s {
            for x in 0..s {
                let inv = match mask {
                    0 => (x + y) % 2 == 0,
                    1 => y % 2 == 0,
                    2 => x % 3 == 0,
                    3 => (x + y) % 3 == 0,
                    4 => (x / 3 + y / 2) % 2 == 0,
                    5 => x * y % 2 + x * y % 3 == 0,
                    6 => (x * y % 2 + x * y % 3) % 2 == 0,
                    _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
                };
                let k = y * s + x;
                if inv && !self.func[k] {
                    self.m[k] = !self.m[k];
                }
            }
        }
    }

    fn penalty(&self) -> u32 {
        let s = self.size;
        let at = |x: usize, y: usize| self.m[y * s + x];
        let mut p = 0u32;
        // runs of 5+ in rows and columns, and finder-like patterns
        let finder = [true, false, true, true, true, false, true];
        for horizontal in [true, false] {
            for a in 0..s {
                let line: Vec<bool> = (0..s).map(|b| if horizontal { at(b, a) } else { at(a, b) }).collect();
                let mut run = 1;
                for b in 1..=s {
                    if b < s && line[b] == line[b - 1] {
                        run += 1;
                    } else {
                        if run >= 5 {
                            p += 3 + (run - 5) as u32;
                        }
                        run = 1;
                    }
                }
                for b in 0..s.saturating_sub(6) {
                    if line[b..b + 7] == finder {
                        let light = |r: std::ops::Range<usize>| r.into_iter().all(|k| k >= s || !line[k]);
                        let before = b >= 4 && light(b - 4..b);
                        let after = light(b + 7..b + 11);
                        if before || after {
                            p += 40;
                        }
                    }
                }
            }
        }
        // 2×2 blocks of one colour
        for y in 0..s - 1 {
            for x in 0..s - 1 {
                let c = at(x, y);
                if c == at(x + 1, y) && c == at(x, y + 1) && c == at(x + 1, y + 1) {
                    p += 3;
                }
            }
        }
        // balance of dark and light
        let dark = self.m.iter().filter(|&&d| d).count() as i64;
        let total = (s * s) as i64;
        let k = ((dark * 20 - total * 10).abs() + total - 1) / total - 1;
        p + (k.max(0) * 10) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_fits_version_2_with_finders() {
        let q = encode(b"https://ufoscene.net").expect("fits");
        assert_eq!(q.size, 25, "20 bytes at level M need version 2");
        // finder pattern corners: dark ring, light ring, dark centre
        for (ox, oy) in [(0, 0), (18, 0), (0, 18)] {
            assert!(q.get(ox, oy) && q.get(ox + 6, oy + 6) && q.get(ox + 3, oy + 3));
            assert!(!q.get(ox + 1, oy + 1) && !q.get(ox + 5, oy + 5));
        }
        assert!(q.get(8, 25 - 8), "dark module");
        assert!(encode(&[b'x'; 107]).is_none());
    }

    #[test]
    fn reed_solomon_known_answer() {
        // ISO 18004 annex example: "01234567" in version 1-M
        let data = [0x10, 0x20, 0x0C, 0x56, 0x61, 0x80, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11];
        let ecc = rs_remainder(&data, &rs_divisor(10));
        assert_eq!(ecc, [0xA5, 0x24, 0xD4, 0xC1, 0xED, 0x36, 0xC7, 0x87, 0x2C, 0x55]);
    }
}
