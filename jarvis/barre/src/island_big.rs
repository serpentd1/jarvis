//! « Grande île » : l'île dépliée en grand panneau (barre de sections + contenu), et les pages des
//! sections de Jarvis (aujourd'hui, tâches, rappels, habitudes, copiés, routines, mémoire, batteries, PC,
//! paramètres), aussi utilisées comme onglets compacts dans l'île normale.

use chrono::{Datelike, Timelike};
use serde_json::Value;

use crate::gfx::{Rgba, WHITE, Wt};
use crate::island::*;
use crate::island_cards::{self, caps, icon, media_card, weather_card};
use crate::island_draw::*;
use crate::jarvis_data::{arr, mem_reason, mem_rows, s, MEM_FILTERS, MEM_SORTS};
use crate::pronote::{Lesson, School};
use crate::widgets::{self, Action, Ctx, GREEN, ORANGE, RED};

pub const BIG_W: f32 = 1100.0;
pub const BIG_H: f32 = 720.0;
/// N° de « Paramètres » dans SECTIONS : cette section ne peut pas être cachée.
pub const SEC_PARAMS: usize = 17;

/// Sections de la grande île, dans l'ordre de la barre.
pub const SECTIONS: [(View, Tab, &str); 21] = [
    (View::Today, Tab::Today, "Aujourd'hui"),
    (View::Main, Tab::Home, "Accueil"),
    (View::Chat, Tab::Jarvis, "Jarvis"),
    (View::Tasks, Tab::Tasks, "Tâches"),
    (View::Reminders, Tab::Reminders, "Rappels"),
    (View::Habits, Tab::Habits, "Habitudes"),
    (View::Clips, Tab::Clips, "Copiés"),
    (View::Routines, Tab::Routines, "Routines"),
    (View::Memory, Tab::Memory, "Mémoire"),
    (View::Music, Tab::Music, "Musique"),
    (View::Batteries, Tab::Batteries, "Batteries"),
    (View::Pc, Tab::Pc, "Mon PC"),
    (View::Phone, Tab::Phone, "Téléphone"),
    (View::School, Tab::School, "Pronote"),
    (View::Mails, Tab::Mails, "Mails"),
    (View::Discord, Tab::Discord, "Discord"),
    (View::Timers, Tab::Timers, "Minuteurs"),
    (View::JSettings, Tab::JSettings, "Paramètres"),
    (View::Accounts, Tab::Accounts, "Comptes"),
    (View::History, Tab::History, "Historique"),
    (View::Printer, Tab::Printer, "Imprimante 3D"),
];

/// État d'interface des sections (liste de tâches choisie, date du rappel, confirmation…).
pub struct BigUi {
    pub task_list: usize,
    pub rem_when: chrono::NaiveDateTime,
    pub pc_confirm: Option<(usize, std::time::Instant)>,
    pub msg: String,
    /// Nouvelle routine en cours d'écriture.
    pub rt_nom: String,
    pub rt_decl: String,
    pub rt_heure: String,
    pub rt_etapes: Vec<String>,
    /// Lien en cours : souvenir de départ, puis d'arrivée.
    pub link_from: Option<String>,
    pub link_to: Option<String>,
    /// Mémoire affichée en liste triable (sinon en carte).
    pub mem_list: bool,
    /// Filtre (n° dans MEM_FILTERS), ordre (n° dans MEM_SORTS) et recherche de la liste.
    pub mem_filter: u8,
    pub mem_sort: u8,
    pub mem_search: String,
    /// Souvenirs cochés dans la liste.
    pub mem_checked: Vec<String>,
    /// Souvenir en cours de renommage.
    pub mem_edit: Option<String>,
    /// Premier clic sur « Supprimer » : il faut confirmer dans les 5 s.
    pub mem_confirm: Option<std::time::Instant>,
    /// Paramètres : page choisie (n° dans reglages::PAGES).
    pub reg_page: usize,
    /// Dernière page de la grande appli (pour « Page qui s'ouvre en premier : Dernière page »).
    pub last_view: Option<View>,
    /// Dernière image de la caméra de l'imprimante, message de la recherche, 1er clic sur « Arrêter ».
    pub printer_cam: Option<tiny_skia::Pixmap>,
    pub printer_msg: String,
    pub printer_stop: Option<std::time::Instant>,
}

impl BigUi {
    /// La suppression de la mémoire attend-elle sa confirmation ?
    pub fn mem_confirming(&self) -> bool {
        self.mem_confirm.is_some_and(|at| at.elapsed().as_secs() < 5)
    }
    /// « Arrêter l'impression » attend-il son 2e clic ?
    pub fn printer_stopping(&self) -> bool {
        self.printer_stop.is_some_and(|at| at.elapsed().as_secs() < 5)
    }
}

impl Default for BigUi {
    fn default() -> Self {
        let now = chrono::Local::now().naive_local() + chrono::Duration::hours(1);
        let rem_when = now.with_minute(0).and_then(|t| t.with_second(0)).unwrap_or(now);
        BigUi {
            task_list: 0,
            rem_when,
            pc_confirm: None,
            msg: String::new(),
            rt_nom: String::new(),
            rt_decl: String::new(),
            rt_heure: String::new(),
            rt_etapes: Vec::new(),
            link_from: None,
            link_to: None,
            mem_list: false,
            mem_filter: 0,
            mem_sort: 0,
            mem_search: String::new(),
            mem_checked: Vec::new(),
            mem_edit: None,
            mem_confirm: None,
            reg_page: 0,
            last_view: None,
            printer_cam: None,
            printer_msg: String::new(),
            printer_stop: None,
        }
    }
}

pub const PC_ACTIONS: [&str; 8] = ["verrouiller", "ecran_off", "veille", "gestionnaire", "parametres", "corbeille", "redemarrer", "eteindre"];
pub const PC_DANGER: [&str; 4] = ["veille", "corbeille", "redemarrer", "eteindre"];

/// La grande île est-elle demandée (réglage ou ouverture à la demande) ?
pub fn is_big(r: &R) -> bool {
    r.c().grande == 1 || r.st.big_force
}

// ---------- Grand panneau ----------

/// Taille de la grande appli (réglages « Largeur » et « Hauteur »).
pub fn big_size(c: &crate::config::IleCfg) -> (f32, f32) {
    (BIG_W * c.g_largeur.clamp(0.75, 1.3), BIG_H * c.g_hauteur.clamp(0.75, 1.3))
}

/// Largeur de la colonne des sections.
pub fn colonne(c: &crate::config::IleCfg) -> f32 {
    if c.g_reduite { 64.0 } else { c.g_colonne.clamp(140.0, 300.0) }
}

/// Ordre des sections (n° dans SECTIONS) : l'ordre choisi, puis celles qui n'y sont pas encore.
pub fn ordre(c: &crate::config::IleCfg) -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    for &i in &c.g_ordre {
        let i = i as usize;
        if i < SECTIONS.len() && !v.contains(&i) {
            v.push(i);
        }
    }
    v.extend((0..SECTIONS.len()).filter(|i| !c.g_ordre.contains(&(*i as u8))));
    v
}

/// Section cachée dans la colonne ? (« Paramètres » ne l'est jamais.)
pub fn cachee(c: &crate::config::IleCfg, i: usize) -> bool {
    i != SEC_PARAMS && c.g_cachees.contains(&(i as u8))
}

/// Animation de page en cours : (sorte, avancement 0 → 1). 1 = fondu, 2 = glissement, 3 = zoom.
pub fn anim_page(r: &R) -> Option<(usize, f32)> {
    let c = r.c();
    let (t0, ouverture) = r.st.page_t?;
    let sorte = if c.g_anim_page > 0 { c.g_anim_page } else if ouverture && c.g_anim_ouverture { 1 } else { return None };
    if !is_big(r) {
        return None;
    }
    let k = t0.elapsed().as_secs_f32() * 1000.0 / c.g_anim_duree.max(50.0);
    (k < 1.0).then_some((sorte, k))
}

/// La souris est-elle sur cette zone (coordonnées de dessin) ?
fn survole(c: &Ctx, r: &R, x: f32, y: f32, w: f32, h: f32) -> bool {
    r.st.souris.is_some_and(|(mx, my)| {
        let (px, py) = (c.ox + x * c.s, c.oy + y * c.s);
        mx >= px && mx < px + w * c.s && my >= py && my < py + h * c.s
    })
}

pub fn big_view(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let cf = r.c();
    island_cards::header(c, r, w);
    // colonne des sections
    let sb = colonne(cf);
    let cur = Tab::of(r.st.view);
    let list: Vec<usize> = ordre(cf).into_iter().filter(|&i| !cachee(cf, i)).collect();
    let y0 = 48.0;
    let bas = if r.st.big_force { 44.0 } else { 12.0 } + if cf.g_heure { 24.0 } else { 0.0 };
    let pas = cf.g_espace.clamp(22.0, 44.0).min((h - y0 - bas) / list.len().max(1) as f32);
    let hl = (pas - 1.0).max(16.0);
    if cf.g_fond_colonne {
        c.rr(6.0, 44.0, sb - 8.0, h - 52.0, 14.0, t.fg.a(0.06));
    }
    let n = SECTIONS.len() as f32;
    let mut y = y0;
    for &i in &list {
        let (v, tab, name) = SECTIONS[i];
        let on = tab == cur;
        let teinte = cf.g_couleurs.then(|| hsv(i as f32 / n, 0.55, 1.0));
        let dx = if cf.g_survol && !on && survole(c, r, 12.0, y, sb - 20.0, hl) { 5.0 } else { 0.0 };
        let (bx, bw) = (12.0 + dx, sb - 20.0);
        let rad = (hl * 0.37).min(11.0);
        let mut fg = if on { t.fg } else { t.fg2 };
        if on {
            match cf.g_selection {
                1 => {
                    c.rr(bx, y, bw, hl, rad, teinte.unwrap_or(t.accent).a(0.9));
                    fg = WHITE;
                }
                2 => c.rr_stroke(bx, y, bw, hl, rad, teinte.unwrap_or(t.accent), 1.5),
                3 => {
                    c.rr(bx, y, bw, hl, rad, t.fg.a(0.08));
                    c.rr(bx, y + 5.0, 3.5, hl - 10.0, 1.75, teinte.unwrap_or(t.accent));
                }
                _ => c.rr(bx, y, bw, hl, rad, teinte.map(|k| k.a(0.28)).unwrap_or(t.fg.a(0.13))),
            }
        }
        let ic = if on && cf.g_selection == 1 { WHITE } else { teinte.unwrap_or(fg) };
        if cf.g_reduite {
            icon(c, tab, sb / 2.0 - 4.0 + dx, y + hl / 2.0, ic);
        } else {
            let mut tx = 20.0 + dx;
            if cf.g_icones {
                icon(c, tab, 32.0 + dx, y + hl / 2.0, ic);
                tx = 52.0 + dx;
            }
            let wt = if cf.g_gras { Wt::Semibold } else { Wt::Regular };
            let fsz = (hl * 0.45).clamp(11.0, 13.5);
            let name = c.fit(wt, fsz, name, sb - tx - 14.0);
            c.text(wt, fsz, tx, y + hl / 2.0 + fsz * 0.38, fg, &name);
        }
        c.hit(12.0, y, sb - 20.0, hl, Action::IleN(N_SECTION, v as u16));
        y += pas;
    }
    if cf.g_heure {
        let yh = if r.st.big_force { h - 48.0 } else { h - 14.0 };
        let hh = r.now.format("%H:%M").to_string();
        c.text_c(Wt::Semibold, 15.0, sb / 2.0, yh, t.fg2, &hh);
    }
    if r.st.big_force {
        let lab = if cf.g_reduite { "Fermer" } else { "Fermer (Échap)" };
        chip(c, if cf.g_reduite { 6.0 } else { 16.0 }, h - 36.0, lab, t.fg.a(0.12), t.fg2, Some(Action::Ile(A_BIG_CLOSE)));
    }
    if cf.g_ligne {
        c.line(sb, 50.0, sb, h - 16.0, 1.0, t.sep);
    }
    // contenu (avec l'animation de changement de page)
    let (ox, oy) = (c.ox, c.oy);
    c.ox += (sb - 4.0) * c.s;
    c.oy += island_cards::TOP * c.s;
    let cw = w - sb + 4.0;
    let ch = h - island_cards::TOP - 12.0;
    match anim_page(r) {
        Some((2, k)) => {
            let e = 1.0 - (1.0 - k).powi(3);
            c.ox += (1.0 - e) * 40.0 * c.s;
            contenu(c, r, cw, ch);
        }
        Some((sorte, k)) => {
            let e = 1.0 - (1.0 - k).powi(3);
            if let Some(mut layer) = crate::gfx::calque(c.pm.width(), c.pm.height()) {
                let hits = {
                    let mut sub = Ctx::new(&mut layer, c.f, c.t, c.s, c.ox, c.oy);
                    sub.tf = c.tf;
                    sub.bold = c.bold;
                    contenu(&mut sub, r, cw, ch);
                    sub.hits
                };
                c.hits.extend(hits);
                let tr = if sorte == 3 {
                    let z = 0.95 + 0.05 * e;
                    let (zx, zy) = (c.ox + cw * c.s / 2.0, c.oy + ch * c.s / 2.0);
                    tiny_skia::Transform::from_translate(zx, zy).pre_scale(z, z).pre_translate(-zx, -zy)
                } else {
                    tiny_skia::Transform::identity()
                };
                let pp = tiny_skia::PixmapPaint { opacity: e.clamp(0.0, 1.0), quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
                c.pm.draw_pixmap(0, 0, layer.as_ref(), &pp, tr, None);
                crate::gfx::rendre_calque(layer);
            }
        }
        None => contenu(c, r, cw, ch),
    }
    c.ox = ox;
    c.oy = oy;
}

fn contenu(c: &mut Ctx, r: &R, cw: f32, ch: f32) {
    match r.st.view {
        View::Main => big_home(c, r, cw, ch),
        v => {
            if !section_page(c, r, v, cw, ch) {
                island_cards::page(c, r, v, cw);
            }
        }
    }
}

/// Pages propres aux sections de Jarvis. `false` = page déjà existante (à dessiner par island_cards).
pub fn section_page(c: &mut Ctx, r: &R, v: View, w: f32, h: f32) -> bool {
    match v {
        View::Today => today_page(c, r, w, h),
        View::Printer => printer_page(c, r, w, h),
        View::Tasks => tasks_page(c, r, w, h),
        View::Reminders => reminders_page(c, r, w, h),
        View::Habits => habits_page(c, r, w, h),
        View::Clips if r.jd.get("copies").is_some() => copies_page(c, r, w, h),
        View::Routines => routines_page(c, r, w, h),
        View::Memory => memory_page(c, r, w, h),
        View::Batteries => batteries_page(c, r, w, h),
        View::Pc => pc_page(c, r, w, h),
        View::JSettings => jsettings_page(c, r, w, h),
        View::Accounts => accounts_page(c, r, w, h),
        View::History => history_page(c, r, w, h),
        View::Phone if is_big(r) => big_phone(c, r, w, h),
        View::Mails if is_big(r) => tall(c, r, w, h, mails_page),
        View::Discord if is_big(r) => tall(c, r, w, h, discord_page),
        View::Homework if is_big(r) => tall(c, r, w, h, homework_page),
        View::Grades if is_big(r) => tall(c, r, w, h, grades_page),
        View::School if is_big(r) => {
            let lw = (w * 0.58).round();
            tall(c, r, lw, h, homework_page);
            pronote_col(c, r, lw + 6.0, w - lw - SIDE - 6.0);
        }
        View::Timers if is_big(r) => {
            time_col(c, r, SIDE, 220.0);
            timers_col(c, r, SIDE + 250.0, 230.0);
            island_cards::focus_card(c, r, SIDE + 510.0, -2.0, w - SIDE - (SIDE + 510.0), h - 10.0, r.c().right_grad);
        }
        View::Music if is_big(r) => {
            island_cards::music_page(c, r, w);
            music_extra(c, r, SIDE, 236.0, w);
        }
        _ => return false,
    }
    true
}

/// Message si Jarvis ne répond pas (ou chargement). `true` = rien à dessiner de plus.
fn absent(c: &mut Ctx, r: &R, name: &str) -> bool {
    match r.jd.missing(name) {
        Some(m) => {
            let t = c.t;
            c.text(Wt::Semibold, 15.0, SIDE, 40.0, t.fg, m);
            if m.starts_with("Jarvis") {
                c.text(Wt::Regular, 12.5, SIDE, 62.0, t.fg2, "Lance l'assistant vocal Jarvis pour voir cette section.");
            }
            true
        }
        None => false,
    }
}

/// Champ de saisie pour une cible donnée (réutilise la saisie de l'île).
pub fn field(c: &mut Ctx, r: &R, x: f32, y: f32, w: f32, kind: InputFor, placeholder: &str) {
    let t = c.t;
    let st = r.st;
    let active = st.typing && st.input_for == kind;
    let fh = 32.0;
    c.rr(x, y, w, fh, 16.0, t.fg.a(if active { 0.16 } else { 0.08 }));
    if active {
        c.rr_stroke(x, y, w, fh, 16.0, t.accent.a(0.85), 1.3);
        let mut v: Vec<char> = st.input.chars().collect();
        while !v.is_empty() && c.measure(Wt::Regular, 12.5, &v.iter().collect::<String>()) > w - 54.0 {
            v.remove(0);
        }
        let shown: String = v.into_iter().collect();
        let tw = c.text(Wt::Regular, 12.5, x + 12.0, y + 20.5, t.fg, &shown);
        if (r.t() * 2.0).fract() < 0.6 {
            c.line(x + 13.0 + tw, y + 9.0, x + 13.0 + tw, y + fh - 9.0, 1.3, t.fg);
        }
    } else {
        c.text(Wt::Regular, 12.5, x + 12.0, y + 20.5, t.fg3, placeholder);
    }
    c.hit(x, y, w - 30.0, fh, Action::IleN(N_INPUT, kind.code()));
    let (bx, by) = (x + w - 17.0, y + fh / 2.0);
    c.circle(bx, by, 12.0, if active && !st.input.trim().is_empty() { t.accent } else { t.track });
    c.line(bx - 5.0, by, bx + 5.0, by, 1.9, WHITE);
    c.line(bx, by - 5.0, bx, by + 5.0, 1.9, WHITE);
    c.hit(bx - 15.0, y, 30.0, fh, Action::Ile(A_SEND));
}

fn checkbox(c: &mut Ctx, cx: f32, cy: f32, on: bool) {
    let t = c.t;
    if on {
        c.circle(cx, cy, 9.0, GREEN);
        c.line(cx - 4.0, cy, cx - 1.0, cy + 3.5, 2.0, WHITE);
        c.line(cx - 1.0, cy + 3.5, cx + 4.5, cy - 3.5, 2.0, WHITE);
    } else {
        c.ring(cx, cy, 8.0, t.fg2, 1.6);
    }
}

fn cross(c: &mut Ctx, cx: f32, cy: f32, act: Action) {
    let t = c.t;
    c.line(cx - 4.0, cy - 4.0, cx + 4.0, cy + 4.0, 1.6, t.fg3);
    c.line(cx - 4.0, cy + 4.0, cx + 4.0, cy - 4.0, 1.6, t.fg3);
    c.hit(cx - 12.0, cy - 12.0, 24.0, 24.0, act);
}

fn flame(c: &mut Ctx, cx: f32, cy: f32) {
    c.poly(&[(cx, cy - 8.0), (cx + 5.0, cy - 1.0), (cx + 4.0, cy + 5.0), (cx, cy + 7.0), (cx - 4.0, cy + 5.0), (cx - 5.0, cy - 1.0), (cx - 2.0, cy - 4.0)], ORANGE);
    c.circle(cx, cy + 3.0, 2.4, Rgba::rgb8(255, 214, 10));
}

/// Jauge horizontale avec libellé et valeur.
fn gauge(c: &mut Ctx, x: f32, y: f32, w: f32, label: &str, frac: f32, value: &str) {
    let t = c.t;
    c.text(Wt::Semibold, 12.5, x, y, t.fg, label);
    c.text_r(Wt::Regular, 12.0, x + w, y, t.fg2, value);
    let f = frac.clamp(0.0, 1.0);
    let col = if f > 0.85 { RED } else if f > 0.65 { ORANGE } else { GREEN };
    c.rr(x, y + 7.0, w, 7.0, 3.5, t.track);
    if f > 0.0 {
        c.rr(x, y + 7.0, (w * f).max(7.0), 7.0, 3.5, col);
    }
}

// ---------- Accueil (grand) ----------

fn big_home(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let mut h = h;
    let oy0 = c.oy;
    if let Some((ti, sub)) = account_alert(r) {
        alert_banner(c, SIDE - 6.0, 4.0, w - 2.0 * SIDE + 12.0, &ti, &sub);
        c.oy += 48.0 * c.s;
        h -= 48.0;
    }
    let x0 = SIDE - 6.0;
    let gw = w - 2.0 * x0;
    // ligne 1 : salut + météo + prochain cours | musique (ou scène météo)
    let lw = (gw * 0.46).round();
    card(c, r, x0, 4.0, lw, 196.0);
    let h_ = r.now.hour();
    let salut = if h_ < 5 { "Bonne nuit" } else if h_ < 12 { "Bonjour" } else if h_ < 18 { "Bon après-midi" } else { "Bonsoir" };
    c.text(Wt::Semibold, 24.0, x0 + 16.0, 40.0, t.fg, &format!("{salut} !"));
    let date = date_txt(r.c(), r.now);
    c.text(Wt::Regular, 13.0, x0 + 16.0, 62.0, t.fg2, &date);
    if let Some(wt) = &r.d.weather {
        c.icon(wt.code, wt.day, x0 + 12.0, 74.0, 44.0);
        c.text(Wt::Light, 34.0, x0 + 62.0, 108.0, t.fg, &temp_txt(r.c(), wt.temp));
        let ph = weather_phrase(r.c(), wt.code, wt.day, wt.temp, r.now.ordinal());
        let ph = c.fit(Wt::Regular, 12.5, &ph, lw - 140.0);
        c.text(Wt::Regular, 12.5, x0 + 130.0, 100.0, t.fg2, &ph);
    }
    if let Some(l) = r.d.pronote.next(r.now).0.filter(|_| r.d.pronote.connected) {
        caps(c, x0 + 16.0, 146.0, t.fg3, "Prochain cours");
        let txt = format!("{} · {} ({})", l.subject, l.start.format("%H:%M"), widgets::countdown_txt(l, r.now));
        let txt = c.fit(Wt::Semibold, 14.0, &txt, lw - 32.0);
        c.text(Wt::Semibold, 14.0, x0 + 16.0, 170.0, t.fg, &txt);
    }
    let mx = x0 + lw + 10.0;
    if r.d.media.as_ref().is_some_and(|m| !m.title.is_empty()) {
        media_card(c, r, mx, 4.0, gw - lw - 10.0, 196.0);
    } else {
        weather_card(c, r, mx, 4.0, gw - lw - 10.0, 196.0);
    }
    // ligne 2 : tâches, rappels, habitudes
    let cw = (gw - 20.0) / 3.0;
    let y2 = 212.0;
    let h2 = 214.0;
    for (i, (title, view)) in [("Tâches", View::Tasks), ("Rappels", View::Reminders), ("Habitudes", View::Habits)].iter().enumerate() {
        let x = x0 + i as f32 * (cw + 10.0);
        card(c, r, x, y2, cw, h2);
        caps(c, x + 14.0, y2 + 24.0, t.fg2, title);
        c.hit(x, y2, cw, 30.0, Action::IleN(N_SECTION, *view as u16));
        let mut y = y2 + 40.0;
        match view {
            View::Tasks => {
                let lists = arr(r.jd.get("taches"), "listes");
                let list = lists.get(r.ui.task_list.min(lists.len().saturating_sub(1)));
                let items: Vec<(usize, &Value)> = list.map(|l| arr(Some(l), "taches")).unwrap_or_default().into_iter().enumerate().filter(|(_, t)| !t["fait"].as_bool().unwrap_or(false)).collect();
                if items.is_empty() {
                    c.text(Wt::Regular, 12.5, x + 14.0, y + 12.0, t.fg3, r.jd.missing("taches").unwrap_or("Rien à faire."));
                }
                for (k, task) in items.into_iter().take(5) {
                    checkbox(c, x + 22.0, y + 9.0, false);
                    c.hit(x + 10.0, y - 4.0, 26.0, 26.0, Action::IleN(N_TASK_TOGGLE, k as u16));
                    let txt = c.fit(Wt::Regular, 13.0, s(task, "texte"), cw - 50.0);
                    c.text(Wt::Regular, 13.0, x + 38.0, y + 13.5, t.fg, &txt);
                    y += 30.0;
                }
            }
            View::Reminders => {
                let list = arr(r.jd.get("rappels"), "rappels");
                let todo: Vec<&&Value> = list.iter().filter(|x| !x["fait"].as_bool().unwrap_or(false)).collect();
                if todo.is_empty() {
                    c.text(Wt::Regular, 12.5, x + 14.0, y + 12.0, t.fg3, r.jd.missing("rappels").unwrap_or("Aucun rappel."));
                }
                for rem in todo.into_iter().take(5) {
                    c.text(Wt::Semibold, 11.5, x + 14.0, y + 12.0, t.accent, &quand_txt(s(rem, "quand"), r.now));
                    let txt = c.fit(Wt::Regular, 12.5, s(rem, "texte"), cw - 28.0);
                    c.text(Wt::Regular, 12.5, x + 14.0, y + 27.0, t.fg, &txt);
                    y += 34.0;
                }
            }
            _ => {
                let list = arr(r.jd.get("habitudes"), "habitudes");
                if list.is_empty() {
                    c.text(Wt::Regular, 12.5, x + 14.0, y + 12.0, t.fg3, r.jd.missing("habitudes").unwrap_or("Aucune habitude."));
                }
                for (k, hb) in list.iter().enumerate().take(5) {
                    checkbox(c, x + 22.0, y + 9.0, hb["aujourdhui"].as_bool().unwrap_or(false));
                    c.hit(x + 10.0, y - 4.0, 26.0, 26.0, Action::IleN(N_HAB_TOGGLE, k as u16));
                    let txt = c.fit(Wt::Regular, 13.0, s(hb, "nom"), cw - 90.0);
                    c.text(Wt::Regular, 13.0, x + 38.0, y + 13.5, t.fg, &txt);
                    let serie = hb["serie"].as_u64().unwrap_or(0);
                    if serie > 0 {
                        flame(c, x + cw - 40.0, y + 9.0);
                        c.text(Wt::Semibold, 12.0, x + cw - 30.0, y + 13.5, ORANGE, &serie.to_string());
                    }
                    y += 30.0;
                }
            }
        }
    }
    // ligne 3 : batteries + téléphone | parler à Jarvis
    let y3 = y2 + h2 + 12.0;
    let h3 = (h - y3 - 4.0).max(120.0);
    let bw = (gw * 0.42).round();
    card(c, r, x0, y3, bw, h3);
    caps(c, x0 + 14.0, y3 + 24.0, t.fg2, "Batteries et téléphone");
    c.hit(x0, y3, bw, 30.0, Action::IleN(N_SECTION, View::Batteries as u16));
    let mut y = y3 + 44.0;
    for a in arr(r.jd.get("appareils"), "appareils").into_iter().filter(|a| a["batterie"].is_number()).take(3) {
        let b = a["batterie"].as_f64().unwrap_or(0.0) as f32;
        gauge(c, x0 + 14.0, y, bw - 28.0, s(a, "nom"), b / 100.0, &format!("{}{} %", if a["branche"].as_bool() == Some(true) { "en charge · " } else { "" }, b.round()));
        y += 30.0;
    }
    for p in r.d.phone.iter().rev().take(2) {
        let txt = format!("{} · {} : {}", p.kind, p.from, mask_codes(&p.text));
        let txt = c.fit(Wt::Regular, 12.5, &txt, bw - 28.0);
        c.text(Wt::Regular, 12.5, x0 + 14.0, y + 12.0, t.fg2, &txt);
        y += 20.0;
    }
    if y == y3 + 44.0 {
        c.text(Wt::Regular, 12.5, x0 + 14.0, y + 12.0, t.fg3, "Aucun appareil ni message du téléphone.");
    }
    let jx = x0 + bw + 10.0;
    let jw = gw - bw - 10.0;
    card(c, r, jx, y3, jw, h3);
    caps(c, jx + 14.0, y3 + 24.0, t.fg2, "Parler à Jarvis");
    input_field(c, r, jx + 10.0, y3 + 36.0, jw - 20.0);
    if let Some((q, a)) = r.chat.log.last() {
        let mut yy = y3 + 92.0;
        if !q.is_empty() {
            let q = c.fit(Wt::Semibold, 12.0, &format!("« {q} »"), jw - 28.0);
            c.text(Wt::Semibold, 12.0, jx + 14.0, yy, t.fg2, &q);
            yy += 18.0;
        }
        if a.is_empty() && r.chat.asking {
            thinking(c, r, jx + 14.0, yy + 4.0);
        } else {
            let max = ((y3 + h3 - 8.0 - yy) / 16.0).max(1.0) as usize;
            for (k, l) in c.wrap(Wt::Regular, 12.5, a, jw - 28.0).iter().take(max).enumerate() {
                c.text(Wt::Regular, 12.5, jx + 14.0, yy + k as f32 * 16.0, t.fg, l);
            }
        }
    }
    c.oy = oy0;
}

fn quand_txt(q: &str, now: chrono::NaiveDateTime) -> String {
    let q = q.replace('T', " ");
    match chrono::NaiveDateTime::parse_from_str(&q, "%Y-%m-%d %H:%M") {
        Ok(d) => format!("{} à {}", widgets::day_label(d.date(), now.date()), d.format("%H:%M")),
        Err(_) => q,
    }
}

// ---------- Tâches ----------

// ---------- Aujourd'hui (tout au même endroit, frise du jour, « fin dans … ») ----------

/// Cours où je vais vraiment (ni PAI, ni annulé, ni prof absent).
fn j_y_vais(l: &Lesson) -> bool {
    l.manque.is_empty() && !l.bad()
}

/// Première lettre en majuscule (« fin du cours… » → « Fin du cours… »).
fn majuscule(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().chain(ch).collect(),
        None => String::new(),
    }
}

/// Minutes entre deux instants, arrondies au-dessus (comme le compte à rebours du prochain cours).
fn mins_entre(a: chrono::NaiveDateTime, b: chrono::NaiveDateTime) -> i64 {
    ((b - a).num_seconds() + 59).max(0) / 60
}

/// Jour montré par la page, et prochain jour de cours (pour les devoirs « pour demain »).
/// Le soir (après mon dernier cours) et les jours sans cours, on montre le prochain jour de cours.
pub(crate) fn jours_page(p: &School, now: chrono::NaiveDateTime) -> (chrono::NaiveDate, chrono::NaiveDate) {
    let today = now.date();
    let suivant = (1..=7).map(|k| today + chrono::Duration::days(k)).find(|d| p.lessons_on(*d).into_iter().any(j_y_vais));
    let fin = p.lessons_on(today).into_iter().filter(|l| j_y_vais(l)).map(|l| l.end).max();
    let montre = match (fin, suivant) {
        (Some(f), _) if now < f => today,
        (_, Some(d)) => d,
        // vacances : rien dans la semaine, on reste sur aujourd'hui
        _ => today,
    };
    (montre, suivant.unwrap_or(today + chrono::Duration::days(1)))
}

/// Heures de début et de fin de la frise : 8 h – 18 h, élargies si un cours déborde.
pub(crate) fn frise_bornes(cours: &[&Lesson]) -> (u32, u32) {
    let (mut a, mut b) = (8, 18);
    for l in cours {
        a = a.min(l.start.hour());
        // un cours qui finit après minuit s'arrête au bout de la frise
        let fin = if l.end.date() > l.start.date() { 24 } else { l.end.hour() + u32::from(l.end.minute() > 0) };
        b = b.max(fin);
    }
    (a, b.min(24).max(a + 1))
}

/// Position (en x) d'une heure sur la frise qui va de `x` à `x + w` (collée aux bords si elle déborde).
pub(crate) fn frise_x(t: chrono::NaiveTime, bornes: (u32, u32), x: f32, w: f32) -> f32 {
    let m = t.num_seconds_from_midnight() as f32 / 60.0;
    let (a, b) = (bornes.0 as f32 * 60.0, bornes.1 as f32 * 60.0);
    x + w * ((m - a) / (b - a)).clamp(0.0, 1.0)
}

/// Textes sous la frise, seulement pendant les heures de cours : (« fin du cours dans 12 min »
/// ou « pause : Maths dans 8 min », « fin de la journée dans 2 h 05 »). `None` avant le premier
/// cours et après le dernier. Les cours annulés et ceux où je ne vais pas ne comptent pas.
pub(crate) fn fins_txt(cours: &[&Lesson], now: chrono::NaiveDateTime) -> Option<(String, String)> {
    let vrais: Vec<&Lesson> = cours.iter().copied().filter(|l| j_y_vais(l) && l.start.date() == now.date()).collect();
    let debut = vrais.iter().map(|l| l.start).min()?;
    let fin = vrais.iter().map(|l| l.end).max()?;
    if now < debut || now >= fin {
        return None;
    }
    let un = match vrais.iter().find(|l| l.start <= now && now < l.end) {
        Some(l) => format!("fin du cours dans {}", widgets::dur_txt(mins_entre(now, l.end))),
        None => {
            let l = vrais.iter().filter(|l| l.start > now).min_by_key(|l| l.start)?;
            format!("pause : {} dans {}", l.subject, widgets::dur_txt(mins_entre(now, l.start)))
        }
    };
    Some((un, format!("fin de la journée dans {}", widgets::dur_txt(mins_entre(now, fin)))))
}

/// Frise de la journée : un bloc coloré par cours et le trait rouge « maintenant » (si c'est aujourd'hui).
/// `zone` = (x, y, largeur, hauteur) de la barre.
fn frise(c: &mut Ctx, cours: &[&Lesson], jour: chrono::NaiveDate, now: chrono::NaiveDateTime, zone: (f32, f32, f32, f32)) {
    let t = c.t;
    let (x, y, w, bh) = zone;
    let bornes = frise_bornes(cours);
    let pos = |d: chrono::NaiveDateTime| if d.date() > jour { x + w } else { frise_x(d.time(), bornes, x, w) };
    c.rr(x, y, w, bh, 10.0, t.fg.a(0.05));
    // un trait fin par heure, l'heure écrite dessous (une sur deux si c'est serré)
    let n = bornes.1 - bornes.0;
    let pas = if w / (n as f32) < 44.0 { 2 } else { 1 };
    for hh in bornes.0..=bornes.1 {
        let hx = x + w * (hh - bornes.0) as f32 / n as f32;
        if hh > bornes.0 && hh < bornes.1 {
            c.line(hx, y + 4.0, hx, y + bh - 4.0, 1.0, t.sep);
        }
        if (hh - bornes.0) % pas == 0 {
            c.text_c(Wt::Regular, 10.5, hx, y + bh + 14.0, t.fg3, &format!("{hh} h"));
        }
    }
    // d'abord les cours où je ne vais pas, puis les annulés, puis les vrais cours par-dessus
    let mut v: Vec<&Lesson> = cours.to_vec();
    v.sort_by_key(|l| if !l.manque.is_empty() { 0 } else if l.bad() { 1 } else { 2 });
    let (by, hh) = (y + 3.0, bh - 6.0);
    for l in v {
        let x1 = pos(l.start) + 1.0;
        let bw = (pos(l.end) - x1 - 1.0).max(3.0);
        if !l.manque.is_empty() {
            // emploi du temps aménagé : je n'y vais pas, juste un bloc gris très pâle
            c.rr(x1, by, bw, hh, 7.0, t.fg.a(0.04));
            continue;
        }
        let bad = l.bad();
        let col = Rgba::from(l.color);
        if bad {
            c.rr(x1, by, bw, hh, 7.0, t.fg.a(0.06));
            c.rr_stroke(x1, by, bw, hh, 7.0, RED.a(0.55), 1.0);
        } else {
            let dedans = jour == now.date() && l.start <= now && now < l.end;
            c.rr(x1, by, bw, hh, 7.0, col.a(if dedans { 0.45 } else { 0.30 }));
            c.rr(x1 + 3.0, by + 5.0, 3.0, hh - 10.0, 1.5, col);
        }
        if bw > 26.0 {
            let ty = by + hh / 2.0 - 1.0;
            let sub = c.fit(Wt::Semibold, 12.0, &l.subject, bw - 14.0);
            let sw = c.text(Wt::Semibold, 12.0, x1 + 10.0, ty, if bad { t.fg3 } else { t.fg }, &sub);
            if bad {
                // annulé ou prof absent : barré
                c.line(x1 + 10.0, ty - 4.5, x1 + 10.0 + sw, ty - 4.5, 1.3, RED);
            }
            if bw > 48.0 {
                c.text(Wt::Regular, 10.5, x1 + 10.0, ty + 14.0, t.fg2, &l.start.format("%H:%M").to_string());
            }
        }
    }
    // trait « maintenant » : il avance à chaque nouvel affichage de l'île
    if jour == now.date() {
        let m = now.time().num_seconds_from_midnight() as f32 / 60.0;
        if m >= bornes.0 as f32 * 60.0 && m <= bornes.1 as f32 * 60.0 {
            let xn = frise_x(now.time(), bornes, x, w);
            c.line(xn, y - 5.0, xn, y + bh + 3.0, 2.0, RED);
            c.circle(xn, y - 5.0, 3.5, RED);
        }
    }
}

/// Page « Aujourd'hui » : la frise du jour en haut, puis les cours, les devoirs pour demain, la météo,
/// les mails non lus et les minuteurs. Le soir, c'est le programme du prochain jour de cours.
pub fn today_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let p = &r.d.pronote;
    let ecole = p.connected && r.cfg.pronote.on;
    let now = r.now;
    let today = now.date();
    let (jour, suivant) = if ecole { jours_page(p, now) } else { (today, today + chrono::Duration::days(1)) };
    let est_auj = jour == today;
    let cours: Vec<&Lesson> = if ecole { p.lessons_on(jour) } else { Vec::new() };
    let vrais: Vec<&Lesson> = cours.iter().copied().filter(|l| j_y_vais(l)).collect();
    let big = h > 400.0;
    let x0 = SIDE - 6.0;
    let gw = w - 2.0 * x0;

    // ---- carte du haut : titre, frise, « fin du cours dans … »
    let fh = if big { 156.0 } else { 132.0 };
    card(c, r, x0, 4.0, gw, fh);
    let titre = if est_auj { "Aujourd'hui".to_string() } else { majuscule(&widgets::day_label(jour, today)) };
    let ty = if big { 36.0 } else { 30.0 };
    let tw = c.text(Wt::Semibold, if big { 20.0 } else { 16.0 }, x0 + 16.0, ty, t.fg, &titre);
    let date = date_txt(r.c(), jour.and_hms_opt(12, 0, 0).unwrap_or(now));
    c.text(Wt::Regular, 13.0, x0 + 26.0 + tw, ty, t.fg2, &date);
    let resume = match (vrais.first(), vrais.iter().map(|l| l.end).max()) {
        (Some(a), Some(b)) => format!("{} cours · {} – {}", vrais.len(), a.start.format("%H:%M"), b.format("%H:%M")),
        _ if ecole => "Pas cours".to_string(),
        _ => String::new(),
    };
    c.text_r(Wt::Semibold, 12.5, x0 + gw - 16.0, ty, t.fg2, &resume);
    let (bx, bw) = (x0 + 16.0, gw - 32.0);
    let (by, bh) = if big { (52.0, 46.0) } else { (44.0, 38.0) };
    let ly = by + bh + if big { 42.0 } else { 36.0 };
    if ecole {
        frise(c, &cours, jour, now, (bx, by, bw, bh));
        // sous la frise : « fin du cours dans 12 min · fin de la journée dans 2 h 05 » (pendant les cours)
        if let Some((a, b)) = if est_auj { fins_txt(&cours, now) } else { None } {
            let aw = c.text(Wt::Semibold, 15.0, bx, ly, t.accent, &majuscule(&a));
            c.text(Wt::Regular, 14.0, bx + aw + 10.0, ly, t.fg2, &format!("·   {}", majuscule(&b)));
        } else {
            let dl = widgets::day_label(jour, today);
            let msg = if !est_auj && !p.lessons_on(today).into_iter().any(j_y_vais) {
                format!("Pas cours aujourd'hui : voici le programme de {dl}.")
            } else if !est_auj {
                format!("Les cours sont finis pour aujourd'hui : voici le programme de {dl}.")
            } else if let Some(l) = vrais.iter().find(|l| l.start > now) {
                format!("Premier cours dans {} ({}).", widgets::dur_txt(mins_entre(now, l.start)), l.subject)
            } else {
                String::new()
            };
            let msg = c.fit(Wt::Regular, 13.5, &msg, bw);
            c.text(Wt::Regular, 13.5, bx, ly, t.fg2, &msg);
        }
    } else {
        let txt = if r.cfg.pronote.on { "Pronote n'est pas connecté : clique ici pour le connecter (Comptes)." } else { "Pronote est désactivé (Comptes) : pas d'emploi du temps." };
        c.text(Wt::Regular, 13.0, bx, by + 24.0, t.fg2, txt);
        c.hit(x0, 4.0, gw, fh, Action::Accounts);
    }

    // devoirs pour le prochain jour de cours : non faits d'abord
    let mut devoirs: Vec<usize> = if ecole { p.homework.iter().enumerate().filter(|(_, hw)| hw.due == suivant).map(|(i, _)| i).collect() } else { Vec::new() };
    devoirs.sort_by_key(|i| p.homework[*i].done);
    let reste = devoirs.iter().filter(|i| !p.homework[**i].done).count();
    let pour = format!("Devoirs pour {}", widgets::day_label(suivant, today));
    let nm = r.d.mail.mails.len();
    let ni = important_mails(r.d, r.c()).len();
    let y2 = 4.0 + fh + 10.0;

    if !big {
        // ---- île normale (petite) : trois petites cartes sous la frise
        let ch = (h - y2 - 2.0).max(60.0);
        let cw = (gw - 20.0) / 3.0;
        let x = x0;
        card(c, r, x, y2, cw, ch);
        // titre court : « Devoirs pour mercredi » ne tiendrait pas dans la petite carte
        caps(c, x + 14.0, y2 + 22.0, t.fg2, "Devoirs");
        let nw = c.text(Wt::Semibold, 22.0, x + 14.0, y2 + 52.0, if reste > 0 { ORANGE } else { t.fg3 }, &reste.to_string());
        let quand = c.fit(Wt::Regular, 12.5, &format!("pour {}", widgets::day_label(suivant, today)), cw - 34.0 - nw);
        c.text(Wt::Regular, 12.5, x + 20.0 + nw, y2 + 52.0, t.fg2, &quand);
        let txt = match devoirs.iter().find(|i| !p.homework[**i].done) {
            Some(&i) => format!("{} · {}", p.homework[i].subject, p.homework[i].text.replace('\n', " ")),
            None if ecole => "Rien à faire, profite !".to_string(),
            None => "Pronote n'est pas connecté.".to_string(),
        };
        let txt = c.fit(Wt::Regular, 12.5, &txt, cw - 28.0);
        c.text(Wt::Regular, 12.5, x + 14.0, y2 + 72.0, t.fg, &txt);
        c.hit(x, y2, cw, ch, Action::IleN(N_SECTION, View::Homework as u16));
        let x = x0 + cw + 10.0;
        card(c, r, x, y2, cw, ch);
        caps(c, x + 14.0, y2 + 22.0, t.fg2, "Météo");
        if let Some(wt) = &r.d.weather {
            c.icon(wt.code, wt.day, x + 10.0, y2 + 28.0, 34.0);
            c.text(Wt::Light, 24.0, x + 50.0, y2 + 54.0, t.fg, &temp_txt(r.c(), wt.temp));
            let ph = weather_phrase(r.c(), wt.code, wt.day, wt.temp, now.ordinal());
            let ph = c.fit(Wt::Regular, 12.0, &ph, cw - 28.0);
            c.text(Wt::Regular, 12.0, x + 14.0, y2 + 74.0, t.fg2, &ph);
        } else {
            c.text(Wt::Regular, 12.5, x + 14.0, y2 + 50.0, t.fg3, "Pas de météo.");
        }
        let x = x0 + 2.0 * (cw + 10.0);
        card(c, r, x, y2, cw, ch);
        caps(c, x + 14.0, y2 + 22.0, t.fg2, "Mails");
        if r.cfg.mail.on {
            let nw = c.text(Wt::Semibold, 22.0, x + 14.0, y2 + 52.0, if nm > 0 { t.fg } else { t.fg3 }, &nm.to_string());
            c.text(Wt::Regular, 12.5, x + 20.0 + nw, y2 + 52.0, t.fg2, if nm > 1 { "non lus" } else { "non lu" });
            if ni > 0 {
                c.text(Wt::Semibold, 12.0, x + 14.0, y2 + 72.0, ORANGE, &format!("dont {ni} important{}", if ni > 1 { "s" } else { "" }));
            }
        } else {
            c.text(Wt::Regular, 12.5, x + 14.0, y2 + 50.0, t.fg3, "Mails désactivés.");
        }
        c.hit(x, y2, cw, ch, Action::IleN(N_SECTION, View::Mails as u16));
        return;
    }

    // ---- à gauche : les cours du jour (sans ceux où je ne vais pas ; les annulés barrés)
    let bot = h - 4.0;
    let lw = (gw * 0.55).round();
    card(c, r, x0, y2, lw, bot - y2);
    let titre = if est_auj { "Cours d'aujourd'hui".to_string() } else { format!("Cours de {}", widgets::day_label(jour, today)) };
    caps(c, x0 + 14.0, y2 + 24.0, t.fg2, &titre);
    let liste: Vec<&Lesson> = cours.iter().copied().filter(|l| l.manque.is_empty()).collect();
    let mut y = y2 + 38.0;
    if !ecole {
        c.text(Wt::Regular, 12.5, x0 + 14.0, y + 14.0, t.fg3, "Pronote n'est pas connecté.");
    } else if liste.is_empty() {
        c.text(Wt::Regular, 12.5, x0 + 14.0, y + 14.0, t.fg3, "Pas cours ce jour-là.");
    }
    let (x, rw) = (x0 + 10.0, lw - 20.0);
    for (k, l) in liste.iter().enumerate() {
        if y + 42.0 > bot - 6.0 {
            let n = liste.len() - k;
            c.text(Wt::Regular, 12.0, x + 6.0, y + 12.0, t.fg3, &format!("+ {n} autre{} cours", if n > 1 { "s" } else { "" }));
            break;
        }
        let dedans = est_auj && l.start <= now && now < l.end;
        let passe = est_auj && l.end <= now;
        let bad = l.bad();
        c.rr(x, y, rw, 42.0, 11.0, if dedans { t.accent.a(0.18) } else { t.fg.a(0.05) });
        c.rr(x + 8.0, y + 9.0, 3.5, 24.0, 1.75, if bad { t.fg3 } else { Rgba::from(l.color) });
        let fg = if bad || passe { t.fg3 } else { t.fg };
        c.text(Wt::Semibold, 13.0, x + 20.0, y + 18.0, fg, &l.start.format("%H:%M").to_string());
        c.text(Wt::Regular, 11.5, x + 20.0, y + 34.0, t.fg3, &l.end.format("%H:%M").to_string());
        // à droite : annulé, prof absent, changement de salle…
        let lab = c.fit(Wt::Semibold, 10.5, &l.alert_label(), 170.0);
        let labw = if lab.is_empty() { 0.0 } else { c.text_r(Wt::Semibold, 10.5, x + rw - 12.0, y + 18.0, if bad { RED } else { ORANGE }, &lab) + 10.0 };
        let sub = c.fit(Wt::Semibold, 14.0, &l.subject, rw - 84.0 - labw);
        let sw = c.text(Wt::Semibold, 14.0, x + 72.0, y + 18.0, fg, &sub);
        if bad {
            c.line(x + 72.0, y + 13.0, x + 72.0 + sw, y + 13.0, 1.4, RED);
        }
        let mut info = Vec::new();
        if !l.room.is_empty() {
            info.push(widgets::room_txt(&l.room));
        }
        if !l.teacher.is_empty() {
            info.push(l.teacher.clone());
        }
        if l.perso {
            info.push("créneau perso".to_string());
        }
        if dedans {
            info.push(widgets::countdown_txt(l, now));
        }
        let info = c.fit(Wt::Regular, 12.0, &info.join("  ·  "), rw - 84.0);
        c.text(Wt::Regular, 12.0, x + 72.0, y + 34.0, t.fg2, &info);
        y += 46.0;
    }

    // ---- à droite : devoirs, puis météo + mails, puis minuteurs
    let rx = x0 + lw + 10.0;
    let rw = gw - lw - 10.0;
    let total = bot - y2;
    let dh = (total * 0.42).round();
    let mh = 104.0;
    let th = (total - dh - mh - 20.0).max(60.0);
    card(c, r, rx, y2, rw, dh);
    caps(c, rx + 14.0, y2 + 24.0, t.fg2, &pour);
    if reste > 0 {
        c.text_r(Wt::Semibold, 12.0, rx + rw - 14.0, y2 + 24.0, ORANGE, &format!("{reste} à faire"));
    }
    let mut y = y2 + 38.0;
    if devoirs.is_empty() {
        c.text(Wt::Regular, 12.5, rx + 14.0, y + 14.0, t.fg3, if ecole { "Rien pour ce jour-là, profite !" } else { "Pronote n'est pas connecté." });
    }
    let rows = ((dh - 46.0) / 28.0).floor().max(0.0) as usize;
    let n = devoirs.len();
    let vus = if n > rows { rows.saturating_sub(1) } else { n };
    for &i in devoirs.iter().take(vus) {
        let hw = &p.homework[i];
        checkbox(c, rx + 24.0, y + 10.0, hw.done);
        let col = if hw.done { t.fg3 } else { t.fg };
        let sub = c.fit(Wt::Semibold, 13.0, &hw.subject, (rw - 54.0) * 0.45);
        let sw = c.text(Wt::Semibold, 13.0, rx + 40.0, y + 14.5, col, &sub);
        let txt = c.fit(Wt::Regular, 13.0, &hw.text.replace('\n', " "), rw - 62.0 - sw);
        c.text(Wt::Regular, 13.0, rx + 48.0 + sw, y + 14.5, if hw.done { t.fg3 } else { t.fg2 }, &txt);
        c.hit(rx + 6.0, y - 3.0, rw - 12.0, 26.0, Action::IleN(N_HW, i as u16));
        y += 28.0;
    }
    if vus < n {
        c.text(Wt::Regular, 12.0, rx + 40.0, y + 14.0, t.fg3, &format!("+ {} autre{}", n - vus, if n - vus > 1 { "s" } else { "" }));
    }
    // météo du moment
    let y3 = y2 + dh + 10.0;
    let hw2 = (rw - 10.0) / 2.0;
    card(c, r, rx, y3, hw2, mh);
    caps(c, rx + 14.0, y3 + 24.0, t.fg2, "Météo");
    if let Some(wt) = &r.d.weather {
        c.icon(wt.code, wt.day, rx + 10.0, y3 + 32.0, 40.0);
        c.text(Wt::Light, 28.0, rx + 58.0, y3 + 64.0, t.fg, &temp_txt(r.c(), wt.temp));
        let ph = weather_phrase(r.c(), wt.code, wt.day, wt.temp, now.ordinal());
        let ph = c.fit(Wt::Regular, 12.0, &ph, hw2 - 28.0);
        c.text(Wt::Regular, 12.0, rx + 14.0, y3 + 92.0, t.fg2, &ph);
    } else {
        c.text(Wt::Regular, 12.5, rx + 14.0, y3 + 54.0, t.fg3, "Pas de météo pour l'instant.");
    }
    // mails non lus
    let mx = rx + hw2 + 10.0;
    card(c, r, mx, y3, hw2, mh);
    caps(c, mx + 14.0, y3 + 24.0, t.fg2, "Mails");
    if r.cfg.mail.on {
        let nw = c.text(Wt::Semibold, 30.0, mx + 14.0, y3 + 66.0, if nm > 0 { t.fg } else { t.fg3 }, &nm.to_string());
        c.text(Wt::Regular, 13.0, mx + 22.0 + nw, y3 + 66.0, t.fg2, if nm > 1 { "non lus" } else { "non lu" });
        if ni > 0 {
            c.text(Wt::Semibold, 12.0, mx + 14.0, y3 + 90.0, ORANGE, &format!("dont {ni} important{}", if ni > 1 { "s" } else { "" }));
        }
    } else {
        c.text(Wt::Regular, 12.5, mx + 14.0, y3 + 54.0, t.fg3, "Mails désactivés.");
    }
    c.hit(mx, y3, hw2, mh, Action::IleN(N_SECTION, View::Mails as u16));
    // minuteurs en cours (lecture seule : un clic sur le titre ouvre la section Minuteurs)
    let y4 = y3 + mh + 10.0;
    card(c, r, rx, y4, rw, th);
    caps(c, rx + 14.0, y4 + 24.0, t.fg2, "Minuteurs");
    c.hit(rx, y4, rw, 30.0, Action::IleN(N_SECTION, View::Timers as u16));
    let mut y = y4 + 40.0;
    if r.clocks.chrono_since.is_some() {
        c.text(Wt::Regular, 12.5, rx + 14.0, y + 13.0, t.fg2, "Chronomètre");
        c.text_r(Wt::Semibold, 15.0, rx + rw - 14.0, y + 14.0, GREEN, &fmt_dur(r.clocks.chrono().as_secs_f64().floor()));
        y += 30.0;
    }
    let rows = ((y4 + th - 8.0 - y) / 30.0).floor().max(0.0) as usize;
    for tm in r.clocks.timers.iter().rev().take(rows) {
        let run = tm.end.is_some();
        let rem = tm.remaining();
        let name = c.fit(Wt::Regular, 12.5, &tm.name, rw - 150.0);
        c.text(Wt::Regular, 12.5, rx + 14.0, y + 13.0, t.fg2, &name);
        let lab = if run { fmt_dur(rem) } else { format!("{} (pause)", fmt_dur(rem)) };
        c.text_r(Wt::Semibold, 15.0, rx + rw - 14.0, y + 14.0, if run { ORANGE } else { t.fg3 }, &lab);
        let f = if tm.total > 0 { (rem / tm.total as f64) as f32 } else { 0.0 };
        c.rr(rx + 14.0, y + 20.0, rw - 28.0, 3.0, 1.5, t.track);
        c.rr(rx + 14.0, y + 20.0, ((rw - 28.0) * f.clamp(0.0, 1.0)).max(3.0), 3.0, 1.5, ORANGE);
        y += 30.0;
    }
    if r.clocks.timers.is_empty() && r.clocks.chrono_since.is_none() {
        c.text(Wt::Regular, 12.5, rx + 14.0, y + 13.0, t.fg3, "Aucun minuteur en cours.");
    }
}

pub fn tasks_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    if absent(c, r, "taches") {
        return;
    }
    let t = c.t;
    let lists = arr(r.jd.get("taches"), "listes");
    let cur = r.ui.task_list.min(lists.len().saturating_sub(1));
    let mut x = SIDE;
    for (i, l) in lists.iter().enumerate() {
        let n = arr(Some(l), "taches").iter().filter(|t| !t["fait"].as_bool().unwrap_or(false)).count();
        let lab = format!("{} ({n})", s(l, "nom"));
        x += chip(c, x, 6.0, &lab, if i == cur { t.accent.a(0.85) } else { t.fg.a(0.1) }, if i == cur { WHITE } else { t.fg }, Some(Action::IleN(N_LIST, i as u16))) + 6.0;
        if x > w - 220.0 {
            break;
        }
    }
    x += chip(c, x, 6.0, "+ Liste", t.fg.a(0.1), t.fg2, Some(Action::IleN(N_INPUT, InputFor::NewList.code()))) + 6.0;
    if lists.len() > 1 {
        chip(c, x, 6.0, "Supprimer la liste", RED.a(0.25), t.fg, Some(Action::Ile(A_LIST_DEL)));
    }
    let ph = if r.st.typing && r.st.input_for == InputFor::NewList { "Nom de la nouvelle liste…" } else { "Ajouter une tâche… (Entrée)" };
    let kind = if r.st.input_for == InputFor::NewList && r.st.typing { InputFor::NewList } else { InputFor::Task };
    field(c, r, SIDE - 4.0, 38.0, w - 2.0 * SIDE + 8.0, kind, ph);
    let Some(list) = lists.get(cur) else { return };
    let mut items: Vec<(usize, &Value)> = arr(Some(list), "taches").into_iter().enumerate().collect();
    items.sort_by_key(|(_, t)| t["fait"].as_bool().unwrap_or(false));
    if items.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 100.0, t.fg3, "Aucune tâche dans cette liste.");
    }
    let mut y = 80.0;
    for (i, task) in items.into_iter().skip(r.st.scroll) {
        if y + 32.0 > h {
            break;
        }
        let done = task["fait"].as_bool().unwrap_or(false);
        c.rr(SIDE - 6.0, y, w - 2.0 * SIDE + 12.0, 30.0, 10.0, t.fg.a(0.05));
        checkbox(c, SIDE + 8.0, y + 15.0, done);
        c.hit(SIDE - 6.0, y, 30.0, 30.0, Action::IleN(N_TASK_TOGGLE, i as u16));
        let txt = c.fit(Wt::Regular, 13.0, s(task, "texte"), w - 2.0 * SIDE - 60.0);
        let tw = c.text(Wt::Regular, 13.0, SIDE + 26.0, y + 19.5, if done { t.fg3 } else { t.fg }, &txt);
        if done {
            c.line(SIDE + 26.0, y + 15.0, SIDE + 26.0 + tw, y + 15.0, 1.2, t.fg3);
        }
        cross(c, w - SIDE - 4.0, y + 15.0, Action::IleN(N_TASK_DEL, i as u16));
        y += 34.0;
    }
}

// ---------- Rappels ----------

pub fn reminders_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    if absent(c, r, "rappels") {
        return;
    }
    let t = c.t;
    let fw = (w * 0.5).max(220.0);
    field(c, r, SIDE - 4.0, 6.0, fw, InputFor::Reminder, "Texte du rappel… (Entrée)");
    // date et heure du nouveau rappel
    let x = SIDE + fw + 10.0;
    let when = format!("{} à {}", widgets::day_label(r.ui.rem_when.date(), r.now.date()), r.ui.rem_when.format("%H:%M"));
    c.text(Wt::Semibold, 13.0, x, 26.5, t.fg, &when);
    let mut cx = SIDE;
    for (k, lab) in ["−1 j", "+1 j", "−1 h", "+1 h", "−15 min", "+15 min"].iter().enumerate() {
        cx += chip(c, cx, 44.0, lab, t.fg.a(0.1), t.fg, Some(Action::IleN(N_REM_ADJ, k as u16))) + 6.0;
    }
    chip(c, w - SIDE - 130.0, 44.0, "Nettoyer les faits", t.fg.a(0.1), t.fg2, Some(Action::Ile(A_REM_CLEAN)));
    let list = arr(r.jd.get("rappels"), "rappels");
    if list.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 100.0, t.fg3, "Aucun rappel.");
    }
    let mut y = 80.0;
    for (i, rem) in list.iter().enumerate().skip(r.st.scroll) {
        if y + 40.0 > h {
            break;
        }
        let done = rem["fait"].as_bool().unwrap_or(false);
        c.rr(SIDE - 6.0, y, w - 2.0 * SIDE + 12.0, 36.0, 10.0, t.fg.a(0.05));
        c.text(Wt::Semibold, 11.5, SIDE + 6.0, y + 15.0, if done { t.fg3 } else { t.accent }, &quand_txt(s(rem, "quand"), r.now));
        let rep = s(rem, "repetition");
        if !rep.is_empty() {
            c.text_r(Wt::Regular, 11.0, w - SIDE - 24.0, y + 15.0, t.fg3, rep);
        }
        let txt = c.fit(Wt::Regular, 13.0, s(rem, "texte"), w - 2.0 * SIDE - 40.0);
        c.text(Wt::Regular, 13.0, SIDE + 6.0, y + 30.0, if done { t.fg3 } else { t.fg }, &txt);
        cross(c, w - SIDE - 4.0, y + 18.0, Action::IleN(N_REM_DEL, i as u16));
        y += 40.0;
    }
}

// ---------- Habitudes ----------

pub fn habits_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    if absent(c, r, "habitudes") {
        return;
    }
    let t = c.t;
    field(c, r, SIDE - 4.0, 6.0, w - 2.0 * SIDE + 8.0, InputFor::Habit, "Nouvelle habitude… (Entrée)");
    let list = arr(r.jd.get("habitudes"), "habitudes");
    if list.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 66.0, t.fg3, "Aucune habitude pour l'instant.");
    }
    let today = r.now.date();
    let mut y = 50.0;
    for (i, hb) in list.iter().enumerate().skip(r.st.scroll) {
        if y + 40.0 > h {
            break;
        }
        c.rr(SIDE - 6.0, y, w - 2.0 * SIDE + 12.0, 36.0, 10.0, t.fg.a(0.05));
        let on = hb["aujourdhui"].as_bool().unwrap_or(false);
        checkbox(c, SIDE + 8.0, y + 18.0, on);
        c.hit(SIDE - 6.0, y, 30.0, 36.0, Action::IleN(N_HAB_TOGGLE, i as u16));
        let txt = c.fit(Wt::Semibold, 13.5, s(hb, "nom"), w * 0.4);
        c.text(Wt::Semibold, 13.5, SIDE + 26.0, y + 22.5, t.fg, &txt);
        // les 7 derniers jours
        let jours: Vec<&str> = hb["jours"].as_array().map(|a| a.iter().filter_map(|j| j.as_str()).collect()).unwrap_or_default();
        let dx0 = w - SIDE - 230.0;
        for k in 0..7 {
            let d = today - chrono::Duration::days(6 - k);
            let fait = jours.contains(&d.format("%Y-%m-%d").to_string().as_str());
            let cx = dx0 + k as f32 * 18.0;
            if fait { c.circle(cx, y + 18.0, 6.0, GREEN) } else { c.ring(cx, y + 18.0, 5.5, t.fg3, 1.2) }
        }
        let serie = hb["serie"].as_u64().unwrap_or(0);
        flame(c, w - SIDE - 82.0, y + 18.0);
        c.text(Wt::Semibold, 12.5, w - SIDE - 72.0, y + 22.5, if serie > 0 { ORANGE } else { t.fg3 }, &format!("{serie} j"));
        cross(c, w - SIDE - 4.0, y + 18.0, Action::IleN(N_HAB_DEL, i as u16));
        y += 40.0;
    }
}

// ---------- Copiés ----------

pub fn copies_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    caps(c, SIDE, 22.0, t.fg2, "Presse-papiers (Jarvis)");
    chip(c, w - SIDE - 100.0, 6.0, "Tout effacer", t.fg.a(0.12), t.fg, Some(Action::Ile(A_COPIES_CLEAR)));
    let list = arr(r.jd.get("copies"), "copies");
    if list.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 56.0, t.fg3, "Rien copié pour l'instant.");
    }
    let mut y = 40.0;
    for (i, x) in list.iter().enumerate().skip(r.st.scroll) {
        if y + 34.0 > h {
            break;
        }
        c.rr(SIDE - 6.0, y, w - 2.0 * SIDE + 12.0, 30.0, 10.0, t.fg.a(0.06));
        let one = s(x, "texte").replace(['\r', '\n', '\t'], " ");
        let txt = c.fit(Wt::Regular, 12.5, &one, w - 2.0 * SIDE - 40.0);
        c.text(Wt::Regular, 12.5, SIDE + 4.0, y + 19.5, t.fg, &txt);
        c.hit(SIDE - 6.0, y, w - 2.0 * SIDE - 20.0, 30.0, Action::IleN(N_COPY, i as u16));
        cross(c, w - SIDE - 4.0, y + 15.0, Action::IleN(N_COPY_DEL, i as u16));
        y += 34.0;
    }
}

// ---------- Historique des conversations ----------

/// Les échanges avec Jarvis (le plus récent en haut) : ta phrase, puis sa réponse. Molette = remonter le temps.
pub fn history_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    if absent(c, r, "historique") {
        return;
    }
    let t = c.t;
    let v = r.jd.get("historique");
    let total = v.and_then(|v| v["total"].as_u64()).unwrap_or(0);
    caps(c, SIDE, 22.0, t.fg2, &format!("Conversations avec Jarvis ({total})"));
    if total > 0 {
        chip(c, w - SIDE - 100.0, 6.0, "Tout effacer", t.fg.a(0.12), t.fg, Some(Action::Ile(A_HIST_CLEAR)));
    }
    let list = arr(v, "echanges");
    if list.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 56.0, t.fg3, "Rien pour l'instant : parle à Jarvis, vos échanges s'afficheront ici.");
        return;
    }
    let tw = w - 2.0 * SIDE - 16.0;
    let mut y = 40.0;
    let mut jour = String::new();
    for x in list.iter().skip(r.st.scroll) {
        let date = s(x, "date");
        let (d, heure) = date.split_once(' ').unwrap_or((date, ""));
        if d != jour {
            if y + 24.0 > h {
                break;
            }
            jour = d.to_string();
            let titre = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").map(|n| n.format("%d/%m/%Y").to_string()).unwrap_or_else(|_| d.to_string());
            c.text(Wt::Semibold, 12.0, SIDE, y + 14.0, t.accent, &titre);
            y += 22.0;
        }
        let q = c.wrap(Wt::Semibold, 12.5, &format!("Toi : {}", s(x, "question").replace(['\r', '\n'], " ")), tw - 50.0);
        let rep = c.wrap(Wt::Regular, 12.5, &format!("Jarvis : {}", s(x, "reponse").replace(['\r', '\n'], " ")), tw);
        let (nq, nr) = (q.len().min(2), rep.len().min(3));
        let bh = 14.0 + (nq + nr) as f32 * 16.0;
        if y + bh > h {
            break;
        }
        c.rr(SIDE - 6.0, y, w - 2.0 * SIDE + 12.0, bh, 10.0, t.fg.a(0.06));
        c.text_r(Wt::Regular, 11.0, w - SIDE - 2.0, y + 17.0, t.fg3, heure);
        let mut ly = y + 17.0;
        for (k, l) in q.iter().take(nq).enumerate() {
            let l = if k + 1 == nq && q.len() > nq { format!("{l}…") } else { l.clone() };
            c.text(Wt::Semibold, 12.5, SIDE + 4.0, ly, t.fg, &l);
            ly += 16.0;
        }
        for (k, l) in rep.iter().take(nr).enumerate() {
            let l = if k + 1 == nr && rep.len() > nr { format!("{l}…") } else { l.clone() };
            c.text(Wt::Regular, 12.5, SIDE + 4.0, ly, t.fg2, &l);
            ly += 16.0;
        }
        y += bh + 6.0;
    }
}

// ---------- Routines ----------

pub fn routines_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    if absent(c, r, "routines") {
        return;
    }
    let t = c.t;
    let list = arr(r.jd.get("routines"), "routines");
    let mut y = 6.0;
    let mut h = h;
    if is_big(r) {
        // formulaire « nouvelle routine » en bas du panneau
        let fy = h - 150.0;
        h = fy - 8.0;
        card(c, r, SIDE - 10.0, fy, w - 2.0 * SIDE + 20.0, 146.0);
        caps(c, SIDE, fy + 22.0, t.fg2, "Nouvelle routine");
        let half = (w - 2.0 * SIDE - 10.0) / 2.0;
        let ui = r.ui;
        let val = |kind: InputFor, v: &str, ph: &'static str| if v.is_empty() || (r.st.typing && r.st.input_for == kind) { ph.to_string() } else { v.to_string() };
        field(c, r, SIDE - 4.0, fy + 32.0, half, InputFor::RoutineName, &val(InputFor::RoutineName, &ui.rt_nom, "Nom (ex : Mode devoirs)"));
        field(c, r, SIDE + half + 6.0, fy + 32.0, half, InputFor::RoutineTrigger, &val(InputFor::RoutineTrigger, &ui.rt_decl, "Phrase qui la lance (ex : mode devoirs)"));
        field(c, r, SIDE - 4.0, fy + 70.0, 160.0, InputFor::RoutineTime, &val(InputFor::RoutineTime, &ui.rt_heure, "Heure (facultatif)"));
        field(c, r, SIDE + 162.0, fy + 70.0, w - 2.0 * SIDE - 158.0, InputFor::RoutineStep, "Ajouter une étape (Entrée), ex : Lance un minuteur de 25 minutes");
        let steps = if ui.rt_etapes.is_empty() { "Aucune étape pour l'instant.".to_string() } else { ui.rt_etapes.iter().enumerate().map(|(i, e)| format!("{}. {e}", i + 1)).collect::<Vec<_>>().join("   ") };
        let steps = c.fit(Wt::Regular, 12.0, &steps, w - 2.0 * SIDE - 220.0);
        c.text(Wt::Regular, 12.0, SIDE, fy + 128.0, t.fg2, &steps);
        let bx = w - SIDE - 200.0;
        let bw = chip(c, bx, fy + 112.0, "Effacer", t.fg.a(0.1), t.fg2, Some(Action::Ile(A_ROUT_CLEAR)));
        chip(c, bx + bw + 6.0, fy + 112.0, "Enregistrer", t.accent.a(0.9), WHITE, Some(Action::Ile(A_ROUT_SAVE)));
    }
    if list.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 40.0, t.fg3, "Aucune routine pour l'instant.");
    }
    for (i, ro) in list.iter().enumerate().skip(r.st.scroll) {
        if y + 60.0 > h {
            break;
        }
        c.rr(SIDE - 6.0, y, w - 2.0 * SIDE + 12.0, 56.0, 12.0, t.fg.a(0.05));
        c.text(Wt::Semibold, 14.0, SIDE + 6.0, y + 21.0, if ro["active"].as_bool() == Some(false) { t.fg3 } else { t.fg }, s(ro, "nom"));
        let mut info = String::new();
        if !s(ro, "declencheur").is_empty() {
            info = format!("« {} »", s(ro, "declencheur"));
        }
        if !s(ro, "heure").is_empty() {
            info = format!("{info}  ·  à {}", s(ro, "heure"));
        }
        let iw = c.measure(Wt::Semibold, 14.0, s(ro, "nom"));
        let info = c.fit(Wt::Regular, 12.0, &info, w - 2.0 * SIDE - iw - 120.0);
        c.text(Wt::Regular, 12.0, SIDE + 16.0 + iw, y + 21.0, t.fg2, &info);
        let etapes: Vec<&str> = ro["etapes"].as_array().map(|a| a.iter().filter_map(|e| e.as_str()).collect()).unwrap_or_default();
        let et = c.fit(Wt::Regular, 12.0, &format!("{} étape{} : {}", etapes.len(), if etapes.len() > 1 { "s" } else { "" }, etapes.join(" → ")), w - 2.0 * SIDE - 100.0);
        c.text(Wt::Regular, 12.0, SIDE + 6.0, y + 42.0, t.fg3, &et);
        let lx = if is_big(r) { w - SIDE - 104.0 } else { w - SIDE - 70.0 };
        chip(c, lx, y + 16.0, "Lancer", t.accent.a(0.85), WHITE, Some(Action::IleN(N_ROUT_RUN, i as u16)));
        if is_big(r) {
            cross(c, w - SIDE - 4.0, y + 28.0, Action::IleN(N_ROUT_DEL, i as u16));
        }
        y += 62.0;
    }
}

// ---------- Mémoire (carte mentale) ----------

fn node_color(typ: &str, t: &crate::gfx::Theme) -> Rgba {
    match typ {
        "moi" => t.accent,
        "personne" => Rgba::rgb8(10, 132, 255),
        "preference" => ORANGE,
        "lieu" => GREEN,
        "activite" | "activité" => Rgba::rgb8(175, 82, 222),
        _ => Rgba::rgb8(142, 142, 147),
    }
}

pub fn memory_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    if absent(c, r, "memoire") {
        return;
    }
    if r.ui.mem_list {
        memory_list(c, r, w, h);
        return;
    }
    let t = c.t;
    let v = r.jd.get("memoire");
    let top = 4.0;
    let area_h = h - 48.0;
    c.rr(SIDE - 10.0, top, w - 2.0 * SIDE + 20.0, area_h, 14.0, t.fg.a(0.04));
    let (aw, ah) = (w - 2.0 * SIDE + 20.0, area_h);
    let (ax, ay) = (SIDE - 10.0, top);
    r.mem.view.set((c.ox + ax * c.s, c.oy + ay * c.s, c.s, aw, ah));
    let g = r.mem;
    let to = |p: &[f32; 4]| (ax + aw / 2.0 + g.pan.0 + p[0] * g.zoom, ay + ah / 2.0 + g.pan.1 + p[1] * g.zoom);
    let inside = |x: f32, y: f32| x > ax + 4.0 && x < ax + aw - 4.0 && y > ay + 4.0 && y < ay + ah - 4.0;
    for l in arr(v, "liens") {
        if let (Some(a), Some(b)) = (g.pos.get(s(l, "a")), g.pos.get(s(l, "b"))) {
            let ((x1, y1), (x2, y2)) = (to(a), to(b));
            if inside(x1, y1) && inside(x2, y2) {
                c.line(x1, y1, x2, y2, 1.2, t.fg.a(0.22));
            }
        }
    }
    // les nœuds d'abord, puis les étiquettes sans chevauchement (dessous, dessus, à droite, à gauche)
    let mut placed: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut labels: Vec<(f32, f32, f32, String, bool)> = Vec::new();
    for n in arr(v, "noeuds") {
        let id = s(n, "id");
        let Some(p) = g.pos.get(id) else { continue };
        let (x, y) = to(p);
        if !inside(x, y) {
            continue;
        }
        let me = id == "moi";
        let col = node_color(s(n, "type"), t);
        let rad = if me { 14.0 } else { 8.0 } * g.zoom.clamp(0.6, 1.6);
        if g.sel.as_deref() == Some(id) {
            c.circle(x, y, rad + 5.0, col.a(0.3));
        }
        c.circle(x, y, rad, col);
        placed.push((x - rad, y - rad, 2.0 * rad, 2.0 * rad));
        labels.push((x, y, rad, s(n, "nom").to_string(), me || g.sel.as_deref() == Some(id)));
    }
    // étiquettes importantes (Moi, sélection) en premier
    labels.sort_by_key(|l| !l.4);
    let fs = if g.zoom < 0.8 { 10.0 } else { 11.0 };
    for (x, y, rad, nom, _) in labels {
        let name = c.fit(Wt::Regular, fs, &nom, 130.0);
        let nw = c.measure(Wt::Regular, fs, &name);
        let lh = fs + 4.0;
        let spots = [
            (x - nw / 2.0, y + rad + 3.0),
            (x - nw / 2.0, y - rad - 3.0 - lh),
            (x + rad + 4.0, y - lh / 2.0),
            (x - rad - 4.0 - nw, y - lh / 2.0),
        ];
        let free = |bx: f32, by: f32| placed.iter().all(|(px, py, pw, ph)| bx + nw < *px || *px + pw < bx || by + lh < *py || *py + ph < by);
        if let Some(&(bx, by)) = spots.iter().find(|(bx, by)| inside(*bx, *by) && inside(bx + nw, by + lh) && free(*bx, *by)) {
            c.text(Wt::Regular, fs, bx, by + fs, t.fg, &name);
            placed.push((bx - 2.0, by, nw + 4.0, lh));
        }
    }
    let nb = arr(v, "noeuds").len();
    let k = arr(v, "suggestions").len();
    let a_trier = if k > 0 { format!(" · {k} à trier") } else { String::new() };
    c.text(Wt::Regular, 11.0, ax + 10.0, ay + 18.0, t.fg3, &format!("{nb} souvenir{}{a_trier} · glisser = déplacer · molette = zoom", if nb > 1 { "s" } else { "" }));
    let by = h - 38.0;
    let mut fx = SIDE - 4.0;
    fx += chip(c, fx, by + 4.0, "Trier la mémoire", t.accent.a(0.85), WHITE, Some(Action::Ile(A_MEM_MODE))) + 6.0;
    if let Some(sel) = &g.sel {
        if sel != "moi" {
            let (lab, bg) = if r.ui.mem_confirming() { ("Confirmer ?", RED) } else { ("Supprimer", RED.a(0.3)) };
            fx += chip(c, fx, by + 4.0, lab, bg, t.fg, Some(Action::Ile(A_MEM_DEL))) + 6.0;
        }
        let lab = if r.ui.link_from.is_some() { "Annuler le lien" } else { "Relier" };
        fx += chip(c, fx, by + 4.0, lab, t.accent.a(0.3), t.fg, Some(Action::Ile(A_MEM_LINK))) + 6.0;
    }
    if r.ui.link_from.is_some() && r.ui.link_to.is_none() {
        c.text(Wt::Semibold, 12.0, ax + 10.0, ay + 36.0, t.accent, "Clique sur le 2e souvenir à relier…");
    }
    if r.ui.link_to.is_some() {
        field(c, r, fx, by, w - SIDE - fx + 4.0, InputFor::Relation, "Relation (ex : aime, est l'ami de)…");
        return;
    }
    fx += chip(c, fx, by + 4.0, "Recentrer", t.fg.a(0.1), t.fg2, Some(Action::Ile(A_MEM_CENTER))) + 6.0;
    field(c, r, fx, by, w - SIDE - fx + 4.0, InputFor::Memory, "Retenir un souvenir… (ex : Lucas est mon meilleur ami)");
}

/// Petite punaise « garder » : pleine si le souvenir est gardé.
fn pin(c: &mut Ctx, cx: f32, cy: f32, on: bool) {
    let t = c.t;
    let col = if on { t.accent } else { t.fg3 };
    c.line(cx, cy, cx, cy + 8.0, 1.6, col);
    if on {
        c.circle(cx, cy - 2.0, 5.0, col);
    } else {
        c.ring(cx, cy - 2.0, 4.5, col, 1.4);
    }
}

/// Mémoire en liste : filtrer, chercher, trier, cocher, supprimer (avec confirmation), garder, renommer.
fn memory_list(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let ui = r.ui;
    let v = r.jd.get("memoire");
    let rows = mem_rows(v, ui.mem_filter, ui.mem_sort, &ui.mem_search);
    // barre d'outils : carte, filtre, ordre, recherche
    let mut x = SIDE - 4.0;
    x += chip(c, x, 8.0, "Carte", t.fg.a(0.12), t.fg, Some(Action::Ile(A_MEM_MODE))) + 6.0;
    let filtre = format!("Filtre : {}", MEM_FILTERS[ui.mem_filter as usize % MEM_FILTERS.len()].0);
    x += chip(c, x, 8.0, &filtre, if ui.mem_filter == 0 { t.fg.a(0.12) } else { t.accent.a(0.35) }, t.fg, Some(Action::Ile(A_MEM_FILTER))) + 6.0;
    let tri = format!("Tri : {}", MEM_SORTS[ui.mem_sort as usize % MEM_SORTS.len()]);
    x += chip(c, x, 8.0, &tri, t.fg.a(0.12), t.fg, Some(Action::Ile(A_MEM_SORT))) + 6.0;
    let typing_search = r.st.typing && r.st.input_for == InputFor::MemSearch;
    if !ui.mem_search.is_empty() && !typing_search {
        let lab = format!("× « {} »", c.fit(Wt::Regular, 12.0, &ui.mem_search, 90.0));
        x += chip(c, x, 8.0, &lab, t.accent.a(0.35), t.fg, Some(Action::Ile(A_MEM_SEARCH_CLEAR))) + 6.0;
    }
    field(c, r, x, 4.0, (w - SIDE - x + 4.0).max(120.0), InputFor::MemSearch, "Chercher un souvenir…");
    // résumé + annulation
    let nb = arr(v, "noeuds").len().saturating_sub(1);
    let k = arr(v, "suggestions").len();
    let info = format!("{} affiché{} sur {nb} · {k} à trier · {} coché{}", rows.len(), if rows.len() > 1 { "s" } else { "" }, ui.mem_checked.len(), if ui.mem_checked.len() > 1 { "s" } else { "" });
    c.text(Wt::Regular, 12.0, SIDE, 58.0, t.fg2, &info);
    if let Some(quand) = v.and_then(|v| v["sauvegarde"].as_str()) {
        let lab = format!("Annuler le nettoyage ({quand})");
        let lw = c.measure(Wt::Semibold, 12.0, &lab) + 24.0;
        chip(c, w - SIDE - lw + 4.0, 42.0, &lab, ORANGE.a(0.3), t.fg, Some(Action::Ile(A_MEM_UNDO)));
    }
    // lignes
    let bottom = h - 46.0 - if ui.msg.is_empty() { 0.0 } else { 20.0 };
    let mut y = 72.0;
    if rows.is_empty() {
        let vide = if ui.mem_filter == 1 { "Rien à trier : ta mémoire est bien rangée." } else { "Aucun souvenir ici." };
        c.text(Wt::Regular, 13.0, SIDE, y + 20.0, t.fg3, vide);
    }
    let rw = w - 2.0 * SIDE + 12.0;
    for (i, n) in rows.iter().enumerate().skip(r.st.scroll) {
        if y + 30.0 > bottom {
            break;
        }
        let id = s(n, "id");
        let on = ui.mem_checked.iter().any(|x| x == id);
        let garde = n["garde"].as_bool() == Some(true);
        c.rr(SIDE - 6.0, y, rw, 30.0, 9.0, if on { RED.a(0.14) } else { t.fg.a(0.05) });
        c.hit(SIDE - 6.0, y, rw - 34.0, 30.0, Action::IleN(N_MEM_ROW, i as u16));
        checkbox(c, SIDE + 8.0, y + 15.0, on);
        c.circle(SIDE + 28.0, y + 15.0, 5.0, node_color(s(n, "type"), t));
        let reason = mem_reason(v, id);
        let rsw = if reason.is_empty() { 0.0 } else { (c.measure(Wt::Regular, 11.0, reason) + 16.0).min(240.0) };
        let nom = c.fit(Wt::Semibold, 13.0, s(n, "nom"), (rw * 0.45).min(rw - rsw - 200.0).max(80.0));
        let nw = c.text(Wt::Semibold, 13.0, SIDE + 40.0, y + 19.5, t.fg, &nom);
        let liens = n["nb_liens"].as_u64().unwrap_or(0);
        let util = n["utilise"].as_u64().unwrap_or(0);
        let typ = match s(n, "type") {
            "personne" => "personne",
            "preference" => "goût",
            "lieu" => "lieu",
            "activite" | "activité" => "activité",
            "ecole" => "école",
            "evenement" => "événement",
            "habitude" => "habitude",
            "projet" => "projet",
            "objet" => "objet",
            _ => "autre",
        };
        let mut det = format!("{typ} · {liens} lien{}", if liens > 1 { "s" } else { "" });
        if util > 0 {
            det = format!("{det} · utilisé {util}×");
        }
        let det = c.fit(Wt::Regular, 11.0, &det, (rw - nw - rsw - 110.0).max(0.0));
        c.text(Wt::Regular, 11.0, SIDE + 50.0 + nw, y + 19.0, t.fg3, &det);
        if !reason.is_empty() {
            let rx = SIDE - 6.0 + rw - 40.0 - rsw;
            c.rr(rx, y + 6.0, rsw, 18.0, 9.0, ORANGE.a(0.25));
            let rt = c.fit(Wt::Regular, 11.0, reason, rsw - 12.0);
            c.text(Wt::Regular, 11.0, rx + 8.0, y + 19.0, t.fg, &rt);
        }
        let px = SIDE - 6.0 + rw - 18.0;
        pin(c, px, y + 13.0, garde);
        c.hit(px - 16.0, y, 32.0, 30.0, Action::IleN(N_MEM_PIN, i as u16));
        y += 34.0;
    }
    if !ui.msg.is_empty() {
        let m = c.fit(Wt::Regular, 12.0, &ui.msg, w - 2.0 * SIDE);
        c.text(Wt::Regular, 12.0, SIDE, h - 50.0, t.fg2, &m);
    }
    // barre du bas
    let by = h - 38.0;
    let mut fx = SIDE - 4.0;
    if ui.mem_edit.is_some() {
        field(c, r, fx, by, w - SIDE - fx + 4.0, InputFor::MemRename, "Nouveau nom du souvenir (Entrée)…");
        return;
    }
    let n = ui.mem_checked.len();
    if n > 0 {
        let (lab, bg) = if ui.mem_confirming() { (format!("Confirmer : supprimer {n} ?"), RED) } else { (format!("Supprimer ({n})"), RED.a(0.3)) };
        fx += chip(c, fx, by + 4.0, &lab, bg, t.fg, Some(Action::Ile(A_MEM_DEL_SEL))) + 6.0;
        fx += chip(c, fx, by + 4.0, "Garder", t.accent.a(0.3), t.fg, Some(Action::Ile(A_MEM_KEEP))) + 6.0;
        fx += chip(c, fx, by + 4.0, "Changer la couleur", t.fg.a(0.12), t.fg, Some(Action::Ile(A_MEM_TYPE))) + 6.0;
        if n == 1 {
            fx += chip(c, fx, by + 4.0, "Renommer", t.fg.a(0.12), t.fg, Some(Action::Ile(A_MEM_RENAME))) + 6.0;
        }
        chip(c, fx, by + 4.0, "Tout décocher", t.fg.a(0.08), t.fg2, Some(Action::Ile(A_MEM_ALL)));
    } else {
        if k > 0 {
            fx += chip(c, fx, by + 4.0, &format!("Nettoyer les suggestions ({k})"), ORANGE.a(0.85), WHITE, Some(Action::Ile(A_MEM_SUGG))) + 6.0;
        }
        if !rows.is_empty() {
            chip(c, fx, by + 4.0, "Tout cocher", t.fg.a(0.1), t.fg2, Some(Action::Ile(A_MEM_ALL)));
        }
        c.text_r(Wt::Regular, 11.0, w - SIDE, by + 21.0, t.fg3, "Punaise = garder pour toujours");
    }
}

// ---------- Musique (volume + Deezer) ----------

pub fn music_extra(c: &mut Ctx, r: &R, x: f32, y: f32, w: f32) {
    let t = c.t;
    let vol = r.jd.get("musique").and_then(|v| v["volume"].as_f64());
    caps(c, x, y + 4.0, t.fg2, "Volume");
    let bx = x + 80.0;
    let bw = (w - bx - SIDE - 20.0).clamp(160.0, 360.0);
    c.rr(bx, y - 4.0, bw, 10.0, 5.0, t.track);
    if let Some(v) = vol {
        c.rr(bx, y - 4.0, (bw * v as f32 / 100.0).max(10.0), 10.0, 5.0, t.accent);
        c.text(Wt::Semibold, 12.0, bx + bw + 10.0, y + 5.0, t.fg, &format!("{} %", v.round()));
    } else {
        c.text(Wt::Regular, 12.0, bx + bw + 10.0, y + 5.0, t.fg3, "—");
    }
    for k in 0..=10 {
        c.hit(bx + bw * (k as f32 - 0.5) / 10.0, y - 12.0, bw / 10.0, 26.0, Action::IleN(N_VOL, k * 10));
    }
    field(c, r, x - 4.0, y + 22.0, (w - 2.0 * SIDE).min(520.0), InputFor::Deezer, "Chercher sur Deezer… (Entrée)");
    if !r.ui.msg.is_empty() {
        let m = c.fit(Wt::Regular, 12.0, &r.ui.msg, w - 2.0 * SIDE);
        c.text(Wt::Regular, 12.0, x, y + 74.0, t.fg2, &m);
    }
}

// ---------- Batteries ----------

pub fn batteries_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    if absent(c, r, "appareils") {
        return;
    }
    let t = c.t;
    let list = arr(r.jd.get("appareils"), "appareils");
    if list.is_empty() {
        c.text(Wt::Semibold, 14.0, SIDE, 40.0, t.fg, "Aucun appareil avec batterie détecté.");
        c.text(Wt::Regular, 12.5, SIDE, 62.0, t.fg2, "Manettes Xbox, casques et souris Bluetooth apparaissent ici.");
        return;
    }
    let mut y = 20.0;
    for a in list {
        if y + 44.0 > h {
            break;
        }
        let b = a["batterie"].as_f64();
        let typ = s(a, "type");
        let lab = format!("{}{}", s(a, "nom"), if typ.is_empty() { String::new() } else { format!("  ·  {typ}") });
        match b {
            Some(b) => {
                let charge = if a["branche"].as_bool() == Some(true) { "en charge · " } else { "" };
                gauge(c, SIDE, y + 12.0, w - 2.0 * SIDE, &lab, b as f32 / 100.0, &format!("{charge}{} %", b.round()));
            }
            None => {
                c.text(Wt::Semibold, 12.5, SIDE, y + 12.0, t.fg, &lab);
                c.text_r(Wt::Regular, 12.0, w - SIDE, y + 12.0, t.fg3, "batterie inconnue");
            }
        }
        y += 44.0;
    }
}

// ---------- Imprimante 3D ----------

fn ligne_kv(c: &mut Ctx, x0: f32, x1: f32, y: f32, k: &str, v: &str) {
    let t = c.t;
    c.text(Wt::Regular, 12.5, x0, y, t.fg2, k);
    c.text_r(Wt::Semibold, 12.5, x1, y, t.fg, v);
}

pub fn printer_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    use crate::imprimante as imp;
    let t = c.t;
    let pc = &r.cfg.printer;
    let p = &r.d.printer;
    let a = |k: u16| Some(Action::IleN(N_PRINTER, k));
    let gris = t.fg.a(0.12);
    // en bas : les réglages rapides
    let yb = h - 34.0;
    let mut x = SIDE;
    for (k, lab, on) in [(5u16, "Dans la pilule", pc.pilule), (6, "Alarme de fin", pc.alarme), (7, "Jarvis le dit", pc.voix)] {
        let txt = format!("{lab} : {}", if on { "oui" } else { "non" });
        x += chip(c, x, yb, &txt, if on { t.accent.a(0.28) } else { t.fg.a(0.08) }, if on { t.fg } else { t.fg2 }, a(k)) + 8.0;
    }
    x += chip(c, x, yb, "Chercher l'imprimante", gris, t.fg, a(3)) + 8.0;
    x += chip(c, x, yb, "Adresse…", gris, t.fg, a(8)) + 12.0;
    if !r.ui.printer_msg.is_empty() {
        let m = c.fit(Wt::Regular, 12.0, &r.ui.printer_msg, (w - SIDE - x).max(40.0));
        c.text_r(Wt::Regular, 12.0, w - SIDE, yb + 16.5, t.fg2, &m);
    }
    if !pc.on {
        c.text(Wt::Semibold, 16.0, SIDE, 40.0, t.fg, "L'imprimante 3D est éteinte dans Jarvis.");
        c.text(Wt::Regular, 12.5, SIDE, 64.0, t.fg2, "Allume-la pour voir l'avancement, la caméra, et sonner à la fin d'une impression.");
        chip(c, SIDE, 84.0, "Allumer", t.accent.a(0.9), WHITE, a(9));
        return;
    }
    if pc.ip.trim().is_empty() {
        c.text(Wt::Semibold, 16.0, SIDE, 40.0, t.fg, "Je cherche ton imprimante sur le Wi-Fi…");
        c.text(Wt::Regular, 12.5, SIDE, 64.0, t.fg2, "Elle doit être allumée et sur le même Wi-Fi que ce PC (Elegoo Centauri Carbon, Saturn, Mars…).");
        let bw = chip(c, SIDE, 84.0, "Chercher", t.accent.a(0.9), WHITE, a(3));
        chip(c, SIDE + bw + 8.0, 84.0, "Taper son adresse", gris, t.fg, a(8));
        return;
    }
    // à gauche : la caméra
    let cw = ((w - 2.0 * SIDE) * 0.6).round();
    let chh = (cw * 9.0 / 16.0).min(yb - 30.0).max(120.0);
    let (cx, cy) = (SIDE, 12.0);
    c.rr(cx, cy, cw, chh, 16.0, t.fg.a(0.07));
    match &r.ui.printer_cam {
        Some(img) if p.online => island_cards::cover(c, img, cx, cy, cw, chh, 16.0),
        _ => {
            let txt = if p.online { "Caméra : connexion…" } else { "Caméra indisponible" };
            c.text_c(Wt::Regular, 12.5, cx + cw / 2.0, cy + chh / 2.0 + 4.0, t.fg3, txt);
        }
    }
    if p.online {
        let lab = if p.light { "Lumière allumée" } else { "Lumière éteinte" };
        chip(c, cx + 10.0, cy + chh - 34.0, lab, Rgba(0.0, 0.0, 0.0, 0.55), WHITE, a(2));
    }
    // à droite : l'état
    let x0 = cx + cw + 24.0;
    let x1 = w - SIDE;
    let lw = (x1 - x0).max(60.0);
    let nom = if p.name.is_empty() { "Imprimante 3D".to_string() } else { p.name.clone() };
    let nom = c.fit(Wt::Semibold, 11.0, &nom.to_uppercase(), lw);
    caps(c, x0, 26.0, t.fg2, &nom);
    if !p.online {
        c.text(Wt::Semibold, 20.0, x0, 58.0, t.fg, "Hors ligne");
        let l1 = c.fit(Wt::Regular, 12.5, &format!("Pas de réponse de {}.", pc.ip.trim()), lw);
        c.text(Wt::Regular, 12.5, x0, 82.0, t.fg2, &l1);
        c.text(Wt::Regular, 12.5, x0, 102.0, t.fg2, "Elle est éteinte ? Sinon :");
        chip(c, x0, 114.0, "Chercher l'imprimante", t.accent.a(0.9), WHITE, a(3));
        return;
    }
    let actif = imp::is_active(p.status);
    let col = if imp::en_pause(p.status) { ORANGE } else if p.status == 9 { GREEN } else if actif { t.accent } else { t.fg };
    c.text(Wt::Semibold, 20.0, x0, 58.0, col, imp::status_label(p.status));
    let mut y = 82.0;
    if actif || p.status == 9 {
        let f = c.fit(Wt::Regular, 12.5, &imp::nom_fichier(&p.file), lw);
        c.text(Wt::Regular, 12.5, x0, y, t.fg2, &f);
        c.text(Wt::Light, 42.0, x0, y + 50.0, t.fg, &format!("{} %", p.progress.clamp(0, 100)));
        let k = (p.progress.clamp(0, 100) as f32) / 100.0;
        c.rr(x0, y + 64.0, lw, 8.0, 4.0, t.track);
        if k > 0.0 {
            c.rr(x0, y + 64.0, (lw * k).max(8.0), 8.0, 4.0, col);
        }
        y += 98.0;
        if p.total > 0 {
            ligne_kv(c, x0, x1, y, "Couche", &format!("{} / {}", p.layer, p.total));
            y += 22.0;
        }
        if actif {
            ligne_kv(c, x0, x1, y, "Reste", &imp::duree(p.left_min));
            let fin = (r.now + chrono::Duration::minutes(p.left_min)).format("%H:%M").to_string();
            ligne_kv(c, x0, x1, y + 22.0, "Fin vers", &fin);
            y += 44.0;
        }
        if p.elapsed_min > 0 {
            ligne_kv(c, x0, x1, y, "Depuis le début", &imp::duree(p.elapsed_min));
            y += 22.0;
        }
    } else {
        c.text(Wt::Regular, 12.5, x0, y, t.fg2, "Prête pour la prochaine impression.");
        y += 22.0;
    }
    y += 14.0;
    caps(c, x0, y, t.fg2, "Températures");
    y += 22.0;
    let deg = |v: i64, cible: i64| if cible > 0 { format!("{v} / {cible} °C") } else { format!("{v} °C") };
    ligne_kv(c, x0, x1, y, "Buse", &deg(p.nozzle, p.nozzle_target));
    ligne_kv(c, x0, x1, y + 22.0, "Plateau", &deg(p.bed, p.bed_target));
    if p.box_temp > 0 {
        ligne_kv(c, x0, x1, y + 44.0, "Caisson", &format!("{} °C", p.box_temp));
        y += 22.0;
    }
    y += 58.0;
    // les boutons
    let mut bx = x0;
    if actif {
        let (lab, k) = if imp::en_pause(p.status) { ("Reprendre", 0) } else { ("Pause", 0) };
        bx += chip(c, bx, y, lab, t.accent.a(0.9), WHITE, a(k)) + 8.0;
        let stop = if r.ui.printer_stopping() { "Sûr ? Re-clique" } else { "Arrêter" };
        bx += chip(c, bx, y, stop, RED.a(if r.ui.printer_stopping() { 1.0 } else { 0.75 }), WHITE, a(1)) + 8.0;
    }
    if bx + 140.0 > x1 {
        bx = x0;
        y += 32.0;
    }
    chip(c, bx, y, "Ouvrir le slicer", gris, t.fg, a(4));
}

// ---------- Mon PC ----------

fn go(b: f64) -> String {
    format!("{:.1} Go", b / 1_073_741_824.0)
}

pub fn pc_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    if absent(c, r, "pc") {
        return;
    }
    let t = c.t;
    let Some(v) = r.jd.get("pc") else { return };
    let num = |k: &str| v[k].as_f64().unwrap_or(0.0);
    // jauges rondes
    let mut items: Vec<(String, f32, String)> = vec![
        ("Processeur".into(), num("cpu") as f32 / 100.0, format!("{} % · {} cœurs", num("cpu").round(), num("coeurs"))),
        ("Mémoire".into(), num("ram_pourcent") as f32 / 100.0, format!("{} / {}", go(num("ram_utilisee")), go(num("ram_totale")))),
    ];
    if let Some(g) = v["gpu"].as_object() {
        let u = g.get("utilisation").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let temp = g.get("temperature").and_then(|x| x.as_f64()).unwrap_or(0.0);
        items.push(("Carte graphique".into(), u as f32 / 100.0, format!("{} % · {} °C", u.round(), temp.round())));
    }
    let dt = num("disque_total").max(1.0);
    items.push(("Disque C:".into(), (num("disque_utilise") / dt) as f32, format!("{} / {}", go(num("disque_utilise")), go(dt))));
    let n = items.len() as f32;
    let cw = (w - 2.0 * SIDE) / n;
    let big = h > 300.0;
    let rad = if big { 46.0 } else { 30.0 };
    for (i, (name, f, txt)) in items.iter().enumerate() {
        let cx = SIDE + cw * (i as f32 + 0.5);
        let cy = 14.0 + rad;
        let col = if *f > 0.85 { RED } else if *f > 0.65 { ORANGE } else { GREEN };
        c.ring(cx, cy, rad, t.track, if big { 8.0 } else { 6.0 });
        arc(c, cx, cy, rad, *f, col);
        c.text_c(Wt::Semibold, if big { 20.0 } else { 14.0 }, cx, cy + 6.0, t.fg, &format!("{}%", (f * 100.0).round()));
        c.text_c(Wt::Semibold, 12.5, cx, cy + rad + 22.0, t.fg, name);
        let tt = c.fit(Wt::Regular, 11.0, txt, cw - 8.0);
        c.text_c(Wt::Regular, 11.0, cx, cy + rad + 38.0, t.fg2, &tt);
    }
    let mut y = 14.0 + 2.0 * rad + 56.0;
    let up = num("allume_depuis") as i64;
    c.text(Wt::Regular, 12.0, SIDE, y, t.fg3, &format!("Allumé depuis {} h {:02}", up / 3600, up % 3600 / 60));
    y += 12.0;
    // boutons
    let actions = &v["actions"];
    let bw = (w - 2.0 * SIDE - 3.0 * 8.0) / 4.0;
    for (i, a) in PC_ACTIONS.iter().enumerate() {
        let (col, row) = (i % 4, i / 4);
        let (bx, by) = (SIDE + col as f32 * (bw + 8.0), y + row as f32 * 36.0);
        if by + 30.0 > h {
            break;
        }
        let danger = PC_DANGER.contains(a);
        let confirm = r.ui.pc_confirm.is_some_and(|(k, at)| k == i && at.elapsed().as_secs() < 5);
        let lab = if confirm { "Confirmer ?".to_string() } else { actions[*a].as_str().unwrap_or(a).to_string() };
        c.rr(bx, by, bw, 30.0, 10.0, if confirm { RED } else if danger { RED.a(0.18) } else { t.fg.a(0.1) });
        let lab = c.fit(Wt::Semibold, 12.0, &lab, bw - 12.0);
        c.text_c(Wt::Semibold, 12.0, bx + bw / 2.0, by + 19.5, if confirm { WHITE } else { t.fg }, &lab);
        c.hit(bx, by, bw, 30.0, Action::IleN(N_PC_ACT, i as u16));
    }
    y += 2.0 * 36.0 + 14.0;
    if big {
        caps(c, SIDE, y, t.fg2, "Ce qui utilise le plus de mémoire");
        y += 10.0;
        for p in arr(Some(v), "processus").into_iter().take(((h - y) / 20.0).max(0.0) as usize) {
            y += 20.0;
            c.text(Wt::Regular, 12.5, SIDE, y, t.fg, s(p, "nom"));
            c.text_r(Wt::Regular, 12.0, w - SIDE, y, t.fg2, &go(p["ram"].as_f64().unwrap_or(0.0)));
        }
    }
}

// ---------- Paramètres de Jarvis ----------

pub fn jsettings_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    crate::reglages::page(c, r, w, h);
}

/// Réglages de Jarvis (page « Jarvis et voix ») : (nom, valeur, interrupteur, boutons).
pub type JRow = (String, String, Option<bool>, Vec<(Action, &'static str)>);

pub fn jarvis_rows(r: &R) -> Vec<JRow> {
    let mut rows: Vec<JRow> = Vec::new();
    // ── allumer / éteindre (et la mémoire qu'on libère)
    let voix = if r.jarvis.online() { "allumée" } else { "coupée (la barre et l'IA écrite marchent)" };
    let bouton = if r.jarvis.online() { "Couper la voix" } else { "Allumer la voix" };
    rows.push(("Voix de Jarvis (« Hey Jarvis », Whisper, la voix) : coupée, elle ne prend plus de mémoire".into(), voix.into(), None, vec![(Action::IleN(N_POWER, 0), bouton)]));
    rows.push(("Allumer la voix au démarrage du PC (sinon seulement la barre)".into(), String::new(), Some(r.cfg.voix_demarrage), vec![(Action::IleN(N_POWER, 1), "")]));
    rows.push(("« Quitter Jarvis » arrête aussi LM Studio et décharge son modèle".into(), String::new(), Some(r.cfg.stop_lmstudio), vec![(Action::IleN(N_POWER, 2), "")]));
    rows.push(("Avec une clé d'API, ne jamais charger l'IA de ce PC (LM Studio, plusieurs Go)".into(), String::new(), Some(r.cfg.sans_local), vec![(Action::IleN(N_POWER, 3), "")]));
    rows.push(("Tout éteindre maintenant (la barre, la voix, LM Studio)".into(), String::new(), None, vec![(Action::IleN(N_POWER, 4), "Éteindre Jarvis")]));
    // ── réglés par la barre : l'IA, le PC, le PAI (marchent même sans l'assistant vocal)
    let choix = crate::profils::CHOIX_MODELE.iter().find(|c| c.0 == r.cfg.ia_mode).map(|c| c.1).unwrap_or("Auto");
    let mode = if r.cfg.ia_mode != 2 && !crate::ia::cle_nvidia() { format!("{choix} — il manque la clé NVIDIA") } else { choix.to_string() };
    rows.push(("Modèle de l'IA (ou dis « passe sur le plus rapide »)".into(), mode, None, vec![(Action::IleN(N_IA_MODE, 0), "‹"), (Action::IleN(N_IA_MODE, 1), "›")]));
    let p = crate::ia::profil();
    let profil = if crate::profils::par_id(&r.cfg.profil).is_some() { p.nom.to_string() } else { format!("Auto : {}", p.nom) };
    rows.push(("Profil du PC".into(), profil, None, vec![(Action::IleN(N_PROFIL, 0), "‹"), (Action::IleN(N_PROFIL, 1), "›")]));
    let pc = crate::profils::materiel_connu().map(|m| crate::profils::resume(&m)).unwrap_or_else(|| "pas encore regardé".into());
    rows.push((format!("Ce PC ({})", p.pour), pc, None, vec![(Action::IleN(N_DETECT, 0), "Détecter")]));
    let modele = match crate::ia::actif() {
        Some(a) => format!("{a} · {} outils", crate::ia::max_outils()),
        None if crate::ia::puissant() => crate::ia::nom_mode().to_string(),
        None => format!("{} · {:.1} Go", p.modele_local, p.taille_go),
    };
    rows.push(("En service".into(), modele, None, Vec::new()));
    let pai = &r.cfg.pronote.pai;
    for (i, j) in crate::pai::JOURS.iter().enumerate().take(6) {
        let val = if pai.jours[i].trim().is_empty() { "comme Pronote".to_string() } else { pai.jours[i].clone() };
        rows.push((format!("PAI · {j} (ex. 8h-12h30, aucun)"), val, None, vec![(Action::IleN(N_PAI_TEXT, i as u16), "Modifier")]));
    }
    let enleves = crate::pai::matieres_sans(pai, &r.d.pronote);
    let sans = if pai.sans.trim().is_empty() { "aucune".to_string() } else if enleves.is_empty() { format!("{} → aucune matière trouvée", pai.sans) } else { format!("{} → {}", pai.sans, enleves.join(", ")) };
    rows.push(("PAI · matières où je ne vais pas (virgules)".into(), sans, None, vec![(Action::IleN(N_PAI_TEXT, 6), "Modifier")]));
    let n = pai.sautes.len() + pai.remis.len();
    rows.push(("PAI · cours changés à la voix".into(), format!("{n} cours"), None, vec![(Action::IleN(N_PAI_RESET, 0), "Oublier")]));
    let ic = &r.cfg.ile;
    rows.push(("Sons des boutons (verre doux, plus bas après 21 h)".into(), String::new(), Some(ic.sons), vec![(Action::IleN(N_SONS, 0), "")]));
    rows.push(("Volume des sons".into(), format!("{} %", (ic.sons_volume * 100.0).round()), None, vec![(Action::IleN(N_SONS_VOL, 0), "−"), (Action::IleN(N_SONS_VOL, 1), "+")]));
    rows.push(("Onde au clic et lueur au survol".into(), String::new(), Some(ic.effets), vec![(Action::IleN(N_EFFETS, 0), "")]));
    // ── réglés par l'assistant vocal (config.toml)
    let vide = serde_json::Value::Null;
    let v = r.jd.get("parametres").unwrap_or(&vide);
    if v.is_null() {
        rows.push(("Les autres réglages s'affichent quand l'assistant vocal Jarvis est lancé.".into(), String::new(), None, Vec::new()));
    }
    if !v.is_null() {
        let voix = v["voix"]["actuelle"].as_str().unwrap_or("?");
        let titre = arr(Some(&v["voix"]), "liste").into_iter().find(|x| s(x, "nom") == voix).map(|x| s(x, "titre")).unwrap_or(voix);
        rows.push(("Voix".into(), titre.into(), None, vec![(Action::IleN(N_VOICE, 0), "‹"), (Action::IleN(N_VOICE, 1), "›")]));
        rows.push(("Lancer Jarvis au démarrage de Windows".into(), String::new(), v["demarrage_windows"].as_bool(), vec![(Action::IleN(N_STARTUP, 0), "")]));
    }
    for (i, rg) in arr(Some(v), "reglages").into_iter().enumerate() {
        let lab = s(rg, "libelle").to_string();
        let i = i as u16;
        match s(rg, "type") {
            "booleen" => rows.push((lab, String::new(), rg["valeur"].as_bool(), vec![(Action::IleN(N_SET_TOGGLE, i), "")])),
            "nombre" => rows.push((lab, rg["valeur"].to_string(), None, vec![(Action::IleN(N_SET_DEC, i), "−"), (Action::IleN(N_SET_INC, i), "+")])),
            "modele" => rows.push((lab, rg["valeur"].as_str().unwrap_or("").into(), None, vec![(Action::IleN(N_SET_DEC, i), "‹"), (Action::IleN(N_SET_INC, i), "›")])),
            "choix" => {
                // liste fournie par Jarvis : [{"valeur": "parakeet", "titre": "Parakeet"}, …]
                let val = rg["valeur"].as_str().unwrap_or("");
                let titre = arr(Some(rg), "choix").into_iter().find(|x| s(x, "valeur") == val).map(|x| s(x, "titre")).unwrap_or(val);
                rows.push((lab, titre.into(), None, vec![(Action::IleN(N_SET_DEC, i), "‹"), (Action::IleN(N_SET_INC, i), "›")]))
            }
            _ => rows.push((lab, rg["valeur"].as_str().unwrap_or("").into(), None, vec![(Action::IleN(N_SET_TEXT, i), "Modifier")])),
        }
    }
    rows
}

/// Une page « détail » existante, sur toute la hauteur du grand panneau.
fn tall(c: &mut Ctx, r: &R, w: f32, h: f32, f: fn(&mut Ctx, &R, f32, f32)) {
    let oy = c.oy;
    c.oy += 14.0 * c.s;
    f(c, r, w, h - 18.0);
    c.oy = oy;
}

/// Téléphone en grand : deux colonnes (SMS et appels / applis et batterie), textes sur 3 lignes.
fn big_phone(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    if r.d.phone.is_empty() {
        island_cards::phone_page(c, r, w);
        return;
    }
    let colw = (w - 2.0 * SIDE - 16.0) / 2.0;
    for (k, (title, kinds)) in [("SMS et appels", ["sms", "appel"]), ("Applis et batterie", ["appli", "batterie"])].iter().enumerate() {
        let x = SIDE + k as f32 * (colw + 16.0);
        caps(c, x, 18.0, t.fg2, title);
        let mut y = 30.0;
        for p in r.d.phone.iter().rev().filter(|p| kinds.contains(&p.kind.as_str())) {
            let lines = c.wrap(Wt::Regular, 12.5, &p.text, colw - 20.0);
            let n = lines.len().clamp(1, 3);
            let rh = 26.0 + n as f32 * 16.0;
            if y + rh > h {
                break;
            }
            c.rr(x - 6.0, y, colw + 12.0, rh - 4.0, 10.0, t.fg.a(0.05));
            let head = if p.app.is_empty() { p.from.clone() } else { format!("{} · {}", p.app, p.from) };
            let head = c.fit(Wt::Semibold, 13.0, &head, colw - 60.0);
            c.text(Wt::Semibold, 13.0, x + 4.0, y + 17.0, t.fg, &head);
            c.text_r(Wt::Regular, 11.5, x + colw - 4.0, y + 17.0, t.fg3, &p.time.format("%d/%m %H:%M").to_string());
            for (j, l) in lines.iter().take(n).enumerate() {
                let l = if j + 1 == n && lines.len() > n { c.fit(Wt::Regular, 12.5, &format!("{l} …"), colw - 20.0) } else { l.clone() };
                c.text(Wt::Regular, 12.5, x + 4.0, y + 34.0 + j as f32 * 16.0, t.fg2, &l);
            }
            y += rh;
        }
    }
}

// ---------- Comptes ----------

/// Problème de compte à signaler (titre, explication) : Pronote absent ou en erreur, aucun mail.
pub fn account_alert(r: &R) -> Option<(String, String)> {
    let p = &r.d.pronote;
    // Pronote ou les mails désactivés dans « Comptes » : jamais d'alerte pour eux
    if r.cfg.pronote.on && !p.connected {
        return Some(("Pronote n'est pas connecté".into(), "Ouvre « Comptes » pour le connecter, ou le désactiver.".into()));
    }
    if r.cfg.pronote.on && !p.error.is_empty() {
        return Some(("Pronote ne répond plus : reconnecte-le".into(), format!("{} — ouvre « Comptes ».", p.error)));
    }
    if r.cfg.mail.on && r.d.mail.accounts == 0 && r.d.accounts.google_json {
        return Some(("Aucun compte mail connecté".into(), "Ouvre « Comptes » pour ajouter Gmail.".into()));
    }
    None
}

/// Bandeau rouge « reconnecte… » avec un bouton vers les Comptes.
pub fn alert_banner(c: &mut Ctx, x: f32, y: f32, w: f32, title: &str, sub: &str) {
    c.rr(x, y, w, 40.0, 14.0, RED.a(0.85));
    c.circle(x + 20.0, y + 20.0, 9.0, WHITE);
    c.text_c(Wt::Semibold, 13.0, x + 20.0, y + 25.0, RED, "!");
    let tw = c.text(Wt::Semibold, 13.5, x + 38.0, y + 18.0, WHITE, title);
    let _ = tw;
    let sub = c.fit(Wt::Regular, 11.5, sub, w - 190.0);
    c.text(Wt::Regular, 11.5, x + 38.0, y + 33.0, WHITE.a(0.9), &sub);
    let bw = c.measure(Wt::Semibold, 12.0, "Ouvrir les comptes") + 20.0;
    c.rr(x + w - bw - 10.0, y + 8.0, bw, 24.0, 12.0, WHITE);
    c.text(Wt::Semibold, 12.0, x + w - bw, y + 24.5, RED, "Ouvrir les comptes");
    c.hit(x, y, w, 40.0, Action::Ile(A_ACCOUNTS));
}

fn status_dot(c: &mut Ctx, x: f32, y: f32, ok: bool) {
    c.circle(x, y, 5.0, if ok { GREEN } else { RED });
}

pub fn accounts_page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let colw = (w - 2.0 * SIDE - 16.0) / 2.0;
    let (x1, x2) = (SIDE - 6.0, SIDE - 6.0 + colw + 16.0);
    // ---- Pronote ----
    let ph = 300.0f32.min(h - 10.0);
    card(c, r, x1, 4.0, colw, ph);
    let p = &r.d.pronote;
    let ok = p.connected && p.error.is_empty();
    status_dot(c, x1 + 18.0, 26.0, ok);
    caps(c, x1 + 30.0, 30.0, t.fg, "Pronote");
    // bouton « Désactiver » / « Réactiver » en haut à droite de la carte
    let (lib, fond) = if r.cfg.pronote.on { ("Désactiver", t.fg.a(0.12)) } else { ("Réactiver", GREEN.a(0.9)) };
    let bw = c.measure(Wt::Semibold, 12.0, lib) + 24.0;
    chip(c, x1 + colw - bw - 12.0, 12.0, lib, fond, if r.cfg.pronote.on { t.fg2 } else { WHITE }, Some(Action::Ile(A_PN_TOGGLE)));
    if !r.cfg.pronote.on {
        for (k, l) in c.wrap(Wt::Regular, 12.5, "Pronote est désactivé : pas d'alerte « pas connecté », et Jarvis ne t'en parle plus. Clique sur « Réactiver » pour le remettre.", colw - 32.0).iter().enumerate() {
            c.text(Wt::Regular, 12.5, x1 + 16.0, 56.0 + k as f32 * 16.0, t.fg2, l);
        }
    } else {
    let etat = if !p.connected {
        "pas connecté".to_string()
    } else if !p.error.is_empty() {
        format!("erreur : {}", p.error)
    } else if p.class.is_empty() {
        format!("connecté : {}", p.name)
    } else {
        format!("connecté : {} ({})", p.name, p.class)
    };
    let etat = c.fit(Wt::Regular, 12.0, &etat, colw - 110.0);
    c.text(Wt::Regular, 12.0, x1 + 100.0, 30.0, if ok { t.fg2 } else { RED }, &etat);
    let steps = [
        "1. Ouvre Pronote dans ton navigateur (souvent par l'ENT), clique sur l'icône du QR code de l'appli mobile et choisis un code à 4 chiffres.",
        "2. Fais Win + Maj + S et capture le QR code.",
        "3. Clique « Coller le QR code », tape ton code à 4 chiffres, puis « Se connecter ».",
    ];
    let mut y = 52.0;
    for s_ in steps {
        for l in c.wrap(Wt::Regular, 12.5, s_, colw - 32.0) {
            c.text(Wt::Regular, 12.5, x1 + 16.0, y, t.fg, &l);
            y += 16.0;
        }
        y += 4.0;
    }
    y += 4.0;
    let mut bx = x1 + 14.0;
    if !p.base.is_empty() {
        bx += chip(c, bx, y, "Ouvrir Pronote", t.fg.a(0.12), t.fg, Some(Action::Ile(A_PN_OPEN))) + 6.0;
    }
    chip(c, bx, y, "Coller le QR code", t.accent.a(0.9), WHITE, Some(Action::Ile(A_PN_PASTE)));
    y += 34.0;
    let typing = r.st.typing && r.st.input_for == InputFor::Pin;
    let pin = if typing { "•".repeat(r.st.input.chars().count()) } else { String::new() };
    let ph_txt = if typing { if pin.is_empty() { "Tape ton code…".to_string() } else { pin } } else { "Code à 4 chiffres".to_string() };
    c.rr(x1 + 14.0, y, 150.0, 30.0, 15.0, t.fg.a(if typing { 0.16 } else { 0.08 }));
    if typing {
        c.rr_stroke(x1 + 14.0, y, 150.0, 30.0, 15.0, t.accent, 1.3);
    }
    c.text(Wt::Regular, 13.0, x1 + 28.0, y + 20.0, if typing { t.fg } else { t.fg3 }, &ph_txt);
    c.hit(x1 + 14.0, y, 150.0, 30.0, Action::IleN(N_INPUT, InputFor::Pin.code()));
    chip(c, x1 + 174.0, y + 3.0, "Se connecter", GREEN.a(0.9), WHITE, Some(Action::Ile(A_PN_LOGIN)));
    if !r.status.is_empty() {
        let lines = c.wrap(Wt::Regular, 12.0, r.status, colw - 28.0);
        for (k, l) in lines.iter().take(3).enumerate() {
            c.text(Wt::Regular, 12.0, x1 + 16.0, y + 50.0 + k as f32 * 15.0, ORANGE, l);
        }
    }
    }
    // ---- Mails ----
    let acc = &r.d.accounts;
    let mh = 200.0;
    card(c, r, x2, 4.0, colw, mh);
    status_dot(c, x2 + 18.0, 26.0, !acc.google.is_empty() || acc.ms.is_some());
    caps(c, x2 + 30.0, 30.0, t.fg, "Mails et agenda");
    let (lib, fond) = if r.cfg.mail.on { ("Désactiver", t.fg.a(0.12)) } else { ("Réactiver", GREEN.a(0.9)) };
    let bw = c.measure(Wt::Semibold, 12.0, lib) + 24.0;
    chip(c, x2 + colw - bw - 12.0, 12.0, lib, fond, if r.cfg.mail.on { t.fg2 } else { WHITE }, Some(Action::Ile(A_MAIL_TOGGLE)));
    if !r.cfg.mail.on {
        for (k, l) in c.wrap(Wt::Regular, 12.5, "Les mails sont désactivés : pas d'alerte, et Jarvis ne lit plus tes mails. Clique sur « Réactiver » pour les remettre.", colw - 32.0).iter().enumerate() {
            c.text(Wt::Regular, 12.5, x2 + 16.0, 56.0 + k as f32 * 16.0, t.fg2, l);
        }
    } else {
    let mut yy = 52.0;
    for e in acc.google.iter().map(|g| format!("Google : {g}")).chain(acc.ms.iter().map(|m| format!("Hotmail : {m}"))).take(3) {
        let e = c.fit(Wt::Regular, 12.5, &e, colw - 32.0);
        c.text(Wt::Regular, 12.5, x2 + 16.0, yy, t.fg, &e);
        yy += 17.0;
    }
    if acc.google.is_empty() && acc.ms.is_none() {
        c.text(Wt::Regular, 12.5, x2 + 16.0, yy, t.fg2, "Aucun compte connecté.");
        yy += 17.0;
    }
    if !acc.google_json {
        for l in c.wrap(Wt::Regular, 11.5, "Connexion Google pas disponible dans cette version.", colw - 32.0).iter().take(2) {
            c.text(Wt::Regular, 11.5, x2 + 16.0, yy, ORANGE, l);
            yy += 15.0;
        }
    }
    let by = mh - 34.0;
    for l in c.wrap(Wt::Regular, 11.0, "Si Google affiche « Google n'a pas validé cette application », clique sur Paramètres avancés puis Accéder à…", colw - 32.0).iter().take(2).rev().enumerate() {
        c.text(Wt::Regular, 11.0, x2 + 16.0, by - 8.0 - l.0 as f32 * 14.0, t.fg3, l.1);
    }
    let gw = chip(c, x2 + 14.0, by, "Se connecter avec Google", t.accent.a(0.9), WHITE, Some(Action::Ile(A_G_ADD)));
    chip(c, x2 + 20.0 + gw, by, "Ajouter Hotmail", t.fg.a(0.12), t.fg, Some(Action::Ile(A_MS_ADD)));
    }
    // ---- Discord et IA ----
    let dy = mh + 14.0;
    let dh = (h - dy - 6.0).max(120.0);
    card(c, r, x2, dy, colw, dh);
    caps(c, x2 + 16.0, dy + 26.0, t.fg, "Discord et IA");
    let mut y3 = dy + 48.0;
    let notif_ok = r.d.winnotif_state.contains("activ");
    status_dot(c, x2 + 20.0, y3 - 4.0, notif_ok);
    let l = c.fit(Wt::Regular, 12.5, &format!("Messages privés Discord : {}", if r.d.winnotif_state.is_empty() { "en attente" } else { &r.d.winnotif_state }), colw - 46.0);
    c.text(Wt::Regular, 12.5, x2 + 32.0, y3, t.fg, &l);
    y3 += 18.0;
    for l in c.wrap(Wt::Regular, 11.5, "Rien à connecter : la barre lit les notifications Windows de Discord (laisse-les activées dans Discord et dans Windows).", colw - 32.0).iter().take(3) {
        c.text(Wt::Regular, 11.5, x2 + 16.0, y3, t.fg2, l);
        y3 += 15.0;
    }
    y3 += 6.0;
    let ai_ok = acc.ai.starts_with("En service");
    status_dot(c, x2 + 20.0, y3 - 4.0, ai_ok);
    let ai = c.fit(Wt::Regular, 12.0, &acc.ai, colw - 46.0);
    c.text(Wt::Regular, 12.0, x2 + 32.0, y3, t.fg2, &ai);
    if y3 + 40.0 < dy + dh {
        chip(c, x2 + 14.0, dy + dh - 34.0, "Réglages avancés…", t.fg.a(0.12), t.fg2, Some(Action::Ile(A_SETTINGS_ACC)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

    fn jour(j: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, j).unwrap()
    }
    fn a(j: u32, h: u32, m: u32) -> NaiveDateTime {
        jour(j).and_hms_opt(h, m, 0).unwrap()
    }
    fn cours(j: u32, debut: (u32, u32), fin: (u32, u32), matiere: &str) -> Lesson {
        Lesson { start: a(j, debut.0, debut.1), end: a(j, fin.0, fin.1), subject: matiere.into(), ..Default::default() }
    }
    fn heure(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn frise_de_8_h_a_18_h_et_positions() {
        let maths = cours(23, (8, 30), (9, 25), "Maths");
        let eps = cours(23, (15, 30), (17, 25), "EPS");
        assert_eq!(frise_bornes(&[&maths, &eps]), (8, 18));
        assert_eq!(frise_bornes(&[]), (8, 18));
        // une sortie qui déborde élargit la frise
        let sortie = cours(23, (7, 45), (18, 30), "Sortie");
        assert_eq!(frise_bornes(&[&sortie]), (7, 19));
        // 13 h est au milieu de 8 h – 18 h
        assert!((frise_x(heure(13, 0), (8, 18), 100.0, 1000.0) - 600.0).abs() < 0.01);
        assert!((frise_x(heure(8, 30), (8, 18), 0.0, 600.0) - 30.0).abs() < 0.01);
        // avant et après : collé aux bords
        assert_eq!(frise_x(heure(6, 0), (8, 18), 100.0, 1000.0), 100.0);
        assert_eq!(frise_x(heure(20, 0), (8, 18), 100.0, 1000.0), 1100.0);
    }

    #[test]
    fn fin_du_cours_et_fin_de_la_journee() {
        let maths = cours(23, (8, 30), (9, 25), "Maths");
        let fr = cours(23, (9, 35), (10, 30), "Français");
        let mut anglais = cours(23, (10, 30), (11, 25), "Anglais");
        anglais.canceled = true;
        let mut latin = cours(23, (11, 30), (12, 25), "Latin");
        latin.manque = "emploi du temps aménagé".into();
        let l = [&maths, &fr, &anglais, &latin];
        let txt = |s: &str, t: &str| Some((s.to_string(), t.to_string()));
        assert_eq!(fins_txt(&l, a(23, 9, 13)), txt("fin du cours dans 12 min", "fin de la journée dans 1 h 17"));
        assert_eq!(fins_txt(&l, a(23, 9, 27)), txt("pause : Français dans 8 min", "fin de la journée dans 1 h 03"));
        // avant le premier cours, et après le dernier (l'annulé et le cours où je ne vais pas ne comptent pas)
        assert_eq!(fins_txt(&l, a(23, 8, 0)), None);
        assert_eq!(fins_txt(&l, a(23, 10, 45)), None);
    }

    #[test]
    fn le_soir_on_montre_le_prochain_jour_de_cours() {
        let p = School { lessons: vec![cours(23, (8, 30), (16, 30), "Maths"), cours(24, (9, 0), (12, 0), "Français")], ..Default::default() };
        // lundi 23 en journée : aujourd'hui, devoirs pour mardi 24
        assert_eq!(jours_page(&p, a(23, 10, 0)), (jour(23), jour(24)));
        // lundi soir : le programme de mardi
        assert_eq!(jours_page(&p, a(23, 17, 0)), (jour(24), jour(24)));
        // samedi 21 (pas cours) : le programme de lundi
        assert_eq!(jours_page(&p, a(21, 10, 0)), (jour(23), jour(23)));
    }
}
