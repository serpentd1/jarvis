//! Disposition « Cartes » de la Dynamic Island (par défaut) : en-tête (date, météo, Jarvis, heure),
//! trois cartes (Jarvis, musique / météo, Focus) et une barre d'onglets en bas, comme un dock.

use chrono::Datelike;
use tiny_skia::{FillRule, FilterQuality, GradientStop, LinearGradient, Paint, Pattern, Pixmap, Point, SpreadMode, Transform};

use crate::data::wmo_label;
use crate::gfx::{self, Rgba, WHITE, Wt};
use crate::island::*;
use crate::island_draw::*;
use crate::widgets::{self, Action, Ctx, GREEN, JOURS_C, MOIS_C, ORANGE, RED};

/// Dégradés des cartes : (nom, haut, bas).
pub const GRAD_NAMES: [&str; 7] = ["Orange", "Violet", "Bleu", "Vert", "Rose", "Graphite", "Rouge"];
const GRADS: [([u8; 3], [u8; 3]); 7] = [
    ([62, 30, 18], [226, 92, 44]),
    ([44, 24, 78], [156, 108, 226]),
    ([14, 34, 74], [52, 136, 236]),
    ([12, 48, 30], [62, 184, 104]),
    ([78, 20, 56], [236, 94, 160]),
    ([30, 30, 34], [96, 98, 110]),
    ([70, 12, 16], [226, 56, 56]),
];

const TOP: f32 = 40.0;
const PAD: f32 = 12.0;
const CARD_H: f32 = 222.0;

/// Position de la souris en coordonnées logiques du contenu (pour le survol).
fn mouse(c: &Ctx, r: &R) -> (f32, f32) {
    let x = (r.mouse.0 - r.st.win.0) as f32;
    let y = (r.mouse.1 - r.st.win.1) as f32;
    ((x - c.ox) / c.s, (y - c.oy) / c.s)
}

/// Rectangle arrondi avec un dégradé vertical.
fn vgrad(c: &mut Ctx, x: f32, y: f32, w: f32, h: f32, rad: f32, top: Rgba, bot: Rgba) {
    let (px, py) = (c.ox + x * c.s, c.oy + y * c.s);
    let Some(path) = gfx::rrect(px, py, w * c.s, h * c.s, rad * c.s, true) else { return };
    let Some(sh) = LinearGradient::new(
        Point::from_xy(0.0, py),
        Point::from_xy(0.0, py + h * c.s),
        vec![GradientStop::new(0.0, top.color()), GradientStop::new(1.0, bot.color())],
        SpreadMode::Pad,
        Transform::identity(),
    ) else {
        return;
    };
    let mut p = Paint::default();
    p.anti_alias = true;
    p.shader = sh;
    c.pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
}

/// Image qui remplit le rectangle (recadrée, sans déformation).
fn cover(c: &mut Ctx, img: &Pixmap, x: f32, y: f32, w: f32, h: f32, rad: f32) {
    let (px, py, pw, ph) = (c.ox + x * c.s, c.oy + y * c.s, w * c.s, h * c.s);
    let Some(path) = gfx::rrect(px, py, pw, ph, rad * c.s, true) else { return };
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let k = (pw / iw).max(ph / ih);
    let tr = Transform::from_scale(k, k).post_translate(px + (pw - iw * k) / 2.0, py + (ph - ih * k) / 2.0);
    let mut p = Paint::default();
    p.anti_alias = true;
    p.shader = Pattern::new(img.as_ref(), SpreadMode::Pad, FilterQuality::Bicubic, 1.0, tr);
    c.pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
}

/// Petites capitales espacées (titres de cartes).
fn caps(c: &mut Ctx, x: f32, y: f32, col: Rgba, s: &str) -> f32 {
    c.text_t(Wt::Semibold, 11.0, x, y, col, &s.to_uppercase(), 1.6)
}

// ---------- Icônes (fines, dessinées) ----------

pub fn icon(c: &mut Ctx, t: Tab, cx: f32, cy: f32, col: Rgba) {
    let lw = 1.6;
    match t {
        Tab::Home => {
            c.line(cx - 7.0, cy - 0.5, cx, cy - 7.0, lw, col);
            c.line(cx, cy - 7.0, cx + 7.0, cy - 0.5, lw, col);
            c.line(cx - 5.0, cy - 2.0, cx - 5.0, cy + 6.5, lw, col);
            c.line(cx + 5.0, cy - 2.0, cx + 5.0, cy + 6.5, lw, col);
            c.line(cx - 5.0, cy + 6.5, cx + 5.0, cy + 6.5, lw, col);
            c.line(cx - 1.5, cy + 6.5, cx - 1.5, cy + 2.0, lw, col);
            c.line(cx + 1.5, cy + 6.5, cx + 1.5, cy + 2.0, lw, col);
            c.line(cx - 1.5, cy + 2.0, cx + 1.5, cy + 2.0, lw, col);
        }
        Tab::Jarvis => {
            let p = |k: f32| [(cx, cy - 8.0 * k), (cx + 2.0 * k, cy - 2.0 * k), (cx + 8.0 * k, cy), (cx + 2.0 * k, cy + 2.0 * k), (cx, cy + 8.0 * k), (cx - 2.0 * k, cy + 2.0 * k), (cx - 8.0 * k, cy), (cx - 2.0 * k, cy - 2.0 * k)];
            c.poly(&p(1.0), col);
            c.circle(cx + 7.0, cy - 6.0, 1.4, col);
        }
        Tab::Music => {
            c.circle(cx - 4.0, cy + 5.0, 3.0, col);
            c.line(cx - 1.2, cy + 5.0, cx - 1.2, cy - 7.0, lw, col);
            c.line(cx - 1.2, cy - 7.0, cx + 6.0, cy - 4.5, lw + 0.4, col);
        }
        Tab::Discord => {
            c.rr_stroke(cx - 9.0, cy - 5.5, 18.0, 11.0, 5.5, col, lw);
            c.line(cx - 6.0, cy, cx - 2.0, cy, lw, col);
            c.line(cx - 4.0, cy - 2.0, cx - 4.0, cy + 2.0, lw, col);
            c.circle(cx + 3.0, cy + 1.0, 1.2, col);
            c.circle(cx + 5.5, cy - 1.5, 1.2, col);
        }
        Tab::Timers => {
            c.ring(cx, cy + 1.0, 6.5, col, lw);
            c.line(cx, cy + 1.0, cx, cy - 3.0, lw, col);
            c.line(cx, cy + 1.0, cx + 2.5, cy + 2.5, lw, col);
            c.line(cx - 2.0, cy - 8.0, cx + 2.0, cy - 8.0, lw, col);
            c.line(cx + 5.5, cy - 5.0, cx + 7.0, cy - 6.5, lw, col);
        }
        Tab::Dnd => {
            let mut pts = Vec::new();
            for i in 0..=16 {
                let a = -2.2 + i as f32 / 16.0 * 4.4;
                pts.push((cx + a.cos() * 7.5, cy + a.sin() * 7.5));
            }
            for i in (0..=16).rev() {
                let a = -2.0 + i as f32 / 16.0 * 4.0;
                pts.push((cx - 3.0 + a.cos() * 5.6, cy - 1.5 + a.sin() * 5.6));
            }
            let pts: Vec<(f32, f32)> = pts.into_iter().map(|(x, y)| (2.0 * cx - x, y)).collect();
            c.poly(&pts, col);
        }
        Tab::Notifs => {
            c.line(cx - 8.0, cy, cx - 8.0, cy + 6.0, lw, col);
            c.line(cx + 8.0, cy, cx + 8.0, cy + 6.0, lw, col);
            c.line(cx - 8.0, cy + 6.0, cx + 8.0, cy + 6.0, lw, col);
            c.line(cx - 8.0, cy, cx - 3.5, cy, lw, col);
            c.line(cx + 8.0, cy, cx + 3.5, cy, lw, col);
            c.line(cx - 3.5, cy, cx - 2.0, cy + 2.5, lw, col);
            c.line(cx + 3.5, cy, cx + 2.0, cy + 2.5, lw, col);
            c.line(cx - 2.0, cy + 2.5, cx + 2.0, cy + 2.5, lw, col);
            c.line(cx - 6.0, cy, cx - 4.0, cy - 7.0, lw, col);
            c.line(cx + 6.0, cy, cx + 4.0, cy - 7.0, lw, col);
            c.line(cx - 4.0, cy - 7.0, cx + 4.0, cy - 7.0, lw, col);
        }
        Tab::Clips => {
            c.rr_stroke(cx - 7.0, cy - 4.0, 10.0, 12.0, 2.0, col, lw);
            c.line(cx - 3.0, cy - 7.5, cx + 6.5, cy - 7.5, lw, col);
            c.line(cx + 6.5, cy - 7.5, cx + 6.5, cy + 4.0, lw, col);
        }
        Tab::School => {
            c.rr_stroke(cx - 6.5, cy - 8.0, 13.0, 16.0, 2.0, col, lw);
            for k in 0..3 {
                let y = cy - 3.5 + k as f32 * 3.5;
                c.line(cx - 3.5, y, cx + (if k == 2 { 1.0 } else { 3.5 }), y, lw * 0.9, col);
            }
        }
        Tab::Calendar => {
            c.rr_stroke(cx - 8.0, cy - 6.0, 16.0, 14.0, 2.5, col, lw);
            c.line(cx - 8.0, cy - 2.0, cx + 8.0, cy - 2.0, lw, col);
            c.line(cx - 4.0, cy - 8.5, cx - 4.0, cy - 4.5, lw, col);
            c.line(cx + 4.0, cy - 8.5, cx + 4.0, cy - 4.5, lw, col);
            c.circle(cx - 3.0, cy + 3.0, 1.1, col);
            c.circle(cx + 1.0, cy + 3.0, 1.1, col);
        }
        Tab::Mails => {
            c.rr_stroke(cx - 8.5, cy - 6.0, 17.0, 12.0, 2.0, col, lw);
            c.line(cx - 7.5, cy - 4.5, cx, cy + 1.0, lw, col);
            c.line(cx + 7.5, cy - 4.5, cx, cy + 1.0, lw, col);
        }
    }
}

fn plus(c: &mut Ctx, cx: f32, cy: f32, col: Rgba) {
    c.line(cx - 6.0, cy, cx + 6.0, cy, 1.7, col);
    c.line(cx, cy - 6.0, cx, cy + 6.0, 1.7, col);
}

fn speaker(c: &mut Ctx, cx: f32, cy: f32, col: Rgba) {
    c.poly(&[(cx - 8.0, cy - 2.5), (cx - 5.0, cy - 2.5), (cx - 1.0, cy - 6.5), (cx - 1.0, cy + 6.5), (cx - 5.0, cy + 2.5), (cx - 8.0, cy + 2.5)], col);
    for (k, rad) in [4.0f32, 7.5].iter().enumerate() {
        let n = 6;
        for i in 0..n {
            let a0 = -0.8 + i as f32 / n as f32 * 1.6;
            let a1 = -0.8 + (i + 1) as f32 / n as f32 * 1.6;
            c.line(cx + a0.cos() * rad, cy + a0.sin() * rad, cx + a1.cos() * rad, cy + a1.sin() * rad, 1.5 - k as f32 * 0.1, col);
        }
    }
}

fn skip(c: &mut Ctx, cx: f32, cy: f32, next: bool, col: Rgba) {
    let d = if next { 1.0 } else { -1.0 };
    c.poly(&[(cx - 4.0 * d, cy - 5.0), (cx + 3.0 * d, cy), (cx - 4.0 * d, cy + 5.0)], col);
    c.line(cx + 4.5 * d, cy - 5.0, cx + 4.5 * d, cy + 5.0, 1.8, col);
}

// ---------- Vue « Cartes » ----------

pub fn cards_view(c: &mut Ctx, r: &R, w: f32, h: f32) {
    header(c, r, w);
    let oy = c.oy;
    c.oy += TOP * c.s;
    let view = r.st.view;
    match view {
        View::Main => home(c, r, w),
        _ => page(c, r, view, w),
    }
    c.oy = oy;
    tab_bar(c, r, w, h);
}

fn header(c: &mut Ctx, r: &R, w: f32) {
    let t = c.t;
    let cf = r.c();
    let now = r.now;
    let mut left = format!("{} {:02} {}", JOURS_C[now.weekday().num_days_from_monday() as usize].trim_end_matches('.'), now.day(), MOIS_C[now.month0() as usize].trim_end_matches('.'));
    if let Some(wt) = &r.d.weather {
        left = format!("{left}  /  {}", temp_txt(cf, wt.temp));
    }
    caps(c, 22.0, 26.0, t.fg2, &left);
    // les deux points du milieu = indicateur Jarvis
    let active = cf.jarvis_show && r.jarvis.online() && r.jarvis.state != "repos";
    let col = if active { jarvis_color(&r.jarvis.state) } else { WHITE.a(if r.jarvis.online() { 0.9 } else { 0.35 }) };
    let k = if active { 0.5 + 0.5 * (r.t() * 5.0).sin() } else { 0.0 };
    for dx in [-5.0, 5.0] {
        if active {
            c.circle(w / 2.0 + dx, 21.0, 3.0 + 2.5 * k, col.a(0.25));
        }
        c.circle(w / 2.0 + dx, 21.0, 2.6, col);
    }
    c.hit(w / 2.0 - 24.0, 4.0, 48.0, 34.0, Action::Ile(A_JARVIS_LISTEN));
    // à droite : heure + menu
    let (tm, suf) = time_txt(cf, now);
    let tx = w - 50.0;
    let sw = if suf.is_empty() { 0.0 } else { c.text_r(Wt::Semibold, 11.0, tx, 26.0, t.fg2, &suf) + 4.0 };
    c.text_r(Wt::Semibold, 15.0, tx - sw, 26.5, t.fg, &tm);
    for i in 0..3 {
        c.circle(w - 32.0 + i as f32 * 5.5, 21.5, 1.6, t.fg2);
    }
    c.hit(w - 44.0, 4.0, 40.0, 34.0, Action::Ile(A_MENU));
}

fn home(c: &mut Ctx, r: &R, w: f32) {
    let cf = r.c();
    let inner = w - 2.0 * PAD;
    let narrow = (inner * 0.215).round();
    let mid = inner - 2.0 * narrow - 16.0;
    let y = 4.0;
    let x0 = PAD;
    let x1 = x0 + narrow + 8.0;
    let x2 = x1 + mid + 8.0;
    left_card(c, r, x0, y, narrow, CARD_H);
    if r.d.media.as_ref().is_some_and(|m| !m.title.is_empty()) {
        media_card(c, r, x1, y, mid, CARD_H);
    } else {
        weather_card(c, r, x1, y, mid, CARD_H);
    }
    focus_card(c, r, x2, y, narrow, CARD_H, cf.right_grad);
}

fn grad(i: usize) -> (Rgba, Rgba) {
    let (a, b) = GRADS[i.min(GRADS.len() - 1)];
    (Rgba::from(a), Rgba::from(b))
}

fn left_card(c: &mut Ctx, r: &R, x: f32, y: f32, w: f32, h: f32) {
    let cf = r.c();
    let (a, b) = grad(cf.left_grad);
    vgrad(c, x, y, w, h, 16.0, a, b);
    let (title, big, sub, act): (&str, String, String, Action) = match cf.left_card {
        1 => {
            let n = homework_list(r.d, cf, r.now).iter().filter(|i| !r.d.pronote.homework[**i].done).count();
            ("Devoirs", n.to_string(), if n > 1 { "devoirs à faire".into() } else { "devoir à faire".into() }, Action::IleN(N_TAB, Tab::School as u16))
        }
        2 => {
            let n = important_mails(r.d, cf).len();
            ("Mails", n.to_string(), if n > 1 { "mails importants".into() } else { "mail important".into() }, Action::IleN(N_TAB, Tab::Mails as u16))
        }
        3 => match &r.d.weather {
            Some(wt) => ("Météo", temp_txt(cf, wt.temp), wmo_label(wt.code, wt.day).to_lowercase(), Action::Refresh),
            None => ("Météo", "–".into(), "chargement…".into(), Action::Refresh),
        },
        _ => {
            let sub = if r.jarvis.online() && r.jarvis.state != "repos" {
                match r.jarvis.state.as_str() {
                    "ecoute" => "t'écoute…",
                    "reflechit" => "réfléchit…",
                    _ => "parle…",
                }
                .to_string()
            } else if r.jarvis.online() {
                if r.jarvis_cmds > 1 { "commandes aujourd'hui".into() } else { "commande aujourd'hui".into() }
            } else {
                "hors ligne".into()
            };
            ("Jarvis", r.jarvis_cmds.to_string(), sub, Action::Ile(A_JARVIS_LISTEN))
        }
    };
    caps(c, x + 14.0, y + 26.0, WHITE.a(0.92), title);
    if cf.left_card == 0 && r.jarvis.online() && r.jarvis.state != "repos" {
        jarvis_wave(c, x + w - 20.0, y + 22.0, &r.jarvis.state, r.t());
    }
    let big = c.fit(Wt::Light, 64.0, &big, w - 24.0);
    c.text(Wt::Light, 64.0, x + 12.0, y + h - 40.0, WHITE, &big);
    let sub = c.fit(Wt::Regular, 12.5, &sub, w - 26.0);
    c.text(Wt::Regular, 12.5, x + 14.0, y + h - 18.0, WHITE.a(0.85), &sub);
    c.hit(x, y, w, h, act);
}

fn media_card(c: &mut Ctx, r: &R, x: f32, y: f32, w: f32, h: f32) {
    let Some(m) = &r.d.media else { return };
    match &m.art {
        Some(art) => {
            if r.c().art_blur {
                let mut b = art.clone();
                let rad = (b.width().max(b.height()) / 24).max(2) as usize;
                let (bw, bh) = (b.width() as usize, b.height() as usize);
                gfx::box_blur(b.data_mut(), bw, bh, rad, 2);
                cover(c, &b, x, y, w, h, 16.0);
            } else {
                cover(c, art, x, y, w, h, 16.0);
            }
        }
        None => vgrad(c, x, y, w, h, 16.0, Rgba::rgb8(40, 44, 60), Rgba::rgb8(90, 70, 140)),
    }
    vgrad(c, x, y + h * 0.35, w, h * 0.65, 16.0, Rgba(0.0, 0.0, 0.0, 0.0), Rgba(0.0, 0.0, 0.0, 0.62));
    let app = if m.app.is_empty() { "En cours".to_string() } else { format!("En cours · {}", m.app) };
    let app = c.fit(Wt::Semibold, 11.0, &app.to_uppercase(), w - 30.0);
    c.text_t(Wt::Semibold, 11.0, x + 16.0, y + 26.0, WHITE.a(0.85), &app, 1.2);
    let tw = w - 90.0;
    let title = c.fit(Wt::Semibold, 21.0, &m.title, tw);
    c.text(Wt::Semibold, 21.0, x + 16.0, y + h - 36.0, WHITE, &title);
    let artist = c.fit(Wt::Regular, 13.5, &m.artist, tw);
    c.text(Wt::Regular, 13.5, x + 16.0, y + h - 16.0, WHITE.a(0.8), &artist);
    let (bx, by) = (x + w - 34.0, y + h - 34.0);
    c.circle(bx, by, 20.0, WHITE.a(0.24));
    play_icon(c, bx + if m.playing { 0.0 } else { 1.5 }, by, m.playing, WHITE);
    c.hit(bx - 22.0, by - 22.0, 44.0, 44.0, Action::Ile(A_PLAY));
    // précédent / suivant au survol de la carte
    let (mx, my) = mouse(c, r);
    if mx >= x && mx < x + w && my >= y && my < y + h {
        for (dx, next) in [(-46.0, true), (-82.0, false)] {
            let cx = bx + dx;
            c.circle(cx, by, 15.0, WHITE.a(0.18));
            skip(c, cx, by, next, WHITE);
            c.hit(cx - 16.0, by - 16.0, 32.0, 32.0, Action::Ile(if next { A_NEXT } else { A_PREV }));
        }
    }
    c.hit(x, y, w, h, Action::IleN(N_TAB, Tab::Music as u16));
}

/// Pas de musique : une petite scène météo dessinée (ciel, soleil ou lune, nuages, pluie, dunes).
fn weather_card(c: &mut Ctx, r: &R, x: f32, y: f32, w: f32, h: f32) {
    let cf = r.c();
    let Some(wt) = &r.d.weather else {
        vgrad(c, x, y, w, h, 16.0, Rgba::rgb8(40, 44, 60), Rgba::rgb8(70, 74, 96));
        caps(c, x + 16.0, y + 26.0, WHITE.a(0.85), "Météo");
        c.text(Wt::Regular, 13.0, x + 16.0, y + h - 20.0, WHITE.a(0.8), "Chargement de la météo…");
        return;
    };
    let cat = wcat(wt.code, wt.day);
    let (a, b) = wcolors(cat);
    let sky_top = a.mix(Rgba(0.0, 0.0, 0.0, 1.0), 0.15);
    vgrad(c, x, y, w, h, 16.0, sky_top, b);
    let t = r.t();
    // astre
    let (sx, sy) = (x + w * 0.74, y + h * 0.3);
    if wt.day && cat != 3 && cat != 4 && cat != 7 {
        for i in (1..5).rev() {
            c.circle(sx, sy, 26.0 + i as f32 * 7.0, Rgba::rgb8(255, 236, 170).a(0.06));
        }
        c.circle(sx, sy, 26.0, Rgba::rgb8(255, 240, 190));
    } else if !wt.day {
        c.circle(sx, sy, 22.0, Rgba::rgb8(240, 240, 255));
        c.circle(sx + 9.0, sy - 6.0, 19.0, sky_top);
        for i in 0..7 {
            let k = (t * 1.5 + i as f32).sin() * 0.5 + 0.5;
            c.circle(x + 20.0 + (i * 53 % (w as i32 - 40)) as f32, y + 18.0 + (i * 29 % 60) as f32, 1.0 + k, WHITE.a(0.4 + 0.5 * k));
        }
    }
    // nuages
    if cat >= 2 {
        let drift = if cf.weather_anim { (t * 6.0).rem_euclid(40.0) } else { 0.0 };
        let cloud_col = if cat == 7 { Rgba::rgb8(70, 66, 96) } else { WHITE.mix(sky_top, 0.12) };
        for (k, (cx, cy, s)) in [(0.28f32, 0.32f32, 1.0f32), (0.58, 0.22, 0.8)].iter().enumerate() {
            let bx = x + w * cx + drift * (1.0 - k as f32 * 0.5) - 20.0;
            let by = y + h * cy;
            c.circle(bx, by, 18.0 * s, cloud_col);
            c.circle(bx + 20.0 * s, by - 9.0 * s, 22.0 * s, cloud_col);
            c.circle(bx + 42.0 * s, by, 17.0 * s, cloud_col);
            c.rr(bx - 2.0, by - 2.0, 46.0 * s, 20.0 * s, 9.0 * s, cloud_col);
        }
    }
    // pluie / neige
    if cat == 5 || cat == 7 || cat == 6 {
        for i in 0..24 {
            let off = (t * if cat == 6 { 18.0 } else { 70.0 } + i as f32 * 17.0) % (h * 0.6);
            let dx = x + 12.0 + (i * 37 % (w as i32 - 24)) as f32;
            let dy = y + h * 0.36 + off;
            if cat == 6 {
                c.circle(dx, dy, 2.0, WHITE.a(0.85));
            } else {
                c.line(dx, dy, dx - 2.0, dy + 7.0, 1.4, Rgba::rgb8(190, 220, 255).a(0.8));
            }
        }
    }
    // dunes
    let base = y + h;
    let d1 = b.mix(Rgba(0.0, 0.0, 0.0, 1.0), 0.25);
    let d2 = b.mix(Rgba(0.0, 0.0, 0.0, 1.0), 0.45);
    let mut p1 = vec![(x, base)];
    let mut p2 = vec![(x, base)];
    for i in 0..=24 {
        let u = i as f32 / 24.0;
        p1.push((x + u * w, base - h * 0.30 - (u * 5.0).sin() * 14.0 - u * 10.0));
        p2.push((x + u * w, base - h * 0.16 - (u * 3.5 + 1.0).sin() * 12.0));
    }
    p1.push((x + w, base));
    p2.push((x + w, base));
    c.poly(&p1, d1.a(0.85));
    c.poly(&p2, d2.a(0.95));
    vgrad(c, x, y + h * 0.5, w, h * 0.5, 16.0, Rgba(0.0, 0.0, 0.0, 0.0), Rgba(0.0, 0.0, 0.0, 0.35));
    let head = format!("Météo · {}", r.cfg.weather.city);
    let head = c.fit(Wt::Semibold, 11.0, &head.to_uppercase(), w - 30.0);
    c.text_t(Wt::Semibold, 11.0, x + 16.0, y + 26.0, WHITE.a(0.9), &head, 1.2);
    c.text(Wt::Light, 46.0, x + 14.0, y + h - 46.0, WHITE, &temp_txt(cf, wt.temp));
    let phrase = if cf.weather_phrase { weather_phrase(cf, wt.code, wt.day, wt.temp, r.now.ordinal()) } else { wmo_label(wt.code, wt.day).to_string() };
    let phrase = c.fit(Wt::Semibold, 13.5, &phrase, w - 30.0);
    c.text(Wt::Semibold, 13.5, x + 16.0, y + h - 18.0, WHITE, &phrase);
    c.hit(x, y, w, h, Action::Refresh);
}

fn focus_card(c: &mut Ctx, r: &R, x: f32, y: f32, w: f32, h: f32, g: usize) {
    let cf = r.c();
    let (a, b) = grad(g);
    vgrad(c, x, y, w, h, 16.0, a, b);
    caps(c, x + 14.0, y + 26.0, WHITE.a(0.92), "Focus");
    let k = &r.clocks;
    let rem = k.pomo_remaining(cf.pomo_work);
    let s = rem.max(0.0).ceil() as i64;
    let txt = format!("{:02}:{:02}", s / 60, s % 60);
    let tw = c.fit(Wt::Light, 40.0, &txt, w - 20.0);
    c.text(Wt::Light, 40.0, x + 12.0, y + h - 44.0, WHITE, &tw);
    let state = match (k.pomo_phase, k.pomo_end.is_some()) {
        (0, _) => "Prêt",
        (1, true) => "Concentration",
        (2, true) => "Pause",
        _ => "En pause",
    };
    c.text(Wt::Regular, 12.5, x + 14.0, y + h - 20.0, WHITE.a(0.85), state);
    let (bx, by) = (x + w - 26.0, y + h - 26.0);
    c.circle(bx, by, 16.0, WHITE);
    play_icon(c, bx + if k.pomo_end.is_some() { 0.0 } else { 1.5 }, by, k.pomo_end.is_some(), Rgba::rgb8(60, 40, 100));
    c.hit(bx - 18.0, by - 18.0, 36.0, 36.0, Action::Ile(A_POMO));
    if k.pomo_phase != 0 {
        let (rx, ry) = (x + w - 20.0, y + 20.0);
        c.ring(rx, ry, 6.0, WHITE.a(0.8), 1.5);
        c.line(rx + 6.0, ry - 3.0, rx + 6.0, ry + 1.0, 1.5, WHITE.a(0.8));
        c.hit(rx - 14.0, ry - 14.0, 28.0, 28.0, Action::Ile(A_POMO_RESET));
    }
    let _ = (GREEN, ORANGE);
}

fn tab_bar(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let y = h - 50.0;
    let bh = 40.0;
    c.rr(PAD, y, w - 2.0 * PAD, bh, 20.0, t.fg.a(0.05));
    let cy = y + bh / 2.0;
    plus(c, PAD + 22.0, cy, t.fg2);
    c.hit(PAD, y, 44.0, bh, Action::Ile(A_TABS_SETTINGS));
    let cur = Tab::of(r.st.view);
    let mut x = PAD + 46.0;
    for tab in tabs(r.c()) {
        if tab == cur {
            let lab = tab.label();
            let lw = c.measure(Wt::Semibold, 11.0, lab) + lab.chars().count() as f32 * 1.4;
            let pw = 30.0 + lw + 16.0;
            c.rr(x, y + 5.0, pw, bh - 10.0, 15.0, t.fg.a(0.13));
            icon(c, tab, x + 17.0, cy, t.fg);
            c.text_t(Wt::Semibold, 11.0, x + 32.0, cy + 4.0, t.fg, lab, 1.4);
            c.hit(x, y, pw, bh, Action::IleN(N_TAB, tab as u16));
            x += pw + 4.0;
        } else {
            icon(c, tab, x + 17.0, cy, t.fg2);
            c.hit(x, y, 34.0, bh, Action::IleN(N_TAB, tab as u16));
            x += 34.0;
        }
        if x > w - 80.0 {
            break;
        }
    }
    let sx = w - PAD - 24.0;
    speaker(c, sx, cy, t.fg2);
    c.hit(sx - 18.0, y, 36.0, bh, Action::Ile(A_VOLUME));
}

// ---------- Pages des onglets ----------

fn page(c: &mut Ctx, r: &R, view: View, w: f32) {
    // les pages « détail » d'origine dessinent un bouton en haut à droite : on décale un peu
    let detail = |c: &mut Ctx, f: fn(&mut Ctx, &R, f32, f32)| {
        let oy = c.oy;
        c.oy += 14.0 * c.s;
        f(c, r, w, BODY - 18.0);
        c.oy = oy;
    };
    match view {
        View::Chat => detail(c, chat_page),
        View::Discord => detail(c, discord_page),
        View::Notifs => detail(c, notifs_page),
        View::Mails => detail(c, mails_page),
        View::Homework => detail(c, homework_page),
        View::Grades => detail(c, grades_page),
        View::Music => music_page(c, r, w),
        View::Timers => {
            time_col(c, r, SIDE, 190.0);
            timers_col(c, r, SIDE + 210.0, 205.0);
            focus_card(c, r, SIDE + 435.0, -2.0, w - SIDE - (SIDE + 435.0), BODY - 12.0, r.c().right_grad);
        }
        View::Dnd => dnd_page(c, r, w),
        View::Clips => clips_page(c, r, w),
        View::School => {
            pronote_col(c, r, SIDE, 280.0);
            school_hw(c, r, SIDE + 300.0, w - SIDE - (SIDE + 300.0));
        }
        View::Calendar => calendar_page(c, r, w),
        View::Main => {}
    }
}

fn music_page(c: &mut Ctx, r: &R, w: f32) {
    let t = c.t;
    let Some(m) = r.d.media.as_ref().filter(|m| !m.title.is_empty()) else {
        c.text(Wt::Semibold, 16.0, SIDE, 40.0, t.fg, "Aucune musique en cours");
        c.text(Wt::Regular, 13.0, SIDE, 62.0, t.fg2, "Lance Spotify, YouTube ou une autre appli : elle s'affichera ici.");
        volume_row(c, SIDE, 120.0);
        return;
    };
    let sz = 196.0;
    match &m.art {
        Some(a) => cover(c, a, SIDE, 4.0, sz, sz, 14.0),
        None => vgrad(c, SIDE, 4.0, sz, sz, 14.0, Rgba::rgb8(40, 44, 60), Rgba::rgb8(90, 70, 140)),
    }
    let x = SIDE + sz + 24.0;
    let tw = w - x - SIDE;
    caps(c, x, 26.0, t.fg2, if m.app.is_empty() { "En cours" } else { &m.app });
    let title = c.fit(Wt::Semibold, 24.0, &m.title, tw);
    c.text(Wt::Semibold, 24.0, x, 62.0, t.fg, &title);
    let artist = c.fit(Wt::Regular, 15.0, &m.artist, tw);
    c.text(Wt::Regular, 15.0, x, 86.0, t.fg2, &artist);
    let cy = 130.0;
    for (i, a) in [A_PREV, A_PLAY, A_NEXT].iter().enumerate() {
        let cx = x + 22.0 + i as f32 * 56.0;
        let big = *a == A_PLAY;
        c.circle(cx, cy, if big { 22.0 } else { 17.0 }, if big { t.fg } else { t.fg.a(0.12) });
        match *a {
            A_PLAY => play_icon(c, cx + if m.playing { 0.0 } else { 1.5 }, cy, m.playing, Rgba::rgb8(20, 20, 24)),
            A_PREV => skip(c, cx, cy, false, t.fg),
            _ => skip(c, cx, cy, true, t.fg),
        }
        c.hit(cx - 24.0, cy - 24.0, 48.0, 48.0, Action::Ile(*a));
    }
    volume_row(c, x, 180.0);
}

fn volume_row(c: &mut Ctx, x: f32, y: f32) {
    let t = c.t;
    speaker(c, x + 10.0, y + 12.0, t.fg2);
    let mut cx = x + 30.0;
    for (lab, a) in [("−", A_VOL_DOWN), ("Couper", A_VOLUME), ("+", A_VOL_UP)] {
        cx += chip(c, cx, y, lab, t.fg.a(0.12), t.fg, Some(Action::Ile(a))) + 6.0;
    }
    c.text(Wt::Regular, 11.5, cx + 6.0, y + 16.5, t.fg3, "(molette sur le haut-parleur en bas à droite)");
}

fn dnd_page(c: &mut Ctx, r: &R, w: f32) {
    let t = c.t;
    let on = r.c().dnd;
    icon(c, Tab::Dnd, SIDE + 18.0, 46.0, if on { Rgba::rgb8(175, 140, 255) } else { t.fg2 });
    c.text(Wt::Semibold, 20.0, SIDE + 44.0, 54.0, t.fg, "Ne pas déranger");
    let (sx, sy) = (w - SIDE - 60.0, 36.0);
    c.rr(sx, sy, 52.0, 28.0, 14.0, if on { Rgba::rgb8(140, 100, 255) } else { t.track });
    c.circle(if on { sx + 38.0 } else { sx + 14.0 }, sy + 14.0, 11.5, WHITE);
    c.hit(SIDE, 20.0, w - 2.0 * SIDE, 50.0, Action::Ile(A_DND));
    let txt = if on {
        "Activé : plus aucune notification dans l'île ni dans Windows. Les alarmes et les minuteurs sonnent quand même."
    } else {
        "Désactivé : les notifications s'affichent selon tes réglages, source par source."
    };
    for (i, l) in c.wrap(Wt::Regular, 13.5, txt, w - 2.0 * SIDE).iter().take(3).enumerate() {
        c.text(Wt::Regular, 13.5, SIDE, 100.0 + i as f32 * 20.0, t.fg2, l);
    }
    chip(c, SIDE, 170.0, "Réglages des notifications", t.fg.a(0.12), t.fg, Some(Action::Ile(A_NOTIF_SETTINGS)));
}

fn clips_page(c: &mut Ctx, r: &R, w: f32) {
    let t = c.t;
    caps(c, SIDE, 22.0, t.fg2, "Presse-papiers");
    c.text_r(Wt::Regular, 11.5, w - SIDE, 22.0, t.fg3, "clic : recopier · gardé en mémoire seulement");
    if r.clips.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 56.0, t.fg2, "Rien copié depuis le lancement de Givre.");
        return;
    }
    let colw = (w - 2.0 * SIDE - 12.0) / 2.0;
    for (i, s) in r.clips.iter().take(10).enumerate() {
        let (cx, cy) = (SIDE + (i % 2) as f32 * (colw + 12.0), 34.0 + (i / 2) as f32 * 38.0);
        c.rr(cx, cy, colw, 32.0, 10.0, t.fg.a(0.06));
        let one = s.replace(['\r', '\n', '\t'], " ");
        let txt = c.fit(Wt::Regular, 12.5, &one, colw - 20.0);
        c.text(Wt::Regular, 12.5, cx + 10.0, cy + 20.5, t.fg, &txt);
        c.hit(cx, cy, colw, 32.0, Action::IleN(N_CLIP, i as u16));
    }
}

fn school_hw(c: &mut Ctx, r: &R, x: f32, w: f32) {
    let t = c.t;
    caps(c, x, 18.0, t.fg2, "Devoirs en détail");
    let list = homework_list(r.d, r.c(), r.now);
    if list.is_empty() {
        c.text(Wt::Regular, 13.0, x, 48.0, t.fg2, "Rien à faire, profite !");
        return;
    }
    let mut y = 30.0;
    for &i in list.iter().skip(r.st.scroll) {
        let h = &r.d.pronote.homework[i];
        let lines = c.wrap(Wt::Regular, 12.0, &h.text, w - 30.0);
        let n = lines.len().min(2);
        let rh = 24.0 + n as f32 * 15.0;
        if y + rh > BODY - 14.0 {
            break;
        }
        let (bx, by) = (x + 8.0, y + 12.0);
        if h.done {
            c.circle(bx, by, 7.5, GREEN);
        } else {
            c.ring(bx, by, 6.5, t.fg2, 1.4);
        }
        c.hit(x, y, 20.0, rh, Action::IleN(N_HW_DONE, i as u16));
        let due = widgets::day_label(h.due, r.now.date());
        let dw = c.text_r(Wt::Semibold, 11.0, x + w, y + 16.0, ORANGE, &due);
        let sub = c.fit(Wt::Semibold, 13.0, &h.subject, w - 30.0 - dw);
        c.text(Wt::Semibold, 13.0, x + 22.0, y + 16.0, if h.done { t.fg3 } else { t.fg }, &sub);
        for (k, l) in lines.iter().take(n).enumerate() {
            c.text(Wt::Regular, 12.0, x + 22.0, y + 32.0 + k as f32 * 15.0, t.fg2, l);
        }
        c.hit(x + 20.0, y, w - 20.0, rh, Action::IleN(N_HW, i as u16));
        y += rh + 4.0;
    }
}

fn calendar_page(c: &mut Ctx, r: &R, w: f32) {
    let t = c.t;
    let p = &r.d.pronote;
    if !p.connected {
        c.text(Wt::Regular, 13.5, SIDE, 40.0, t.fg2, "Pronote n'est pas connecté : l'emploi du temps s'affichera ici.");
        c.hit(SIDE, 20.0, w - 2.0 * SIDE, 40.0, Action::Accounts);
        return;
    }
    let today = r.now.date();
    let colw = (w - 2.0 * SIDE - 20.0) / 2.0;
    for (k, day) in [today, today + chrono::Duration::days(1)].iter().enumerate() {
        let x = SIDE + k as f32 * (colw + 20.0);
        caps(c, x, 18.0, t.fg2, &widgets::day_label(*day, today));
        let mut y = 30.0;
        let list: Vec<_> = p.lessons.iter().filter(|l| l.start.date() == *day).collect();
        if list.is_empty() {
            c.text(Wt::Regular, 13.0, x, y + 20.0, t.fg3, "Pas cours");
        }
        for l in list {
            if y + 30.0 > BODY - 10.0 {
                break;
            }
            let now_in = r.now >= l.start && r.now < l.end;
            c.rr(x, y, colw, 28.0, 9.0, if now_in { t.accent.a(0.18) } else { t.fg.a(0.05) });
            c.rr(x + 6.0, y + 6.0, 3.0, 16.0, 1.5, Rgba::from(l.color));
            c.text(Wt::Semibold, 12.0, x + 16.0, y + 18.5, t.fg2, &l.start.format("%H:%M").to_string());
            let room = if l.room.is_empty() { String::new() } else { widgets::room_txt(&l.room) };
            let rw = c.text_r(Wt::Regular, 11.5, x + colw - 8.0, y + 18.5, t.fg3, &room);
            let sub = c.fit(Wt::Semibold, 13.0, &l.subject, colw - 70.0 - rw);
            let sw = c.text(Wt::Semibold, 13.0, x + 58.0, y + 18.5, if l.bad() { t.fg3 } else { t.fg }, &sub);
            if l.bad() {
                c.line(x + 58.0, y + 14.0, x + 58.0 + sw, y + 14.0, 1.4, RED);
            }
            y += 32.0;
        }
    }
}
