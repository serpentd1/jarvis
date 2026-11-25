//! Fond d'écran réduit et flouté : sert de « verre dépoli » derrière les widgets.

use image::{DynamicImage, GenericImageView, RgbaImage, imageops::FilterType};
use tiny_skia::{IntSize, Pixmap};
use windows_sys::Win32::Graphics::Gdi::{COLOR_BACKGROUND, GetSysColor};
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETDESKWALLPAPER, SystemParametersInfoW};

use crate::gfx;
use crate::sys;

/// Numéro de version du fond flouté (change à chaque chargement ou nouveau flou).
static GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct Backdrop {
    pub pm: Pixmap,
    /// Version de ce fond : l'île garde un fond agrandi tant qu'elle ne change pas.
    pub version: u64,
    /// pixels d'écran par pixel du fond réduit
    pub scale: f32,
    pub accent: [u8; 3],
    base: Vec<u8>,
    w: u32,
    h: u32,
    blur: f32,
}

impl Backdrop {
    /// Fond d'écran au quart de la résolution (≈ 0,5 Mo pour un écran 1080p).
    pub fn load(sw: i32, sh: i32, blur: f32) -> Backdrop {
        let (w, h) = ((sw.max(4) / 4) as u32, (sh.max(4) / 4) as u32);
        let mut base = compose(w, h, sw.max(1) as u32);
        saturate(&mut base, 1.25);
        let accent = accent_of(&base);
        let mut b = Backdrop {
            pm: Pixmap::new(w, h).unwrap_or_else(|| Pixmap::new(1, 1).unwrap()),
            version: 0,
            scale: sw as f32 / w as f32,
            accent,
            base,
            w,
            h,
            blur: -1.0,
        };
        b.set_blur(blur);
        b
    }

    pub fn set_blur(&mut self, blur: f32) {
        if (blur - self.blur).abs() < 0.001 {
            return;
        }
        self.blur = blur;
        let mut d = self.base.clone();
        let r = (1.0 + blur.clamp(0.0, 1.0) * 12.0).round() as usize;
        gfx::box_blur(&mut d, self.w as usize, self.h as usize, r, 3);
        if let Some(pm) = IntSize::from_wh(self.w, self.h).and_then(|s| Pixmap::from_vec(d, s)) {
            self.pm = pm;
            self.version = GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

fn wallpaper() -> Option<DynamicImage> {
    let mut buf = [0u16; 520];
    unsafe {
        SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, buf.as_mut_ptr() as _, 0);
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path = String::from_utf16_lossy(&buf[..n]);
    let mut cands = Vec::new();
    if !path.is_empty() {
        cands.push(std::path::PathBuf::from(path));
    }
    if let Some(ad) = std::env::var_os("APPDATA") {
        cands.push(std::path::PathBuf::from(ad).join("Microsoft\\Windows\\Themes\\TranscodedWallpaper"));
    }
    cands.into_iter().find_map(|p| {
        image::ImageReader::open(&p).ok()?.with_guessed_format().ok()?.decode().ok()
    })
}

fn desktop_color() -> [u8; 3] {
    let c = unsafe { GetSysColor(COLOR_BACKGROUND) };
    [(c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8]
}

fn blit(out: &mut [u8], w: u32, h: u32, t: &RgbaImage, x0: i32, y0: i32) {
    let (tw, th) = t.dimensions();
    let src = t.as_raw();
    for ty in 0..th as i32 {
        let y = y0 + ty;
        if y < 0 || y >= h as i32 {
            continue;
        }
        for tx in 0..tw as i32 {
            let x = x0 + tx;
            if x < 0 || x >= w as i32 {
                continue;
            }
            let si = ((ty as u32 * tw + tx as u32) * 4) as usize;
            let di = ((y as u32 * w + x as u32) * 4) as usize;
            out[di..di + 3].copy_from_slice(&src[si..si + 3]);
            out[di + 3] = 255;
        }
    }
}

/// Reproduit le bureau (fond + mode d'affichage) dans un tampon RGBA de w×h.
pub fn compose(w: u32, h: u32, screen_w: u32) -> Vec<u8> {
    let bg = desktop_color();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        out.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }
    let Some(img) = wallpaper() else { return out };
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return out;
    }
    let (style, tile) = sys::wallpaper_style();
    let (fw, fh) = (w as f32, h as f32);
    let ks = w as f32 / screen_w as f32;
    let resize = |nw: u32, nh: u32| -> RgbaImage {
        let (nw, nh) = (nw.max(1), nh.max(1));
        if nw <= iw && nh <= ih {
            img.thumbnail_exact(nw, nh).to_rgba8()
        } else {
            img.resize_exact(nw, nh, FilterType::Triangle).to_rgba8()
        }
    };
    let centered = |out: &mut Vec<u8>, t: &RgbaImage| {
        let (tw, th) = t.dimensions();
        blit(out, w, h, t, (w as i32 - tw as i32) / 2, (h as i32 - th as i32) / 2);
    };
    match (style, tile) {
        (0, true) => {
            let t = resize((iw as f32 * ks).round() as u32, (ih as f32 * ks).round() as u32);
            let (tw, th) = t.dimensions();
            let mut y = 0;
            while y < h as i32 {
                let mut x = 0;
                while x < w as i32 {
                    blit(&mut out, w, h, &t, x, y);
                    x += tw as i32;
                }
                y += th as i32;
            }
        }
        (0, false) => {
            let t = resize((iw as f32 * ks).round() as u32, (ih as f32 * ks).round() as u32);
            centered(&mut out, &t);
        }
        (2, _) => {
            let t = resize(w, h);
            blit(&mut out, w, h, &t, 0, 0);
        }
        (6, _) => {
            let k = (fw / iw as f32).min(fh / ih as f32);
            let t = resize((iw as f32 * k).round() as u32, (ih as f32 * k).round() as u32);
            centered(&mut out, &t);
        }
        _ => {
            let k = (fw / iw as f32).max(fh / ih as f32);
            let t = resize((iw as f32 * k).ceil() as u32, (ih as f32 * k).ceil() as u32);
            centered(&mut out, &t);
        }
    }
    out
}

/// Couleurs un peu plus vives, comme le verre dépoli d'iOS.
fn saturate(d: &mut [u8], k: f32) {
    for p in d.chunks_exact_mut(4) {
        let l = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
        for c in 0..3 {
            p[c] = (l + (p[c] as f32 - l) * k).clamp(0.0, 255.0) as u8;
        }
    }
}

fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8]
}

/// Couleur dominante (teinte la plus présente parmi les couleurs vives).
fn accent_of(d: &[u8]) -> [u8; 3] {
    let mut bins = [0f32; 24];
    let mut hs = [0f32; 24];
    for px in d.chunks_exact(4).step_by(3) {
        let (r, g, b) = (px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0);
        let mx = r.max(g).max(b);
        let c = mx - r.min(g).min(b);
        if c < 0.08 || mx < 0.15 {
            continue;
        }
        let h = (if mx == r {
            ((g - b) / c).rem_euclid(6.0)
        } else if mx == g {
            (b - r) / c + 2.0
        } else {
            (r - g) / c + 4.0
        }) * 60.0;
        let wgt = (c / mx) * mx;
        let i = ((h / 15.0) as usize).min(23);
        bins[i] += wgt;
        hs[i] += h * wgt;
    }
    let (i, best) = bins.iter().enumerate().fold((0, 0.0f32), |a, (i, &v)| if v > a.1 { (i, v) } else { a });
    if best < 20.0 {
        return [52, 199, 89];
    }
    hsv(hs[i] / best, 0.62, 0.92)
}
