//! « Grande île » : l'île dépliée en grand panneau (barre de sections + contenu), et les pages des
//! sections de Jarvis (tâches, rappels, habitudes, copiés, routines, mémoire, batteries, PC, paramètres),
//! aussi utilisées comme onglets compacts dans l'île normale.

use chrono::{Datelike, Timelike};
use serde_json::Value;

use crate::gfx::{Rgba, WHITE, Wt};
use crate::island::*;
use crate::island_cards::{self, caps, icon, media_card, vgrad, weather_card};
use crate::island_draw::*;
use crate::jarvis_data::{arr, s};
use crate::widgets::{self, Action, Ctx, GREEN, ORANGE, RED};

pub const BIG_W: f32 = 1100.0;
pub const BIG_H: f32 = 720.0;
const SIDEBAR: f32 = 212.0;

/// Sections de la grande île, dans l'ordre de la barre.
pub const SECTIONS: [(View, Tab, &str); 18] = [
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

pub fn big_view(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    island_cards::header(c, r, w);
    // barre des sections
    let cur = Tab::of(r.st.view);
    let mut y = 48.0;
    for (v, tab, name) in SECTIONS {
        let on = tab == cur;
        if on {
            c.rr(12.0, y, SIDEBAR - 20.0, 32.0, 11.0, t.fg.a(0.13));
        }
        icon(c, tab, 32.0, y + 16.0, if on { t.fg } else { t.fg2 });
        c.text(Wt::Semibold, 13.0, 52.0, y + 21.0, if on { t.fg } else { t.fg2 }, name);
        c.hit(12.0, y, SIDEBAR - 20.0, 32.0, Action::IleN(N_SECTION, v as u16));
        y += 35.0;
    }
    if r.st.big_force {
        chip(c, 16.0, h - 36.0, "Fermer (Échap)", t.fg.a(0.12), t.fg2, Some(Action::Ile(A_BIG_CLOSE)));
    }
    c.line(SIDEBAR, 50.0, SIDEBAR, h - 16.0, 1.0, t.sep);
    // contenu
    let (ox, oy) = (c.ox, c.oy);
    c.ox += (SIDEBAR - 4.0) * c.s;
    c.oy += island_cards::TOP * c.s;
    let cw = w - SIDEBAR + 4.0;
    let ch = h - island_cards::TOP - 12.0;
    match r.st.view {
        View::Main => big_home(c, r, cw, ch),
        v => {
            if !section_page(c, r, v, cw, ch) {
                island_cards::page(c, r, v, cw);
            }
        }
    }
    c.ox = ox;
    c.oy = oy;
}

/// Pages propres aux sections de Jarvis. `false` = page déjà existante (à dessiner par island_cards).
pub fn section_page(c: &mut Ctx, r: &R, v: View, w: f32, h: f32) -> bool {
    match v {
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
    c.text(Wt::Regular, 11.0, ax + 10.0, ay + 18.0, t.fg3, &format!("{nb} souvenir{} · glisser = déplacer · molette = zoom", if nb > 1 { "s" } else { "" }));
    let by = h - 38.0;
    let mut fx = SIDE - 4.0;
    if let Some(sel) = &g.sel {
        if sel != "moi" {
            fx += chip(c, fx, by + 4.0, "Supprimer", RED.a(0.3), t.fg, Some(Action::Ile(A_MEM_DEL))) + 6.0;
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
    if absent(c, r, "parametres") {
        return;
    }
    let t = c.t;
    let Some(v) = r.jd.get("parametres") else { return };
    let mut rows: Vec<(String, String, Option<bool>, Vec<(Action, &'static str)>)> = Vec::new();
    let voix = v["voix"]["actuelle"].as_str().unwrap_or("?");
    let titre = arr(Some(&v["voix"]), "liste").into_iter().find(|x| s(x, "nom") == voix).map(|x| s(x, "titre")).unwrap_or(voix);
    rows.push(("Voix".into(), titre.into(), None, vec![(Action::IleN(N_VOICE, 0), "‹"), (Action::IleN(N_VOICE, 1), "›")]));
    rows.push(("Lancer Jarvis au démarrage de Windows".into(), String::new(), v["demarrage_windows"].as_bool(), vec![(Action::IleN(N_STARTUP, 0), "")]));
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
    let mut y = 4.0;
    for (lab, val, toggle, acts) in rows.into_iter().skip(r.st.scroll) {
        if y + 40.0 > h - 22.0 {
            break;
        }
        c.rr(SIDE - 6.0, y, w - 2.0 * SIDE + 12.0, 36.0, 10.0, t.fg.a(0.05));
        let lab = c.fit(Wt::Regular, 13.0, &lab, w * 0.55);
        c.text(Wt::Regular, 13.0, SIDE + 4.0, y + 23.0, t.fg, &lab);
        let xr = w - SIDE;
        match toggle {
            Some(on) => {
                let (sx, sy) = (xr - 44.0, y + 8.0);
                c.rr(sx, sy, 40.0, 20.0, 10.0, if on { t.accent } else { t.track });
                c.circle(if on { sx + 30.0 } else { sx + 10.0 }, sy + 10.0, 8.0, WHITE);
                c.hit(SIDE - 6.0, y, w - 2.0 * SIDE + 12.0, 36.0, acts[0].0);
            }
            None => {
                let mut x = xr;
                for (a, lab) in acts.iter().rev() {
                    let bw = c.measure(Wt::Semibold, 12.0, lab) + 20.0;
                    x -= bw;
                    chip(c, x, y + 6.0, lab, t.fg.a(0.12), t.fg, Some(*a));
                    x -= 6.0;
                }
                let editing = matches!(r.st.input_for, InputFor::Setting(_)) && r.st.typing && acts.iter().any(|(a, _)| matches!(a, Action::IleN(N_SET_TEXT, k) if r.st.input_for == InputFor::Setting(*k as usize)));
                let shown = if editing { format!("{}|", r.st.input) } else { val };
                let vv = c.fit(Wt::Semibold, 12.5, &shown, (x - w * 0.6).max(60.0));
                c.text_r(Wt::Semibold, 12.5, x - 8.0, y + 23.0, if editing { t.accent } else { t.fg2 }, &vv);
            }
        }
        y += 40.0;
    }
    c.text(Wt::Regular, 11.5, SIDE, h - 6.0, t.fg3, "Certains réglages servent au prochain démarrage de Jarvis. Molette pour voir la suite.");
    let _ = vgrad;
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
