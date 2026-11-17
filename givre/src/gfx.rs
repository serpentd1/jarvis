//! Dessin : couleurs, texte (polices Windows), formes, flou, icônes météo, envoi à l'écran.

use std::f32::consts::PI;
use std::ptr::null_mut;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, GradientStop, LineCap, LinearGradient, Paint, Path,
    PathBuilder, Pattern, Pixmap, PixmapPaint, Point, Rect, SpreadMode, Stroke, StrokeDash, Transform,
};
use windows_sys::Win32::Foundation::{HWND, POINT, SIZE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, HICON, ICONINFO, ULW_ALPHA, UpdateLayeredWindow,
};

use crate::backdrop::Backdrop;

// ---------- Couleurs ----------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

pub const WHITE: Rgba = Rgba(1.0, 1.0, 1.0, 1.0);

impl Rgba {
    pub fn rgb8(r: u8, g: u8, b: u8) -> Rgba {
        Rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
    }
    pub fn from(c: [u8; 3]) -> Rgba {
        Self::rgb8(c[0], c[1], c[2])
    }
    pub fn a(self, a: f32) -> Rgba {
        Rgba(self.0, self.1, self.2, self.3 * a)
    }
    pub fn mix(self, o: Rgba, t: f32) -> Rgba {
        Rgba(
            self.0 + (o.0 - self.0) * t,
            self.1 + (o.1 - self.1) * t,
            self.2 + (o.2 - self.2) * t,
            self.3 + (o.3 - self.3) * t,
        )
    }
    pub fn color(self) -> Color {
        let c = |v: f32| v.clamp(0.0, 1.0);
        Color::from_rgba(c(self.0), c(self.1), c(self.2), c(self.3)).unwrap_or(Color::BLACK)
    }
}

pub struct Theme {
    pub dark: bool,
    pub fg: Rgba,
    pub fg2: Rgba,
    pub fg3: Rgba,
    pub sep: Rgba,
    pub track: Rgba,
    pub tint: Rgba,
    pub border: Rgba,
    pub accent: Rgba,
    pub sun: Rgba,
    pub moon: Rgba,
    pub cloud: Rgba,
    pub rain: Rgba,
    pub snow: Rgba,
    pub gamma: f32,
}

impl Theme {
    pub fn new(dark: bool, accent: [u8; 3], opacity: f32) -> Theme {
        let accent = Rgba::from(accent);
        let o = opacity.clamp(0.0, 1.0);
        if dark {
            Theme {
                dark,
                fg: Rgba(1.0, 1.0, 1.0, 0.96),
                fg2: Rgba(1.0, 1.0, 1.0, 0.64),
                fg3: Rgba(1.0, 1.0, 1.0, 0.40),
                sep: Rgba(1.0, 1.0, 1.0, 0.12),
                track: Rgba(1.0, 1.0, 1.0, 0.16),
                tint: Rgba::rgb8(24, 24, 28).a(0.12 + 0.8 * o),
                border: Rgba(1.0, 1.0, 1.0, 0.18),
                accent,
                sun: Rgba::rgb8(255, 204, 0),
                moon: Rgba::rgb8(232, 232, 242),
                cloud: Rgba::rgb8(238, 239, 244),
                rain: Rgba::rgb8(100, 200, 255),
                snow: WHITE,
                gamma: 0.8,
            }
        } else {
            let ink = Rgba(0.08, 0.08, 0.10, 1.0);
            Theme {
                dark,
                fg: ink.a(0.92),
                fg2: ink.a(0.60),
                fg3: ink.a(0.38),
                sep: Rgba(0.0, 0.0, 0.0, 0.09),
                track: Rgba(0.0, 0.0, 0.0, 0.10),
                tint: Rgba::rgb8(250, 250, 252).a(0.38 + 0.6 * o),
                border: Rgba(1.0, 1.0, 1.0, 0.6),
                accent,
                sun: Rgba::rgb8(255, 176, 0),
                moon: Rgba::rgb8(150, 150, 170),
                cloud: Rgba::rgb8(172, 176, 188),
                rain: Rgba::rgb8(10, 132, 255),
                snow: Rgba::rgb8(120, 170, 230),
                gamma: 1.0,
            }
        }
    }
}

pub fn paint<'a>(c: Rgba) -> Paint<'a> {
    let mut p = Paint::default();
    p.set_color(c.color());
    p.anti_alias = true;
    p
}

// ---------- Texte ----------

#[derive(Clone, Copy)]
pub enum Wt {
    Light = 0,
    Regular = 1,
    Semibold = 2,
}

pub struct Fonts {
    f: Vec<FontVec>,
}

impl Fonts {
    pub fn load() -> Option<Fonts> {
        let dir = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
        let pick = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| std::fs::read(dir.join(n)).ok().and_then(|b| FontVec::try_from_vec(b).ok()))
        };
        Some(Fonts {
            f: vec![
                pick(&["segoeuil.ttf", "segoeuisl.ttf", "segoeui.ttf", "arial.ttf"])?,
                pick(&["segoeui.ttf", "arial.ttf"])?,
                pick(&["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf", "arial.ttf"])?,
            ],
        })
    }

    fn scale(&self, w: Wt, size: f32) -> PxScale {
        let f = &self.f[w as usize];
        let upm = f.units_per_em().unwrap_or(2048.0);
        PxScale::from(size * f.height_unscaled() / upm)
    }

    /// Largeur du texte (taille = taille CSS en pixels).
    pub fn measure(&self, w: Wt, size: f32, s: &str, track: f32) -> f32 {
        let font = &self.f[w as usize];
        let sf = font.as_scaled(self.scale(w, size));
        let mut x = 0.0;
        let mut prev = None;
        let mut n = 0;
        for ch in s.chars() {
            let id = font.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            x += sf.h_advance(id) + track;
            prev = Some(id);
            n += 1;
        }
        if n > 0 { x - track } else { 0.0 }
    }

    /// Dessine le texte ; (x, y) = début de la ligne de base. Renvoie la largeur.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&self, pm: &mut Pixmap, w: Wt, size: f32, x: f32, y: f32, c: Rgba, s: &str, track: f32, gamma: f32) -> f32 {
        let font = &self.f[w as usize];
        let scale = self.scale(w, size);
        let sf = font.as_scaled(scale);
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let data = pm.data_mut();
        let mut cx = x;
        let mut prev = None;
        let mut n = 0;
        for ch in s.chars() {
            let id = font.glyph_id(ch);
            if let Some(p) = prev {
                cx += sf.kern(p, id);
            }
            let g = id.with_scale_and_position(scale, point(cx, y));
            cx += sf.h_advance(id) + track;
            prev = Some(id);
            n += 1;
            let Some(og) = font.outline_glyph(g) else { continue };
            let b = og.px_bounds();
            let (bx, by) = (b.min.x as i32, b.min.y as i32);
            og.draw(|gx, gy, cov| {
                let (px, py) = (bx + gx as i32, by + gy as i32);
                if px < 0 || py < 0 || px >= pw || py >= ph {
                    return;
                }
                let a = c.3 * cov.clamp(0.0, 1.0).powf(gamma);
                if a <= 0.002 {
                    return;
                }
                let i = ((py * pw + px) * 4) as usize;
                let inv = 1.0 - a;
                data[i] = (c.0 * a * 255.0 + data[i] as f32 * inv + 0.5).min(255.0) as u8;
                data[i + 1] = (c.1 * a * 255.0 + data[i + 1] as f32 * inv + 0.5).min(255.0) as u8;
                data[i + 2] = (c.2 * a * 255.0 + data[i + 2] as f32 * inv + 0.5).min(255.0) as u8;
                data[i + 3] = (a * 255.0 + data[i + 3] as f32 * inv + 0.5).min(255.0) as u8;
            });
        }
        if n > 0 { cx - track - x } else { 0.0 }
    }

    /// Coupe le texte avec « … » s'il dépasse `max`.
    pub fn ellipsize(&self, w: Wt, size: f32, s: &str, max: f32) -> String {
        if self.measure(w, size, s, 0.0) <= max {
            return s.to_string();
        }
        let mut out: String = s.to_string();
        while !out.is_empty() {
            out.pop();
            let cand = format!("{}…", out.trim_end());
            if self.measure(w, size, &cand, 0.0) <= max {
                return cand;
            }
        }
        "…".into()
    }
}

// ---------- Formes ----------

/// Rectangle arrondi ; `cont` = coins « continus » façon iPhone (courbure progressive).
pub fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32, cont: bool) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let r = r.max(0.0);
    if r < 0.5 {
        pb.push_rect(Rect::from_xywh(x, y, w, h)?);
        return pb.finish();
    }
    let lim = (w / 2.0).min(h / 2.0);
    let (a, c) = if cont {
        let a = (r * 1.528).min(lim);
        (a, a * 0.178)
    } else {
        let a = r.min(lim);
        (a, a * 0.4477)
    };
    let (x2, y2) = (x + w, y + h);
    pb.move_to(x + a, y);
    pb.line_to(x2 - a, y);
    pb.cubic_to(x2 - c, y, x2, y + c, x2, y + a);
    pb.line_to(x2, y2 - a);
    pb.cubic_to(x2, y2 - c, x2 - c, y2, x2 - a, y2);
    pb.line_to(x + a, y2);
    pb.cubic_to(x + c, y2, x, y2 - c, x, y2 - a);
    pb.line_to(x, y + a);
    pb.cubic_to(x, y + c, x + c, y, x + a, y);
    pb.close();
    pb.finish()
}

pub fn fill(pm: &mut Pixmap, p: &Path, c: Rgba) {
    pm.fill_path(p, &paint(c), FillRule::Winding, Transform::identity(), None);
}

pub fn stroke(pm: &mut Pixmap, p: &Path, c: Rgba, w: f32) {
    let st = Stroke { width: w, line_cap: LineCap::Round, ..Default::default() };
    pm.stroke_path(p, &paint(c), &st, Transform::identity(), None);
}

pub fn line(pm: &mut Pixmap, x1: f32, y1: f32, x2: f32, y2: f32, w: f32, c: Rgba) {
    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    if let Some(p) = pb.finish() {
        stroke(pm, &p, c, w);
    }
}

pub fn circle(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, c: Rgba) {
    if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
        fill(pm, &p, c);
    }
}

pub fn grad_fill(pm: &mut Pixmap, p: &Path, x0: f32, x1: f32, c0: Rgba, c1: Rgba) {
    let sh = LinearGradient::new(
        Point::from_xy(x0, 0.0),
        Point::from_xy(x1.max(x0 + 0.5), 0.0),
        vec![GradientStop::new(0.0, c0.color()), GradientStop::new(1.0, c1.color())],
        SpreadMode::Pad,
        Transform::identity(),
    );
    let mut pt = paint(c0);
    if let Some(sh) = sh {
        pt.shader = sh;
    }
    pm.fill_path(p, &pt, FillRule::Winding, Transform::identity(), None);
}

// ---------- Flou ----------

fn blur_1d(src: &[u8], dst: &mut [u8], len: usize, lines: usize, step: usize, ls: usize, r: usize) {
    let div = (2 * r + 1) as u32;
    let half = div / 2;
    let last = len - 1;
    for l in 0..lines {
        let base = l * ls;
        let mut acc = [0u32; 4];
        for i in 0..=2 * r {
            let k = (i as isize - r as isize).clamp(0, last as isize) as usize;
            let o = base + k * step;
            for c in 0..4 {
                acc[c] += src[o + c] as u32;
            }
        }
        for i in 0..len {
            let o = base + i * step;
            for c in 0..4 {
                dst[o + c] = ((acc[c] + half) / div) as u8;
            }
            let add = base + (i + r + 1).min(last) * step;
            let sub = base + i.saturating_sub(r) * step;
            for c in 0..4 {
                acc[c] = acc[c] + src[add + c] as u32 - src[sub + c] as u32;
            }
        }
    }
}

/// Flou rapide (boîte répétée ≈ flou gaussien) sur un tampon RGBA.
pub fn box_blur(d: &mut [u8], w: usize, h: usize, r: usize, passes: usize) {
    if r == 0 || w == 0 || h == 0 {
        return;
    }
    let mut tmp = vec![0u8; d.len()];
    for _ in 0..passes {
        blur_1d(d, &mut tmp, w, h, 4, w * 4, r);
        blur_1d(&tmp, d, h, w, w * 4, 4, r);
    }
}

fn shadow(pm: &mut Pixmap, path: &Path, blur: f32, dy: f32, alpha: f32) {
    let (w, h) = (pm.width(), pm.height());
    let Some(mut sh) = Pixmap::new(w, h) else { return };
    let mut p = paint(Rgba(0.0, 0.0, 0.0, alpha));
    sh.fill_path(path, &p, FillRule::Winding, Transform::from_translate(0.0, dy), None);
    box_blur(sh.data_mut(), w as usize, h as usize, (blur / 2.0).round().max(1.0) as usize, 3);
    // l'ombre ne doit pas assombrir l'intérieur de la carte
    p.blend_mode = BlendMode::DestinationOut;
    p.set_color(Color::BLACK);
    sh.fill_path(path, &p, FillRule::Winding, Transform::identity(), None);
    pm.draw_pixmap(0, 0, sh.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

// ---------- Carte en verre ----------

pub struct CardStyle {
    pub radius: f32,
    pub cont: bool,
    pub shadow: f32,
    pub border: bool,
}

/// Fond d'un widget : ombre, fond d'écran flouté (si `bd`), teinte, bordure fine.
#[allow(clippy::too_many_arguments)]
pub fn card(pm: &mut Pixmap, m: f32, cw: f32, ch: f32, st: &CardStyle, t: &Theme, bd: Option<&Backdrop>, win: (i32, i32), s: f32) {
    let Some(path) = rrect(m, m, cw, ch, st.radius * s, st.cont) else { return };
    if st.shadow > 0.01 {
        shadow(pm, &path, 18.0 * s, 6.0 * s, 0.45 * st.shadow);
    }
    if let Some(b) = bd {
        let tr = Transform::from_scale(b.scale, b.scale).post_translate(-(win.0 as f32), -(win.1 as f32));
        let mut p = Paint::default();
        p.anti_alias = true;
        p.shader = Pattern::new(b.pm.as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, 1.0, tr);
        pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
    }
    fill(pm, &path, t.tint);
    if st.border {
        if let Some(bp) = rrect(m + 0.5, m + 0.5, cw - 1.0, ch - 1.0, (st.radius * s - 0.5).max(0.0), st.cont) {
            let stl = Stroke { width: 1.0, ..Default::default() };
            pm.stroke_path(&bp, &paint(t.border), &stl, Transform::identity(), None);
        }
    }
}

/// Contour pointillé + taille, affichés en mode « Modifier la disposition ».
#[allow(clippy::too_many_arguments)]
pub fn edit_overlay(pm: &mut Pixmap, fonts: &Fonts, m: f32, cw: f32, ch: f32, st: &CardStyle, t: &Theme, s: f32, scale: f32) {
    let g = 3.0 * s;
    if let Some(p) = rrect(m - g, m - g, cw + 2.0 * g, ch + 2.0 * g, st.radius * s + g, st.cont) {
        let stl = Stroke { width: 2.0 * s, dash: StrokeDash::new(vec![7.0 * s, 5.0 * s], 0.0), ..Default::default() };
        pm.stroke_path(&p, &paint(t.accent), &stl, Transform::identity(), None);
    }
    let label = format!("Taille {} %  ·  molette", (scale * 100.0).round() as i32);
    let tw = fonts.measure(Wt::Semibold, 12.0 * s, &label, 0.0) + 20.0 * s;
    let (px, py) = (m + (cw - tw) / 2.0, m + ch - 34.0 * s);
    if let Some(p) = rrect(px, py, tw, 24.0 * s, 12.0 * s, false) {
        fill(pm, &p, t.accent);
    }
    fonts.draw(pm, Wt::Semibold, 12.0 * s, px + 10.0 * s, py + 16.5 * s, WHITE, &label, 0.0, 0.8);
}

// ---------- Icônes météo ----------

#[derive(Clone, Copy, PartialEq)]
enum Ic {
    Clear,
    PartCloud,
    Cloud,
    Fog,
    Drizzle,
    Rain,
    Sleet,
    Snow,
    Thunder,
}

fn icon_kind(code: i32) -> Ic {
    match code {
        0 => Ic::Clear,
        1 | 2 => Ic::PartCloud,
        3 => Ic::Cloud,
        45 | 48 => Ic::Fog,
        51 | 53 | 55 => Ic::Drizzle,
        56 | 57 | 66 | 67 => Ic::Sleet,
        61 | 63 | 65 | 80 | 81 | 82 => Ic::Rain,
        71 | 73 | 75 | 77 | 85 | 86 => Ic::Snow,
        95 | 96 | 99 => Ic::Thunder,
        _ => Ic::Cloud,
    }
}

fn sun(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, c: Rgba) {
    circle(pm, cx, cy, r * 0.5, c);
    for k in 0..8 {
        let a = k as f32 * PI / 4.0;
        let (s, co) = a.sin_cos();
        line(pm, cx + co * r * 0.72, cy + s * r * 0.72, cx + co * r * 0.97, cy + s * r * 0.97, r * 0.14, c);
    }
}

fn moon(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, c: Rgba) {
    let d = (2.0 * r + 4.0).ceil();
    let Some(mut tmp) = Pixmap::new(d as u32, d as u32) else { return };
    let h = d / 2.0;
    circle(&mut tmp, h, h, r, c);
    if let Some(p) = PathBuilder::from_circle(h + r * 0.45, h - r * 0.32, r * 0.82) {
        let mut pt = paint(WHITE);
        pt.blend_mode = BlendMode::DestinationOut;
        tmp.fill_path(&p, &pt, FillRule::Winding, Transform::identity(), None);
    }
    pm.draw_pixmap((cx - h).round() as i32, (cy - h).round() as i32, tmp.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
}

fn cloud(pm: &mut Pixmap, bx: f32, by: f32, bw: f32, c: Rgba) {
    circle(pm, bx + 0.28 * bw, by + 0.42 * bw, 0.19 * bw, c);
    circle(pm, bx + 0.52 * bw, by + 0.32 * bw, 0.26 * bw, c);
    circle(pm, bx + 0.75 * bw, by + 0.45 * bw, 0.17 * bw, c);
    if let Some(p) = rrect(bx + 0.09 * bw, by + 0.40 * bw, 0.83 * bw, 0.22 * bw, 0.11 * bw, false) {
        fill(pm, &p, c);
    }
}

fn flake(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, c: Rgba) {
    for k in 0..3 {
        let a = k as f32 * PI / 3.0 + PI / 2.0;
        let (s, co) = a.sin_cos();
        line(pm, cx - co * r, cy - s * r, cx + co * r, cy + s * r, r * 0.4, c);
    }
}

/// Icône météo (code WMO) dans le carré (x, y, sz).
pub fn weather_icon(pm: &mut Pixmap, code: i32, day: bool, x: f32, y: f32, sz: f32, t: &Theme) {
    let k = icon_kind(code);
    let at = |fx: f32, fy: f32| (x + fx * sz, y + fy * sz);
    match k {
        Ic::Clear => {
            if day {
                sun(pm, x + sz * 0.5, y + sz * 0.5, sz * 0.46, t.sun)
            } else {
                moon(pm, x + sz * 0.5, y + sz * 0.5, sz * 0.36, t.moon)
            }
        }
        Ic::PartCloud => {
            if day {
                sun(pm, x + sz * 0.38, y + sz * 0.36, sz * 0.32, t.sun)
            } else {
                moon(pm, x + sz * 0.38, y + sz * 0.34, sz * 0.24, t.moon)
            }
            cloud(pm, x + sz * 0.20, y + sz * 0.30, sz * 0.78, t.cloud);
        }
        Ic::Cloud => cloud(pm, x + sz * 0.06, y + sz * 0.16, sz * 0.88, t.cloud),
        Ic::Fog => {
            for (i, (a, b)) in [(0.14, 0.80), (0.24, 0.90), (0.10, 0.70), (0.30, 0.86)].iter().enumerate() {
                let yy = y + sz * (0.30 + i as f32 * 0.14);
                line(pm, x + sz * a, yy, x + sz * b, yy, sz * 0.075, t.cloud);
            }
        }
        _ => {
            cloud(pm, x + sz * 0.08, y + sz * 0.02, sz * 0.84, t.cloud);
            match k {
                Ic::Rain => {
                    for fx in [0.32, 0.50, 0.68] {
                        let (a, b) = (at(fx + 0.04, 0.64), at(fx - 0.04, 0.88));
                        line(pm, a.0, a.1, b.0, b.1, sz * 0.07, t.rain);
                    }
                }
                Ic::Drizzle => {
                    for (fx, fy) in [(0.32, 0.70), (0.50, 0.82), (0.68, 0.70)] {
                        let p = at(fx, fy);
                        circle(pm, p.0, p.1, sz * 0.045, t.rain);
                    }
                }
                Ic::Snow => {
                    for (fx, fy) in [(0.30, 0.70), (0.50, 0.84), (0.70, 0.70)] {
                        let p = at(fx, fy);
                        flake(pm, p.0, p.1, sz * 0.075, t.snow);
                    }
                }
                Ic::Sleet => {
                    let (a, b) = at(0.40, 0.64);
                    let (c, d) = at(0.33, 0.88);
                    line(pm, a, b, c, d, sz * 0.07, t.rain);
                    let p = at(0.64, 0.76);
                    flake(pm, p.0, p.1, sz * 0.075, t.snow);
                }
                Ic::Thunder => {
                    let pts = [(0.54, 0.50), (0.36, 0.76), (0.50, 0.76), (0.42, 0.98), (0.66, 0.68), (0.52, 0.68), (0.60, 0.50)];
                    let mut pb = PathBuilder::new();
                    for (i, (fx, fy)) in pts.iter().enumerate() {
                        let p = at(*fx, *fy);
                        if i == 0 { pb.move_to(p.0, p.1) } else { pb.line_to(p.0, p.1) }
                    }
                    pb.close();
                    if let Some(p) = pb.finish() {
                        fill(pm, &p, t.sun);
                    }
                }
                _ => {}
            }
        }
    }
}

// ---------- Logo / icône de l'application ----------

pub fn draw_logo(pm: &mut Pixmap, s: f32) {
    if let Some(p) = rrect(0.5, 0.5, s - 1.0, s - 1.0, s * 0.2, true) {
        let sh = LinearGradient::new(
            Point::from_xy(0.0, 0.0),
            Point::from_xy(s * 0.4, s),
            vec![
                GradientStop::new(0.0, Rgba::rgb8(120, 222, 255).color()),
                GradientStop::new(1.0, Rgba::rgb8(36, 112, 232).color()),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        );
        let mut pt = Paint::default();
        pt.anti_alias = true;
        if let Some(sh) = sh {
            pt.shader = sh;
        }
        pm.fill_path(&p, &pt, FillRule::Winding, Transform::identity(), None);
    }
    let (c, r, w) = (s / 2.0, s * 0.31, (s * 0.075).max(1.4));
    for k in 0..3 {
        let a = k as f32 * PI / 3.0 + PI / 2.0;
        let (sn, co) = a.sin_cos();
        line(pm, c - co * r, c - sn * r, c + co * r, c + sn * r, w, WHITE);
        for end in [-1.0f32, 1.0] {
            let (ex, ey) = (c + end * co * r * 0.62, c + end * sn * r * 0.62);
            for b in [-1.0f32, 1.0] {
                let ba = a + b * PI / 4.0 + if end < 0.0 { PI } else { 0.0 };
                let (bs, bc) = ba.sin_cos();
                line(pm, ex, ey, ex + bc * r * 0.3, ey + bs * r * 0.3, w * 0.85, WHITE);
            }
        }
    }
}

// ---------- Envoi à Windows ----------

pub fn bmi32(w: i32, h: i32) -> BITMAPINFO {
    let mut b: BITMAPINFO = unsafe { std::mem::zeroed() };
    b.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    b.bmiHeader.biWidth = w;
    b.bmiHeader.biHeight = -h;
    b.bmiHeader.biPlanes = 1;
    b.bmiHeader.biBitCount = 32;
    b.bmiHeader.biCompression = BI_RGB;
    b
}

pub fn rgba_to_bgra(src: &[u8], dst: &mut [u8]) {
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        d[0] = s[2];
        d[1] = s[1];
        d[2] = s[0];
        d[3] = s[3];
    }
}

/// Affiche l'image (alpha prémultiplié) dans une fenêtre « layered », à la position (x, y).
pub unsafe fn present(hwnd: HWND, pm: &Pixmap, x: i32, y: i32) {
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    let screen = GetDC(null_mut());
    let mem = CreateCompatibleDC(screen);
    let bmi = bmi32(w, h);
    let mut bits: *mut core::ffi::c_void = null_mut();
    let bmp = CreateDIBSection(screen, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if !bmp.is_null() && !bits.is_null() {
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
        rgba_to_bgra(pm.data(), dst);
        let old = SelectObject(mem, bmp);
        let size = SIZE { cx: w, cy: h };
        let src = POINT { x: 0, y: 0 };
        let dstp = POINT { x, y };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        UpdateLayeredWindow(hwnd, screen, &dstp, &size, mem, &src, 0, &blend, ULW_ALPHA);
        SelectObject(mem, old);
        DeleteObject(bmp);
    }
    DeleteDC(mem);
    ReleaseDC(null_mut(), screen);
}

/// Icône Windows générée à partir du logo.
pub unsafe fn make_icon(size: i32) -> HICON {
    let Some(mut pm) = Pixmap::new(size as u32, size as u32) else { return null_mut() };
    draw_logo(&mut pm, size as f32);
    let bmi = bmi32(size, size);
    let mut bits: *mut core::ffi::c_void = null_mut();
    let hdc = GetDC(null_mut());
    let color = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    ReleaseDC(null_mut(), hdc);
    if color.is_null() || bits.is_null() {
        return null_mut();
    }
    let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (size * size * 4) as usize);
    for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
        let a = s[3] as u32;
        let un = |v: u8| if a == 0 { 0 } else { ((v as u32 * 255 + a / 2) / a).min(255) as u8 };
        d[0] = un(s[2]);
        d[1] = un(s[1]);
        d[2] = un(s[0]);
        d[3] = s[3];
    }
    let row = ((size + 15) / 16 * 2) as usize;
    let zeros = vec![0u8; row * size as usize];
    let mask = CreateBitmap(size, size, 1, 1, zeros.as_ptr() as _);
    let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
    let icon = CreateIconIndirect(&ii);
    DeleteObject(mask);
    DeleteObject(color);
    icon
}
