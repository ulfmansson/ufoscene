//! The TV through the kernel's DRM/KMS interface, with no X, Wayland or GPU library: find a
//! connected screen, pick a mode, make two "dumb" framebuffers in memory and page-flip between
//! them on the vertical blank. Plain ioctls on /dev/dri/cardN, as in the kernel's
//! include/uapi/drm/drm.h and drm_mode.h; the structs below must match those byte for byte.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;

#[repr(C)]
#[derive(Default)]
struct CardRes {
    fb_id_ptr: u64,
    crtc_id_ptr: u64,
    connector_id_ptr: u64,
    encoder_id_ptr: u64,
    count_fbs: u32,
    count_crtcs: u32,
    count_connectors: u32,
    count_encoders: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct ModeInfo {
    clock: u32,
    pub hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    pub vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    pub vrefresh: u32,
    flags: u32,
    type_: u32,
    name: [u8; 32],
}

#[repr(C)]
#[derive(Default)]
struct GetConnector {
    encoders_ptr: u64,
    modes_ptr: u64,
    props_ptr: u64,
    prop_values_ptr: u64,
    count_modes: u32,
    count_props: u32,
    count_encoders: u32,
    encoder_id: u32,
    connector_id: u32,
    connector_type: u32,
    connector_type_id: u32,
    connection: u32,
    mm_width: u32,
    mm_height: u32,
    subpixel: u32,
    pad: u32,
}

#[repr(C)]
#[derive(Default)]
struct GetEncoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
#[derive(Default)]
struct Crtc {
    set_connectors_ptr: u64,
    count_connectors: u32,
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    gamma_size: u32,
    mode_valid: u32,
    mode: ModeInfo,
}

#[repr(C)]
#[derive(Default)]
struct CreateDumb {
    height: u32,
    width: u32,
    bpp: u32,
    flags: u32,
    handle: u32,
    pitch: u32,
    size: u64,
}

#[repr(C)]
#[derive(Default)]
struct MapDumb {
    handle: u32,
    pad: u32,
    offset: u64,
}

#[repr(C)]
#[derive(Default)]
struct FbCmd {
    fb_id: u32,
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    depth: u32,
    handle: u32,
}

#[repr(C)]
#[derive(Default)]
struct PageFlip {
    crtc_id: u32,
    fb_id: u32,
    flags: u32,
    reserved: u32,
    user_data: u64,
}

const _: () = {
    assert!(size_of::<CardRes>() == 64);
    assert!(size_of::<ModeInfo>() == 68);
    assert!(size_of::<GetConnector>() == 80);
    assert!(size_of::<GetEncoder>() == 20);
    assert!(size_of::<Crtc>() == 104);
    assert!(size_of::<CreateDumb>() == 32);
    assert!(size_of::<MapDumb>() == 16);
    assert!(size_of::<FbCmd>() == 28);
    assert!(size_of::<PageFlip>() == 24);
};

// _IOWR('d', nr, size) in the generic Linux layout (arm, arm64, x86)
const fn iowr(nr: u32, size: usize) -> u64 {
    ((3u64) << 30) | ((size as u64) << 16) | ((b'd' as u64) << 8) | nr as u64
}
const fn io(nr: u32) -> u64 {
    ((b'd' as u64) << 8) | nr as u64
}
const SET_MASTER: u64 = io(0x1e);
const GET_RESOURCES: u64 = iowr(0xA0, size_of::<CardRes>());
const GET_CRTC: u64 = iowr(0xA1, size_of::<Crtc>());
const SET_CRTC: u64 = iowr(0xA2, size_of::<Crtc>());
const GET_ENCODER: u64 = iowr(0xA6, size_of::<GetEncoder>());
const GET_CONNECTOR: u64 = iowr(0xA7, size_of::<GetConnector>());
const ADD_FB: u64 = iowr(0xAE, size_of::<FbCmd>());
const RM_FB: u64 = iowr(0xAF, size_of::<u32>());
const PAGE_FLIP: u64 = iowr(0xB0, size_of::<PageFlip>());
const CREATE_DUMB: u64 = iowr(0xB2, size_of::<CreateDumb>());
const MAP_DUMB: u64 = iowr(0xB3, size_of::<MapDumb>());
const DESTROY_DUMB: u64 = iowr(0xB4, size_of::<u32>());

const CONNECTED: u32 = 1;
const MODE_TYPE_PREFERRED: u32 = 1 << 3;
const PAGE_FLIP_EVENT: u32 = 1;
const EVENT_FLIP_COMPLETE: u32 = 2;

const PROT_READ: i32 = 1;
const PROT_WRITE: i32 = 2;
const MAP_SHARED: i32 = 1;
const EINTR: i32 = 4;
const EAGAIN: i32 = 11;

#[cfg(any(target_pointer_width = "64", target_env = "musl"))]
type OffT = i64;
#[cfg(not(any(target_pointer_width = "64", target_env = "musl")))]
type OffT = i32;

unsafe extern "C" {
    fn ioctl(fd: i32, request: std::ffi::c_ulong, ...) -> i32;
    fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, off: OffT) -> *mut u8;
    fn munmap(addr: *mut u8, len: usize) -> i32;
}

fn ctl<T>(f: &File, req: u64, arg: &mut T) -> io::Result<()> {
    loop {
        // SAFETY: `arg` is a #[repr(C)] struct of the size encoded in `req`
        let r = unsafe { ioctl(f.as_raw_fd(), req as std::ffi::c_ulong, arg as *mut T) };
        if r == 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if !matches!(e.raw_os_error(), Some(EINTR) | Some(EAGAIN)) {
            return Err(e);
        }
    }
}

fn ptr<T>(v: &mut [T]) -> u64 {
    v.as_mut_ptr() as usize as u64
}

struct Buffer {
    handle: u32,
    fb: u32,
    map: *mut u8,
    size: usize,
    pitch: usize,
}

pub struct Screen {
    card: File,
    crtc: u32,
    connector: u32,
    pub mode: ModeInfo,
    saved: Crtc,
    bufs: [Buffer; 2],
    back: usize,
    flipping: bool,
    pub path: String,
}

fn mode_name(m: &ModeInfo) -> String {
    format!("{}x{}@{}", m.hdisplay, m.vdisplay, m.vrefresh)
}

/// The mode to use: `want` ("1280x720" or "1280x720@50") if the screen has it; otherwise
/// 1920×1080 at 60 Hz (or 50), which every TV has and which scales 384×216 by exactly 5;
/// otherwise what the screen prefers.
pub fn choose_mode(modes: &[ModeInfo], want: Option<&str>) -> Option<ModeInfo> {
    let is = |m: &ModeInfo, w: u16, h: u16| m.hdisplay == w && m.vdisplay == h;
    if let Some(want) = want {
        let (size, hz) = want.split_once('@').map_or((want, None), |(s, hz)| (s, hz.parse::<u32>().ok()));
        if let Some((w, h)) = size.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))) {
            let found = modes.iter().filter(|m| is(m, w, h)).find(|m| hz.is_none_or(|hz| m.vrefresh == hz));
            if found.is_some() {
                return found.copied();
            }
        }
        eprintln!("ufo-native: the screen has no mode {want}; using the default");
    }
    for hz in [60, 50] {
        if let Some(m) = modes.iter().find(|m| is(m, 1920, 1080) && m.vrefresh == hz) {
            return Some(*m);
        }
    }
    modes.iter().find(|m| m.type_ & MODE_TYPE_PREFERRED != 0).or(modes.first()).copied()
}

impl Screen {
    /// Open the first card with a connected screen (on a Pi 5 the display is card1; card0 is
    /// the 3D engine and has no connectors).
    pub fn open(card: Option<&str>, want_mode: Option<&str>) -> io::Result<Screen> {
        let paths: Vec<String> = match card {
            Some(c) => vec![c.to_string()],
            None => (0..4).map(|i| format!("/dev/dri/card{i}")).collect(),
        };
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no /dev/dri/card* with a connected screen");
        for path in paths {
            let f = match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(f) => f,
                Err(e) => {
                    if e.kind() != io::ErrorKind::NotFound {
                        last = io::Error::new(e.kind(), format!("{path}: {e}"));
                    }
                    continue;
                }
            };
            match Self::on_card(f, &path, want_mode) {
                Ok(s) => return Ok(s),
                Err(e) => last = io::Error::new(e.kind(), format!("{path}: {e}")),
            }
        }
        Err(last)
    }

    fn on_card(card: File, path: &str, want_mode: Option<&str>) -> io::Result<Screen> {
        // master is ours already if nobody else has the card open; asking again does no harm
        let _ = ctl(&card, SET_MASTER, &mut 0u32);
        let mut res = CardRes::default();
        ctl(&card, GET_RESOURCES, &mut res)?;
        let mut crtcs = vec![0u32; res.count_crtcs as usize];
        let mut conns = vec![0u32; res.count_connectors as usize];
        let mut encs = vec![0u32; res.count_encoders as usize];
        let mut fbs = vec![0u32; res.count_fbs as usize];
        res.crtc_id_ptr = ptr(&mut crtcs);
        res.connector_id_ptr = ptr(&mut conns);
        res.encoder_id_ptr = ptr(&mut encs);
        res.fb_id_ptr = ptr(&mut fbs);
        ctl(&card, GET_RESOURCES, &mut res)?;
        crtcs.truncate(res.count_crtcs as usize);
        conns.truncate(res.count_connectors as usize);

        for &id in &conns {
            // first call: how many modes and encoders (and the kernel probes the screen)
            let mut c = GetConnector { connector_id: id, ..Default::default() };
            if ctl(&card, GET_CONNECTOR, &mut c).is_err() || c.connection != CONNECTED || c.count_modes == 0 {
                continue;
            }
            let mut modes = vec![ModeInfo::default(); c.count_modes as usize];
            let mut cencs = vec![0u32; c.count_encoders as usize];
            let current_encoder = c.encoder_id;
            c = GetConnector {
                connector_id: id,
                count_modes: modes.len() as u32,
                count_encoders: cencs.len() as u32,
                modes_ptr: ptr(&mut modes),
                encoders_ptr: ptr(&mut cencs),
                ..Default::default()
            };
            ctl(&card, GET_CONNECTOR, &mut c)?;
            modes.truncate(c.count_modes as usize);
            cencs.truncate(c.count_encoders as usize);
            let Some(mode) = choose_mode(&modes, want_mode) else { continue };

            // the CRTC the encoder drives now, or the first one any of its encoders can use
            let mut crtc = 0;
            for &e in std::iter::once(&current_encoder).chain(cencs.iter()) {
                if e == 0 {
                    continue;
                }
                let mut enc = GetEncoder { encoder_id: e, ..Default::default() };
                if ctl(&card, GET_ENCODER, &mut enc).is_err() {
                    continue;
                }
                if enc.crtc_id != 0 {
                    crtc = enc.crtc_id;
                    break;
                }
                if let Some(i) = (0..crtcs.len()).find(|&i| enc.possible_crtcs & (1 << i) != 0) {
                    crtc = crtcs[i];
                    break;
                }
            }
            if crtc == 0 {
                continue;
            }
            let mut saved = Crtc { crtc_id: crtc, ..Default::default() };
            let _ = ctl(&card, GET_CRTC, &mut saved);
            let bufs = [
                Self::buffer(&card, mode.hdisplay as u32, mode.vdisplay as u32)?,
                Self::buffer(&card, mode.hdisplay as u32, mode.vdisplay as u32)?,
            ];
            let mut s = Screen { card, crtc, connector: id, mode, saved, bufs, back: 1, flipping: false, path: path.into() };
            s.set_crtc(0)?;
            return Ok(s);
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "no connected screen"))
    }

    fn buffer(card: &File, width: u32, height: u32) -> io::Result<Buffer> {
        let mut c = CreateDumb { width, height, bpp: 32, ..Default::default() };
        ctl(card, CREATE_DUMB, &mut c)?;
        let mut f = FbCmd { width, height, pitch: c.pitch, bpp: 32, depth: 24, handle: c.handle, ..Default::default() };
        ctl(card, ADD_FB, &mut f)?;
        let mut m = MapDumb { handle: c.handle, ..Default::default() };
        ctl(card, MAP_DUMB, &mut m)?;
        let size = c.size as usize;
        // SAFETY: the offset is the one the kernel gave for this buffer on this fd
        let map = unsafe { mmap(std::ptr::null_mut(), size, PROT_READ | PROT_WRITE, MAP_SHARED, card.as_raw_fd(), m.offset as OffT) };
        if map.is_null() || map as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `size` bytes are mapped; black to start with (the letterbox stays black)
        unsafe { std::ptr::write_bytes(map, 0, size) };
        Ok(Buffer { handle: c.handle, fb: f.fb_id, map, size, pitch: c.pitch as usize })
    }

    fn set_crtc(&mut self, buf: usize) -> io::Result<()> {
        let mut conn = [self.connector];
        let mut c = Crtc {
            set_connectors_ptr: ptr(&mut conn),
            count_connectors: 1,
            crtc_id: self.crtc,
            fb_id: self.bufs[buf].fb,
            mode_valid: 1,
            mode: self.mode,
            ..Default::default()
        };
        ctl(&self.card, SET_CRTC, &mut c)
    }

    pub fn width(&self) -> usize {
        self.mode.hdisplay as usize
    }

    pub fn height(&self) -> usize {
        self.mode.vdisplay as usize
    }

    pub fn describe(&self) -> String {
        format!("{} on {}", mode_name(&self.mode), self.path)
    }

    /// The buffer to draw the next frame into, as pixels, and its pitch in pixels.
    pub fn back(&mut self) -> (&mut [u32], usize) {
        let b = &self.bufs[self.back];
        // SAFETY: the mapping is `size` bytes, page aligned, alive as long as `self`
        let px = unsafe { std::slice::from_raw_parts_mut(b.map as *mut u32, b.size / 4) };
        (px, b.pitch / 4)
    }

    /// Show the back buffer at the next vertical blank, after waiting for the previous flip:
    /// the pace of the loop is the TV's refresh rate.
    pub fn flip(&mut self) -> io::Result<()> {
        self.wait()?;
        let mut f = PageFlip { crtc_id: self.crtc, fb_id: self.bufs[self.back].fb, flags: PAGE_FLIP_EVENT, ..Default::default() };
        match ctl(&self.card, PAGE_FLIP, &mut f) {
            Ok(()) => self.flipping = true,
            // a VT switch took the screen away, or the driver will not flip: set it outright
            Err(_) => {
                let _ = ctl(&self.card, SET_MASTER, &mut 0u32);
                self.set_crtc(self.back)?;
            }
        }
        self.back ^= 1;
        Ok(())
    }

    fn wait(&mut self) -> io::Result<()> {
        let mut buf = [0u8; 1024];
        while self.flipping {
            let n = match (&self.card).read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            let mut i = 0;
            while i + 8 <= n {
                let kind = u32::from_ne_bytes(buf[i..i + 4].try_into().unwrap());
                let len = u32::from_ne_bytes(buf[i + 4..i + 8].try_into().unwrap()) as usize;
                if kind == EVENT_FLIP_COMPLETE {
                    self.flipping = false;
                }
                i += len.max(8);
            }
        }
        Ok(())
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = self.wait();
        // give the console its picture back
        if self.saved.mode_valid != 0 {
            let mut conn = [self.connector];
            self.saved.set_connectors_ptr = ptr(&mut conn);
            self.saved.count_connectors = 1;
            let _ = ctl(&self.card, SET_CRTC, &mut self.saved);
        }
        for b in &self.bufs {
            // SAFETY: mapped in `buffer` with this size
            unsafe { munmap(b.map, b.size) };
            let _ = ctl(&self.card, RM_FB, &mut { b.fb });
            let _ = ctl(&self.card, DESTROY_DUMB, &mut { b.handle });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(w: u16, h: u16, hz: u32, preferred: bool) -> ModeInfo {
        ModeInfo { hdisplay: w, vdisplay: h, vrefresh: hz, type_: if preferred { MODE_TYPE_PREFERRED } else { 0 }, ..Default::default() }
    }

    #[test]
    fn ioctl_numbers_match_the_kernel() {
        // from libdrm's xf86drm / strace on a Pi
        assert_eq!(GET_RESOURCES, 0xC04064A0);
        assert_eq!(SET_CRTC, 0xC06864A2);
        assert_eq!(GET_CONNECTOR, 0xC05064A7);
        assert_eq!(CREATE_DUMB, 0xC02064B2);
        assert_eq!(PAGE_FLIP, 0xC01864B0);
        assert_eq!(SET_MASTER, 0x641E);
    }

    #[test]
    fn mode_choice() {
        let tv4k = [m(3840, 2160, 30, true), m(1920, 1080, 50, false), m(1920, 1080, 60, false), m(1280, 720, 60, false)];
        assert_eq!(mode_name(&choose_mode(&tv4k, None).unwrap()), "1920x1080@60");
        assert_eq!(mode_name(&choose_mode(&tv4k, Some("1280x720")).unwrap()), "1280x720@60");
        assert_eq!(mode_name(&choose_mode(&tv4k, Some("1920x1080@50")).unwrap()), "1920x1080@50");
        assert_eq!(mode_name(&choose_mode(&tv4k, Some("800x600")).unwrap()), "1920x1080@60");
        let monitor = [m(1024, 768, 60, false), m(1280, 1024, 60, true)];
        assert_eq!(mode_name(&choose_mode(&monitor, None).unwrap()), "1280x1024@60");
        assert!(choose_mode(&[], None).is_none());
    }
}
