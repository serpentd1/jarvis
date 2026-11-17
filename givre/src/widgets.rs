//! Les widgets : taille, dessin, zones cliquables.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use tiny_skia::{FillRule, FilterQuality, Paint, PathBuilder, Pattern, Pixmap, SpreadMode, Stroke, Transform};

use crate::backdrop::Backdrop;
use crate::config::{Config, Place};
use crate::data::{Cat, Data, Event, Holiday, wmo_label};
use crate::extra::{is_active, status_label};
use crate::gfx::{self, Fonts, Rgba, Theme, WHITE, Wt};
use crate::pronote::{Alert, Lesson, grade_num, grade_txt};

pub const W: f32 = 340.0;
pub const JOURS: [&str; 7] = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche"];
pub const JOURS_C: [&str; 7] = ["Lun.", "Mar.", "Mer.", "Jeu.", "Ven.", "Sam.", "Dim."];
pub const MOIS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre",
];
pub const MOIS_C: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Clock,
    Weather,
    Calendar,
    Countdown,
    Mail,
    Music,
    Discord,
    Printer,
    NextClass,
    Homework,
    Grades,
    School,
}

impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::Clock,
        Kind::Weather,
        Kind::Calendar,
        Kind::Countdown,
        Kind::Mail,
        Kind::Music,
        Kind::Discord,
        Kind::Printer,
        Kind::NextClass,
        Kind::Homework,
        Kind::Grades,
        Kind::School,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Kind::Clock => "horloge",
            Kind::Weather => "meteo",
            Kind::Calendar => "calendrier",
            Kind::Countdown => "vacances",
            Kind::Mail => "mails",
            Kind::Music => "musique",
            Kind::Discord => "discord",
            Kind::Printer => "imprimante",
            Kind::NextClass => "prochain_cours",
            Kind::Homework => "devoirs",
            Kind::Grades => "notes",
            Kind::School => "pronote",
        }
    }
    pub fn enabled(self, c: &Config) -> bool {
        match self {
            Kind::Clock => c.clock.on,
            Kind::Weather => c.weather.on,
            Kind::Calendar => c.calendar.on,
            Kind::Countdown => c.countdown.on,
            Kind::Mail => c.mail.on,
            Kind::Music => c.music,
            Kind::Discord => c.discord.on,
            Kind::Printer => c.printer.on,
            Kind::NextClass => c.pronote.next,
            Kind::Homework => c.pronote.homework,
            Kind::Grades => c.pronote.grades,
            Kind::School => c.pronote.info,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    PrevMonth,
    NextMonth,
    Today,
    Refresh,
    Day(NaiveDate),
    Prev,
    Toggle,
    Next,
    OpenPrinter,
    Accounts,
    /// Cocher / décocher le devoir n° i de la liste Pronote.
    Homework(u16),
    OpenPronote,
    /// Ouvrir le mail n° i de la liste (dans le navigateur).
    OpenMail(u16),
    /// Ouvrir la boîte de réception Gmail.
    OpenInbox,
}

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub act: Action,
}

/// Contexte de dessin : coordonnées « logiques » (à 100 %), converties en pixels.
pub struct Ctx<'a> {
    pub pm: &'a mut Pixmap,
    pub f: &'a Fonts,
    pub t: &'a Theme,
    pub s: f32,
    pub ox: f32,
    pub oy: f32,
    pub hits: Vec<Hit>,
    /// Facteur de taille du texte (1 = normal) et texte plus gras.
    pub tf: f32,
    pub bold: bool,
}

#[allow(clippy::too_many_arguments)]
impl<'a> Ctx<'a> {
    pub fn new(pm: &'a mut Pixmap, f: &'a Fonts, t: &'a Theme, s: f32, ox: f32, oy: f32) -> Self {
        Ctx { pm, f, t, s, ox, oy, hits: Vec::new(), tf: 1.0, bold: false }
    }
    fn w(&self, wt: Wt) -> Wt {
        match (self.bold, wt) {
            (true, Wt::Light) => Wt::Regular,
            (true, _) => Wt::Semibold,
            _ => wt,
        }
    }
    fn px(&self, x: f32) -> f32 {
        self.ox + x * self.s
    }
    fn py(&self, y: f32) -> f32 {
        self.oy + y * self.s
    }
    pub fn text(&mut self, wt: Wt, size: f32, x: f32, y: f32, c: Rgba, s: &str) -> f32 {
        self.text_t(wt, size, x, y, c, s, 0.0)
    }
    pub fn text_t(&mut self, wt: Wt, size: f32, x: f32, y: f32, c: Rgba, s: &str, track: f32) -> f32 {
        let (px, py) = (self.px(x), self.py(y));
        let wt = self.w(wt);
        self.f.draw(self.pm, wt, size * self.tf * self.s, px, py, c, s, track * self.s, self.t.gamma) / self.s
    }
    pub fn measure(&self, wt: Wt, size: f32, s: &str) -> f32 {
        self.f.measure(self.w(wt), size * self.tf * self.s, s, 0.0) / self.s
    }
    pub fn text_r(&mut self, wt: Wt, size: f32, xr: f32, y: f32, c: Rgba, s: &str) -> f32 {
        let w = self.measure(wt, size, s);
        self.text(wt, size, xr - w, y, c, s);
        w
    }
    pub fn text_c(&mut self, wt: Wt, size: f32, xc: f32, y: f32, c: Rgba, s: &str) -> f32 {
        let w = self.measure(wt, size, s);
        self.text(wt, size, xc - w / 2.0, y, c, s);
        w
    }
    pub fn fit(&self, wt: Wt, size: f32, s: &str, max: f32) -> String {
        self.f.ellipsize(self.w(wt), size * self.tf * self.s, s, max * self.s)
    }
    pub fn rr(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgba) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        if let Some(p) = gfx::rrect(self.px(x), self.py(y), w * self.s, h * self.s, r * self.s, false) {
            gfx::fill(self.pm, &p, c);
        }
    }
    pub fn rr_stroke(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgba, width: f32) {
        let hw = width * self.s / 2.0;
        if let Some(p) = gfx::rrect(self.px(x) + hw, self.py(y) + hw, w * self.s - 2.0 * hw, h * self.s - 2.0 * hw, r * self.s, false) {
            let st = Stroke { width: width * self.s, ..Default::default() };
            self.pm.stroke_path(&p, &gfx::paint(c), &st, Transform::identity(), None);
        }
    }
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        let (x, y) = (self.px(cx), self.py(cy));
        gfx::circle(self.pm, x, y, r * self.s, c);
    }
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, c: Rgba, width: f32) {
        if let Some(p) = PathBuilder::from_circle(self.px(cx), self.py(cy), r * self.s) {
            gfx::stroke(self.pm, &p, c, width * self.s);
        }
    }
    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, w: f32, c: Rgba) {
        let (a, b, cc, d) = (self.px(x1), self.py(y1), self.px(x2), self.py(y2));
        gfx::line(self.pm, a, b, cc, d, w * self.s, c);
    }
    /// Ligne horizontale nette d'un pixel.
    pub fn hline(&mut self, x1: f32, x2: f32, y: f32, c: Rgba) {
        let (a, b, yy) = (self.px(x1), self.px(x2), self.py(y).round());
        if let Some(p) = tiny_skia::Rect::from_xywh(a, yy, b - a, self.s.round().max(1.0)).map(PathBuilder::from_rect) {
            gfx::fill(self.pm, &p, c);
        }
    }
    pub fn grad_rr(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c0: Rgba, c1: Rgba) {
        let (px, py) = (self.px(x), self.py(y));
        if let Some(p) = gfx::rrect(px, py, w * self.s, h * self.s, r * self.s, false) {
            gfx::grad_fill(self.pm, &p, px, px + w * self.s, c0, c1);
        }
    }
    pub fn icon(&mut self, code: i32, day: bool, x: f32, y: f32, sz: f32) {
        let (px, py) = (self.px(x), self.py(y));
        gfx::weather_icon(self.pm, code, day, px, py, sz * self.s, self.t);
    }
    pub fn chevron(&mut self, cx: f32, cy: f32, size: f32, left: bool, c: Rgba) {
        let d = if left { 1.0 } else { -1.0 } * size * 0.5;
        self.line(cx + d, cy - size, cx - d, cy, 2.0, c);
        self.line(cx - d, cy, cx + d, cy + size, 2.0, c);
    }
    pub fn wrap(&self, wt: Wt, size: f32, s: &str, max: f32) -> Vec<String> {
        let mut out = Vec::new();
        let mut line = String::new();
        for w in s.split_whitespace() {
            let cand = if line.is_empty() { w.to_string() } else { format!("{line} {w}") };
            if !line.is_empty() && self.measure(wt, size, &cand) > max {
                out.push(std::mem::take(&mut line));
                line = w.to_string();
            } else {
                line = cand;
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
        out
    }
    pub fn image(&mut self, img: &Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let (px, py, pw, ph) = (self.px(x), self.py(y), w * self.s, h * self.s);
        let Some(path) = gfx::rrect(px, py, pw, ph, r * self.s, true) else { return };
        let tr = Transform::from_scale(pw / img.width() as f32, ph / img.height() as f32).post_translate(px, py);
        let mut p = Paint::default();
        p.anti_alias = true;
        p.shader = Pattern::new(img.as_ref(), SpreadMode::Pad, FilterQuality::Bicubic, 1.0, tr);
        self.pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
    }
    pub fn poly(&mut self, pts: &[(f32, f32)], c: Rgba) {
        let mut pb = PathBuilder::new();
        for (i, (x, y)) in pts.iter().enumerate() {
            let (a, b) = (self.px(*x), self.py(*y));
            if i == 0 { pb.move_to(a, b) } else { pb.line_to(a, b) }
        }
        pb.close();
        if let Some(p) = pb.finish() {
            gfx::fill(self.pm, &p, c);
        }
    }
    pub fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, act: Action) {
        let (px, py) = (self.px(x), self.py(y));
        self.hits.push(Hit { x: px, y: py, w: w * self.s, h: h * self.s, act });
    }
}

// ---------- Tailles ----------

pub fn exam_date(c: &Config) -> Option<NaiveDate> {
    let s = c.countdown.exam_date.trim();
    NaiveDate::parse_from_str(s, "%d/%m/%Y").or_else(|_| NaiveDate::parse_from_str(s, "%Y-%m-%d")).ok()
}

/// Taille logique (à 100 %) du widget. `loaded` : données météo disponibles.
pub fn size(k: Kind, c: &Config, loaded: bool) -> (f32, f32) {
    let h = match k {
        Kind::Clock => if c.clock.date { 160.0 } else { 112.0 },
        Kind::Weather => {
            let mut h = 150.0;
            if loaded {
                if c.weather.hourly {
                    h += 96.0;
                }
                if c.weather.daily {
                    h += 170.0;
                }
            }
            h
        }
        Kind::Calendar => if c.calendar.agenda { 424.0 } else { 304.0 },
        Kind::Countdown => if exam_date(c).is_some() { 172.0 } else { 128.0 },
        Kind::Mail => 300.0,
        Kind::Music => 104.0,
        Kind::Discord => 236.0,
        Kind::Printer => 120.0,
        Kind::NextClass => 156.0,
        Kind::Homework => 300.0,
        Kind::Grades => 300.0,
        Kind::School => 248.0,
    };
    (W, h)
}

pub struct Look<'a> {
    pub cfg: &'a Config,
    pub fonts: &'a Fonts,
    pub theme: &'a Theme,
    pub backdrop: Option<&'a Backdrop>,
    pub dpi: f32,
    pub edit: bool,
    /// Jour choisi dans le calendrier (sa journée s'affiche en bas).
    pub sel: Option<NaiveDate>,
}

/// Dessine un widget complet. Renvoie l'image, la marge (ombre) et les zones cliquables.
pub fn render(k: Kind, l: &Look, d: &Data, place: Place, month_off: i32, now: NaiveDateTime) -> Option<(Pixmap, i32, Vec<Hit>)> {
    let c = l.cfg;
    let s = l.dpi * c.scale * place.scale;
    let (lw, lh) = size(k, c, d.weather.is_some());
    let (cw, ch) = ((lw * s).round(), (lh * s).round());
    let m = if c.shadow > 0.01 { (32.0 * s).ceil() } else { (6.0 * s).ceil() };
    let mut pm = Pixmap::new((cw + 2.0 * m) as u32, (ch + 2.0 * m) as u32)?;
    let win = (place.x - m as i32, place.y - m as i32);
    let st = gfx::CardStyle { radius: c.radius, cont: c.continuous, shadow: c.shadow, border: c.border };
    gfx::card(&mut pm, m, cw, ch, &st, l.theme, if c.blur { l.backdrop } else { None }, win, s);
    let hits = {
        let mut x = Ctx::new(&mut pm, l.fonts, l.theme, s, m, m);
        match k {
            Kind::Clock => clock(&mut x, c, d, now),
            Kind::Weather => weather(&mut x, c, d),
            Kind::Calendar => calendar(&mut x, c, d, now, month_off, l.sel),
            Kind::Countdown => countdown(&mut x, c, d, now),
            Kind::Mail => mail(&mut x, d),
            Kind::Music => music(&mut x, d),
            Kind::Discord => discord(&mut x, c, d),
            Kind::Printer => printer(&mut x, d),
            Kind::NextClass => next_class(&mut x, d, now),
            Kind::Homework => homework(&mut x, d, now),
            Kind::Grades => grades(&mut x, d),
            Kind::School => school(&mut x, d),
        }
        x.hits
    };
    if l.edit {
        gfx::edit_overlay(&mut pm, l.fonts, m, cw, ch, &st, l.theme, s, place.scale);
    }
    Some((pm, m as i32, hits))
}

pub(crate) fn deg(v: f32) -> String {
    format!("{}°", v.round() as i32)
}

fn day_num(d: u32) -> String {
    if d == 1 { "1er".into() } else { d.to_string() }
}

fn short_date(d: NaiveDate) -> String {
    format!("{} {}", day_num(d.day()), MOIS_C[d.month0() as usize])
}

// ---------- Horloge ----------

fn clock(c: &mut Ctx, cfg: &Config, d: &Data, now: NaiveDateTime) {
    let t = c.t;
    let wt = match cfg.clock_weight {
        0 => Wt::Light,
        1 => Wt::Regular,
        _ => Wt::Semibold,
    };
    let (h, mi) = (now.hour(), now.minute());
    let time = if cfg.clock.h24 {
        format!("{h:02}:{mi:02}")
    } else {
        format!("{}:{mi:02}", if h % 12 == 0 { 12 } else { h % 12 })
    };
    let base = 82.0;
    let mut x = 19.0 + c.text(wt, 74.0, 19.0, base, t.fg, &time) + 8.0;
    if cfg.clock.seconds {
        x += c.text(Wt::Regular, 26.0, x, base, t.fg3, &format!("{:02}", now.second())) + 8.0;
    }
    if !cfg.clock.h24 {
        c.text(Wt::Semibold, 18.0, x, base, t.fg2, if h < 12 { "AM" } else { "PM" });
    }
    if cfg.clock.date {
        let date = format!(
            "{} {} {}",
            JOURS[now.weekday().num_days_from_monday() as usize],
            day_num(now.day()),
            MOIS[now.month0() as usize]
        );
        c.text(Wt::Semibold, 19.0, 23.0, 116.0, t.fg, &date);
        let mut sub: Vec<String> = Vec::new();
        if cfg.clock.week {
            sub.push(format!("Semaine {}", now.iso_week().week()));
        }
        if let Some(w) = &d.weather {
            sub.push(format!("Lever {}", w.sunrise));
            sub.push(format!("Coucher {}", w.sunset));
        }
        if sub.is_empty() {
            sub.push(now.year().to_string());
        }
        let line = c.fit(Wt::Regular, 14.0, &sub.join("  ·  "), W - 46.0);
        c.text(Wt::Regular, 14.0, 23.0, 140.0, t.fg2, &line);
    }
}

// ---------- Météo ----------

pub(crate) fn temp_color(t: f32) -> Rgba {
    const S: [(f32, [u8; 3]); 6] = [
        (-5.0, [94, 92, 230]),
        (3.0, [90, 200, 250]),
        (10.0, [52, 199, 89]),
        (18.0, [255, 204, 0]),
        (26.0, [255, 149, 0]),
        (34.0, [255, 59, 48]),
    ];
    if t <= S[0].0 {
        return Rgba::from(S[0].1);
    }
    for w in S.windows(2) {
        if t <= w[1].0 {
            let k = (t - w[0].0) / (w[1].0 - w[0].0);
            return Rgba::from(w[0].1).mix(Rgba::from(w[1].1), k);
        }
    }
    Rgba::from(S[5].1)
}

fn weather(c: &mut Ctx, cfg: &Config, d: &Data) {
    let t = c.t;
    let city = c.fit(Wt::Semibold, 16.0, &cfg.weather.city, 190.0);
    c.text(Wt::Semibold, 16.0, 22.0, 36.0, t.fg, &city);
    c.hit(0.0, 0.0, W, 150.0, Action::Refresh);
    let Some(w) = &d.weather else {
        c.text(Wt::Regular, 14.0, 22.0, 84.0, t.fg2, "Chargement de la météo…");
        return;
    };
    c.text(Wt::Light, 60.0, 17.0, 100.0, t.fg, &deg(w.temp));
    c.icon(w.code, w.day, W - 22.0 - 52.0, 14.0, 52.0);
    c.text_r(Wt::Semibold, 14.0, W - 22.0, 88.0, t.fg, wmo_label(w.code, w.day));
    if let Some(td) = w.days.first() {
        c.text_r(Wt::Regular, 13.5, W - 22.0, 108.0, t.fg2, &format!("↑ {}   ↓ {}", deg(td.max), deg(td.min)));
    }
    match &w.rain {
        Some(r) => {
            c.text(Wt::Semibold, 13.5, 22.0, 134.0, t.rain, r);
        }
        None => {
            let s = format!(
                "Ressenti {}  ·  Vent {} km/h  ·  Humidité {} %",
                deg(w.feels),
                w.wind.round() as i32,
                w.humidity.round() as i32
            );
            let s = c.fit(Wt::Regular, 13.0, &s, W - 44.0);
            c.text(Wt::Regular, 13.0, 22.0, 134.0, t.fg2, &s);
        }
    }
    let mut y = 150.0;
    if cfg.weather.hourly && !w.hours.is_empty() {
        c.hline(22.0, W - 22.0, y, t.sep);
        let n = w.hours.len().min(6);
        let cw = (W - 36.0) / n as f32;
        for (i, h) in w.hours.iter().take(n).enumerate() {
            let cx = 18.0 + cw * (i as f32 + 0.5);
            let (wt, col) = if i == 0 { (Wt::Semibold, t.fg) } else { (Wt::Regular, t.fg2) };
            c.text_c(wt, 12.5, cx, y + 26.0, col, &h.label);
            c.icon(h.code, h.day, cx - 14.0, y + 34.0, 28.0);
            c.text_c(Wt::Semibold, 14.5, cx, y + 82.0, t.fg, &deg(h.temp));
        }
        y += 96.0;
    }
    if cfg.weather.daily && !w.days.is_empty() {
        c.hline(22.0, W - 22.0, y, t.sep);
        y += 8.0;
        let days = &w.days[..w.days.len().min(5)];
        let lo = days.iter().map(|d| d.min).fold(f32::MAX, f32::min);
        let hi = days.iter().map(|d| d.max).fold(f32::MIN, f32::max);
        let span = (hi - lo).max(1.0);
        let (bx0, bx1) = (156.0, W - 64.0);
        let bw = bx1 - bx0;
        for (i, dd) in days.iter().enumerate() {
            let ry = y + i as f32 * 30.0;
            let name = if i == 0 { "Auj." } else { JOURS_C[dd.date.weekday().num_days_from_monday() as usize] };
            c.text(Wt::Semibold, 14.0, 22.0, ry + 20.0, t.fg, name);
            c.icon(dd.code, true, 76.0, ry + 3.0, 24.0);
            c.text_r(Wt::Regular, 14.0, bx0 - 10.0, ry + 20.0, t.fg2, &deg(dd.min));
            c.rr(bx0, ry + 13.0, bw, 5.0, 2.5, t.track);
            let a = bx0 + bw * (dd.min - lo) / span;
            let b = bx0 + bw * (dd.max - lo) / span;
            c.grad_rr(a, ry + 13.0, (b - a).max(5.0), 5.0, 2.5, temp_color(dd.min), temp_color(dd.max));
            if i == 0 {
                let p = bx0 + bw * ((w.temp - lo) / span).clamp(0.0, 1.0);
                c.circle(p, ry + 15.5, 4.8, Rgba(0.0, 0.0, 0.0, 0.35));
                c.circle(p, ry + 15.5, 3.2, WHITE);
            }
            c.text_r(Wt::Semibold, 14.0, W - 22.0, ry + 20.0, t.fg, &deg(dd.max));
        }
    }
}

// ---------- Calendrier ----------

fn is_holiday(hs: &[Holiday], d: NaiveDate) -> bool {
    hs.iter().any(|h| h.start <= d && d < h.end)
}

fn event_on(e: &Event, d: NaiveDate) -> bool {
    let (s, mut en) = (e.start.date(), e.end.date());
    if e.all_day && en > s {
        en -= Duration::days(1);
    }
    s <= d && d <= en
}

fn day_colors(d: &Data, date: NaiveDate) -> Vec<Rgba> {
    let mut v: Vec<[u8; 3]> = Vec::new();
    let p = &d.pronote;
    if p.lessons.iter().any(|l| l.start.date() == date && l.bad()) {
        v.push(RED_U8);
    } else if p.lessons.iter().any(|l| l.start.date() == date) {
        v.push(SCHOOL_U8);
    }
    if p.homework.iter().any(|h| h.due == date && !h.done) {
        v.push(ORANGE_U8);
    }
    for e in &d.events {
        if event_on(e, date) && !v.contains(&e.color) {
            v.push(e.color);
            if v.len() == 3 {
                break;
            }
        }
    }
    v.into_iter().map(Rgba::from).collect()
}

struct Item {
    color: Rgba,
    title: String,
    when: String,
    alert: bool,
}

/// « 14:30 », « Demain 14:30 », « Mar. 12  14:30 ».
fn when_label(st: NaiveDateTime, today: NaiveDate) -> String {
    let sd = st.date();
    let hm = st.format("%H:%M").to_string();
    if sd <= today {
        hm
    } else if sd == today + Duration::days(1) {
        format!("Demain {hm}")
    } else {
        format!("{} {}  {hm}", JOURS_C[sd.weekday().num_days_from_monday() as usize], sd.day())
    }
}

/// « aujourd'hui », « demain », « jeudi », « lun. 17 nov. ».
pub fn day_label(d: NaiveDate, today: NaiveDate) -> String {
    let k = (d - today).num_days();
    match k {
        0 => "aujourd'hui".into(),
        1 => "demain".into(),
        2..=6 => JOURS[d.weekday().num_days_from_monday() as usize].to_lowercase(),
        _ => format!("{} {}", JOURS_C[d.weekday().num_days_from_monday() as usize].to_lowercase(), short_date(d)),
    }
}

fn alert_short(l: &Lesson) -> String {
    match l.alert() {
        Alert::Canceled => "annulé".into(),
        Alert::Absent => "prof absent".into(),
        Alert::Room => "changement de salle".into(),
        _ => l.status.to_lowercase(),
    }
}

fn alert_color(l: &Lesson, t: &Theme) -> Rgba {
    match l.alert() {
        Alert::Canceled | Alert::Absent => RED,
        Alert::Room => ORANGE,
        _ => t.accent,
    }
}

/// Tout ce qui se passe un jour donné : cours Pronote, devoirs à rendre, événements Google, vacances.
fn day_items(d: &Data, day: NaiveDate, t: &Theme) -> Vec<Item> {
    let mut v: Vec<(NaiveDateTime, Item)> = Vec::new();
    let midnight = day.and_hms_opt(0, 0, 0).unwrap_or_default();
    if is_holiday(&d.holidays, day) {
        if let Some(h) = d.holidays.iter().find(|h| h.start <= day && day < h.end) {
            v.push((midnight, Item { color: t.accent, title: h.name.clone(), when: "vacances".into(), alert: false }));
        }
    }
    for h in d.pronote.homework.iter().filter(|h| h.due == day) {
        let title = format!("{} : {}", if h.done { "Fait" } else { "À rendre" }, h.subject);
        v.push((midnight, Item { color: ORANGE, title, when: "devoir".into(), alert: false }));
    }
    for l in d.pronote.lessons_on(day) {
        let mut title = l.subject.clone();
        if l.alert() != Alert::None {
            title = format!("{title} — {}", alert_short(l));
        }
        let mut when = l.start.format("%H:%M").to_string();
        if !l.room.is_empty() {
            when = format!("{when} · {}", l.room);
        }
        let color = if l.alert() == Alert::None { Rgba::from(l.color) } else { alert_color(l, t) };
        v.push((l.start, Item { color, title, when, alert: l.bad() }));
    }
    for e in d.events.iter().filter(|e| event_on(e, day)) {
        let when = if e.all_day { "journée".into() } else { e.start.format("%H:%M").to_string() };
        v.push((if e.all_day { midnight } else { e.start }, Item { color: Rgba::from(e.color), title: e.title.clone(), when, alert: false }));
    }
    v.sort_by_key(|(k, _)| *k);
    v.into_iter().map(|(_, i)| i).collect()
}

fn upcoming(d: &Data, now: NaiveDateTime, t: &Theme, n: usize) -> Vec<Item> {
    let today = now.date();
    let mut v: Vec<(NaiveDateTime, Item)> = Vec::new();
    for e in &d.events {
        if e.end > now && e.start < now + Duration::days(14) {
            let sd = e.start.date();
            let when = if e.all_day {
                if sd <= today { "Aujourd'hui".into() } else { short_date(sd) }
            } else {
                when_label(e.start, today)
            };
            v.push((e.start, Item { color: Rgba::from(e.color), title: e.title.clone(), when, alert: false }));
        }
    }
    // Pronote : cours annulés, profs absents, changements de salle (7 prochains jours)
    for l in &d.pronote.lessons {
        if l.alert() != Alert::None && l.end > now && l.start < now + Duration::days(7) {
            let title = format!("{} — {}", l.subject, alert_short(l));
            v.push((l.start, Item { color: alert_color(l, t), title, when: when_label(l.start, today), alert: l.bad() }));
        }
    }
    for h in &d.holidays {
        if h.start <= today && today < h.end {
            let last = h.end - Duration::days(1);
            v.push((now, Item { color: t.accent, title: h.name.clone(), when: format!("jusqu'au {}", short_date(last)), alert: false }));
        } else if h.start > today && (h.start - today).num_days() <= 60 {
            let k = (h.start - today).num_days();
            let when = if k == 1 { "demain".to_string() } else { format!("dans {k} jours") };
            v.push((h.start.and_hms_opt(0, 0, 0).unwrap_or(now), Item { color: t.accent, title: h.name.clone(), when, alert: false }));
        }
    }
    v.sort_by_key(|(k, _)| *k);
    v.into_iter().take(n).map(|(_, i)| i).collect()
}

fn calendar(c: &mut Ctx, cfg: &Config, d: &Data, now: NaiveDateTime, off: i32, sel: Option<NaiveDate>) {
    let t = c.t;
    let today = now.date();
    let mm = today.year() * 12 + today.month0() as i32 + off;
    let (y, m0) = (mm.div_euclid(12), mm.rem_euclid(12) as u32);
    let first = NaiveDate::from_ymd_opt(y, m0 + 1, 1).unwrap_or(today);
    let next = if m0 == 11 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m0 + 2, 1) }
        .unwrap_or(first + Duration::days(30));
    let ndays = (next - first).num_days() as u32;
    let lead = first.weekday().num_days_from_monday();
    let rows = (lead + ndays).div_ceil(7);

    let w1 = c.text_t(Wt::Semibold, 15.0, 22.0, 38.0, t.accent, &MOIS[m0 as usize].to_uppercase(), 0.6);
    c.text(Wt::Semibold, 15.0, 22.0 + w1 + 8.0, 38.0, t.fg2, &y.to_string());
    let (ax, ay) = (W - 22.0 - 26.0, 18.0);
    c.chevron(ax - 30.0 + 13.0, ay + 13.0, 6.0, true, t.fg2);
    c.hit(ax - 30.0, ay, 26.0, 26.0, Action::PrevMonth);
    c.chevron(ax + 13.0, ay + 13.0, 6.0, false, t.fg2);
    c.hit(ax, ay, 26.0, 26.0, Action::NextMonth);
    if off != 0 || sel.is_some_and(|s| s != today) {
        let lw = c.measure(Wt::Semibold, 12.5, "Aujourd'hui") + 18.0;
        let lx = ax - 40.0 - lw;
        c.rr(lx, ay + 1.0, lw, 24.0, 12.0, t.accent.a(0.18));
        c.text_c(Wt::Semibold, 12.5, lx + lw / 2.0, ay + 17.5, t.accent, "Aujourd'hui");
        c.hit(lx, ay, lw, 26.0, Action::Today);
    }

    let gx = 16.0;
    let cw = (W - 32.0) / 7.0;
    for (i, l) in ["L", "M", "M", "J", "V", "S", "D"].iter().enumerate() {
        c.text_c(Wt::Semibold, 11.5, gx + cw * (i as f32 + 0.5), 66.0, t.fg3, l);
    }
    let gy = 76.0;
    let rh = 216.0 / rows as f32;
    let date_of = |idx: u32| -> Option<NaiveDate> {
        if idx < lead || idx >= lead + ndays { None } else { Some(first + Duration::days((idx - lead) as i64)) }
    };
    if cfg.calendar.holidays {
        for r in 0..rows {
            let in_h = |k: u32| date_of(r * 7 + k).map(|dt| is_holiday(&d.holidays, dt)).unwrap_or(false);
            let mut col = 0;
            while col < 7 {
                if in_h(col) {
                    let s0 = col;
                    while col < 7 && in_h(col) {
                        col += 1;
                    }
                    let x0 = gx + cw * s0 as f32 + 3.0;
                    let x1 = gx + cw * col as f32 - 3.0;
                    let cy = gy + rh * (r as f32 + 0.5);
                    c.rr(x0, cy - 15.0, x1 - x0, 30.0, 15.0, t.accent.a(0.17));
                } else {
                    col += 1;
                }
            }
        }
    }
    for day in 1..=ndays {
        let idx = lead + day - 1;
        let (r, col) = (idx / 7, idx % 7);
        let (cx, cy) = (gx + cw * (col as f32 + 0.5), gy + rh * (r as f32 + 0.5));
        let date = first + Duration::days(day as i64 - 1);
        c.hit(cx - cw / 2.0, cy - rh / 2.0, cw, rh, Action::Day(date));
        let num = day.to_string();
        let is_today = date == today;
        if is_today {
            c.circle(cx, cy, 15.0, t.accent);
            c.text_c(Wt::Semibold, 15.0, cx, cy + 5.3, WHITE, &num);
        } else {
            if sel == Some(date) {
                c.ring(cx, cy, 14.5, t.accent, 1.6);
            }
            c.text_c(Wt::Regular, 15.0, cx, cy + 5.3, if col >= 5 { t.fg2 } else { t.fg }, &num);
        }
        let cols = day_colors(d, date);
        let n = cols.len() as f32;
        for (k, col) in cols.iter().enumerate() {
            c.circle(cx + (k as f32 - (n - 1.0) / 2.0) * 6.0, cy + 11.5, 2.0, if is_today { WHITE } else { *col });
        }
    }

    if cfg.calendar.agenda {
        let y0 = 302.0;
        c.hline(22.0, W - 22.0, y0, t.sep);
        let head = match sel {
            Some(day) if (0..=6).contains(&(day - today).num_days()) => format!("{} {}", day_label(day, today), short_date(day)),
            Some(day) => format!("{} {}", JOURS[day.weekday().num_days_from_monday() as usize], short_date(day)),
            None => "À venir".into(),
        };
        c.text_t(Wt::Semibold, 11.5, 22.0, y0 + 27.0, if sel.is_some() { t.accent } else { t.fg2 }, &head.to_uppercase(), 0.8);
        let items = match sel {
            Some(day) => day_items(d, day, t),
            None => upcoming(d, now, t, 3),
        };
        if items.len() > 3 && sel.is_some() {
            c.text_r(Wt::Regular, 12.0, W - 22.0, y0 + 27.0, t.fg3, &format!("+{} autres", items.len() - 3));
        }
        if items.is_empty() {
            let msg = if sel.is_some() { "Rien ce jour-là" } else { "Rien de prévu pour l'instant" };
            c.text(Wt::Regular, 14.0, 22.0, y0 + 60.0, t.fg3, msg);
        }
        for (i, it) in items.iter().take(3).enumerate() {
            let ry = y0 + 40.0 + i as f32 * 26.0;
            c.rr(22.0, ry, 3.5, 19.0, 1.75, it.color);
            let ww = c.measure(Wt::Regular, 13.0, &it.when);
            let title = c.fit(Wt::Semibold, 14.0, &it.title, W - 34.0 - 22.0 - ww - 12.0);
            c.text(Wt::Semibold, 14.0, 34.0, ry + 14.5, if it.alert { RED } else { t.fg }, &title);
            c.text_r(Wt::Regular, 13.0, W - 22.0, ry + 14.5, t.fg2, &it.when);
        }
    }
}

// ---------- Compte à rebours ----------

fn period(h: &Holiday) -> String {
    let last = h.end - Duration::days(1);
    if last <= h.start { short_date(h.start) } else { format!("{} – {}", short_date(h.start), short_date(last)) }
}

#[allow(clippy::too_many_arguments)]
fn draw_count(c: &mut Ctx, label: &str, n: i64, name: &str, dates: &str, p: f32) {
    let t = c.t;
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, label, 0.8);
    let w = c.text(Wt::Light, 50.0, 19.0, 88.0, t.fg, &n.to_string());
    c.text(Wt::Regular, 17.0, 19.0 + w + 6.0, 88.0, t.fg2, if n > 1 { "jours" } else { "jour" });
    let nm = c.fit(Wt::Semibold, 15.0, name, 175.0);
    c.text_r(Wt::Semibold, 15.0, W - 22.0, 62.0, t.fg, &nm);
    c.text_r(Wt::Regular, 13.0, W - 22.0, 84.0, t.fg2, dates);
    c.rr(22.0, 104.0, W - 44.0, 6.0, 3.0, t.track);
    c.rr(22.0, 104.0, ((W - 44.0) * p.clamp(0.0, 1.0)).max(6.0), 6.0, 3.0, t.accent);
}

fn countdown(c: &mut Ctx, cfg: &Config, d: &Data, now: NaiveDateTime) {
    let t = c.t;
    let today = now.date();
    let hs = &d.holidays;
    let frac = |a: i64, b: i64| if b <= 0 { 0.0 } else { a as f32 / b as f32 };
    if hs.is_empty() {
        c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "VACANCES", 0.8);
        c.text(Wt::Regular, 14.0, 22.0, 76.0, t.fg2, "Chargement du calendrier scolaire…");
    } else if let Some(h) = hs.iter().find(|h| h.start <= today && today < h.end) {
        let p = frac((today - h.start).num_days(), (h.end - h.start).num_days());
        draw_count(c, "REPRISE DES COURS DANS", (h.end - today).num_days(), &h.name, &period(h), p);
    } else if let Some(n) = hs.iter().find(|h| h.start > today) {
        let prev = hs.iter().filter(|h| h.end <= today).map(|h| h.end).max().unwrap_or(n.start - Duration::days(49));
        let p = frac((today - prev).num_days(), (n.start - prev).num_days());
        draw_count(c, "PROCHAINES VACANCES DANS", (n.start - today).num_days(), &n.name, &period(n), p);
    } else {
        c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "VACANCES", 0.8);
        c.text(Wt::Regular, 14.0, 22.0, 76.0, t.fg2, "Aucune période à venir trouvée");
    }
    if let Some(ed) = exam_date(cfg) {
        let y = 128.0;
        c.hline(22.0, W - 22.0, y, t.sep);
        let name = cfg.countdown.exam_name.trim();
        c.text(Wt::Semibold, 14.5, 22.0, y + 28.0, t.fg, if name.is_empty() { "Examen" } else { name });
        let n = (ed - today).num_days();
        let s = match n {
            i64::MIN..=-1 => "terminé".to_string(),
            0 => "c'est aujourd'hui !".to_string(),
            1 => format!("demain · {}", short_date(ed)),
            _ => format!("dans {n} jours · {}", short_date(ed)),
        };
        c.text_r(Wt::Regular, 13.5, W - 22.0, y + 28.0, t.fg2, &s);
    }
}

// ---------- Mails ----------

pub(crate) const RED: Rgba = Rgba(1.0, 0.27, 0.23, 1.0);
pub(crate) const ORANGE: Rgba = Rgba(1.0, 0.62, 0.04, 1.0);
pub(crate) const GREEN: Rgba = Rgba(0.2, 0.78, 0.35, 1.0);
const RED_U8: [u8; 3] = [255, 69, 58];
const ORANGE_U8: [u8; 3] = [255, 159, 10];
const SCHOOL_U8: [u8; 3] = [90, 200, 250];

fn mail(c: &mut Ctx, d: &Data) {
    let t = c.t;
    let mb = &d.mail;
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "MAILS", 0.8);
    if mb.accounts == 0 {
        c.text(Wt::Regular, 14.0, 22.0, 70.0, t.fg2, "Connecte Gmail ou Hotmail");
        c.text(Wt::Regular, 14.0, 22.0, 90.0, t.fg2, "dans Paramètres › Comptes.");
        c.hit(0.0, 0.0, W, 300.0, Action::Accounts);
        return;
    }
    let n = mb.mails.len();
    c.text_r(Wt::Semibold, 12.5, W - 22.0, 32.0, t.fg2, &format!("{n} non lu{}", if n > 1 { "s" } else { "" }));
    let mut y = 46.0;
    let urgent = mb.mails.iter().filter(|m| m.urgent).count();
    if urgent > 0 {
        let label = format!("{urgent} urgent{}", if urgent > 1 { "s" } else { "" });
        let w = c.measure(Wt::Semibold, 12.5, &label) + 22.0;
        c.rr(22.0, y, w, 24.0, 12.0, RED);
        c.text(Wt::Semibold, 12.5, 33.0, y + 16.5, WHITE, &label);
        // la pastille ouvre le mail urgent le plus récent (la liste est triée du plus récent au plus ancien)
        if let Some(i) = mb.mails.iter().position(|m| m.urgent) {
            c.hit(22.0, y, w, 24.0, Action::OpenMail(i as u16));
        }
        y += 34.0;
    }
    if n == 0 {
        c.text(Wt::Regular, 14.0, 22.0, y + 22.0, t.fg2, "Aucun mail non lu. Tranquille !");
        c.hit(0.0, 0.0, W, 300.0, Action::OpenInbox);
        return;
    }
    if !mb.summary.is_empty() {
        let lines = c.wrap(Wt::Regular, 13.5, &mb.summary, W - 44.0);
        for (i, l) in lines.iter().take(4).enumerate() {
            c.text(Wt::Regular, 13.5, 22.0, y + 16.0 + i as f32 * 19.0, t.fg, l);
        }
        y += lines.len().min(4) as f32 * 19.0 + 12.0;
    }
    c.hline(22.0, W - 22.0, y, t.sep);
    y += 6.0;
    for (i, m) in mb.mails.iter().enumerate() {
        if y + 38.0 > 300.0 - 8.0 {
            break;
        }
        c.hit(14.0, y, W - 28.0, 38.0, Action::OpenMail(i as u16));
        let dot = match m.cat {
            Cat::Important => RED,
            Cat::Reply => ORANGE,
            Cat::Notif | Cat::Promo | Cat::Spam => t.fg3,
            Cat::None => t.accent,
        };
        c.circle(26.0, y + 11.0, 3.5, dot);
        let when = m.date.format("%H:%M").to_string();
        let ww = c.measure(Wt::Regular, 12.0, &when);
        let from = c.fit(Wt::Semibold, 13.5, &m.from, W - 36.0 - 22.0 - ww - 10.0);
        c.text(Wt::Semibold, 13.5, 36.0, y + 15.5, t.fg, &from);
        c.text_r(Wt::Regular, 12.0, W - 22.0, y + 15.5, t.fg3, &when);
        let tag = if matches!(m.cat, Cat::None | Cat::Important) { "" } else { m.cat.label() };
        let tw = if tag.is_empty() { 0.0 } else { c.measure(Wt::Regular, 11.0, tag) + 8.0 };
        let subj = c.fit(Wt::Regular, 12.5, if m.subject.is_empty() { &m.snippet } else { &m.subject }, W - 58.0 - tw);
        c.text(Wt::Regular, 12.5, 36.0, y + 32.0, t.fg2, &subj);
        if !tag.is_empty() {
            c.text_r(Wt::Regular, 11.0, W - 22.0, y + 32.0, if m.cat == Cat::Reply { ORANGE } else { t.fg3 }, tag);
        }
        y += 38.0;
    }
    // ailleurs sur le widget : la boîte de réception
    c.hit(0.0, 0.0, W, 300.0, Action::OpenInbox);
}

// ---------- Musique ----------

fn music(c: &mut Ctx, d: &Data) {
    let t = c.t;
    let Some(m) = &d.media else {
        c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "MUSIQUE", 0.8);
        c.text(Wt::Regular, 14.0, 22.0, 66.0, t.fg2, "Rien en lecture pour l'instant");
        return;
    };
    match &m.art {
        Some(img) => c.image(img, 16.0, 16.0, 72.0, 72.0, 12.0),
        None => {
            c.rr(16.0, 16.0, 72.0, 72.0, 12.0, t.fg.a(0.1));
            c.text_c(Wt::Regular, 30.0, 52.0, 63.0, t.fg3, "♪");
        }
    }
    let tx = 102.0;
    let title = c.fit(Wt::Semibold, 15.0, &m.title, W - tx - 18.0);
    c.text(Wt::Semibold, 15.0, tx, 37.0, t.fg, &title);
    let artist = c.fit(Wt::Regular, 13.0, &m.artist, W - tx - 18.0);
    c.text(Wt::Regular, 13.0, tx, 56.0, t.fg2, &artist);
    let cy = 78.0;
    let (a, b, n) = (tx + 12.0, tx + 52.0, tx + 92.0);
    c.rr(a - 7.0, cy - 6.5, 2.4, 13.0, 1.2, t.fg);
    c.poly(&[(a + 7.0, cy - 6.5), (a + 7.0, cy + 6.5), (a - 4.0, cy)], t.fg);
    if m.playing {
        c.rr(b - 6.0, cy - 8.0, 4.2, 16.0, 1.5, t.fg);
        c.rr(b + 1.8, cy - 8.0, 4.2, 16.0, 1.5, t.fg);
    } else {
        c.poly(&[(b - 5.0, cy - 8.5), (b - 5.0, cy + 8.5), (b + 8.5, cy)], t.fg);
    }
    c.poly(&[(n - 7.0, cy - 6.5), (n - 7.0, cy + 6.5), (n + 4.0, cy)], t.fg);
    c.rr(n + 4.6, cy - 6.5, 2.4, 13.0, 1.2, t.fg);
    c.hit(a - 16.0, cy - 14.0, 32.0, 28.0, Action::Prev);
    c.hit(b - 18.0, cy - 14.0, 36.0, 28.0, Action::Toggle);
    c.hit(n - 16.0, cy - 14.0, 32.0, 28.0, Action::Next);
    let app = c.fit(Wt::Regular, 11.5, &m.app, 90.0);
    c.text_r(Wt::Regular, 11.5, W - 18.0, cy + 4.0, t.fg3, &app);
}

// ---------- Discord ----------

fn discord(c: &mut Ctx, cfg: &Config, d: &Data) {
    let t = c.t;
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "DISCORD", 0.8);
    let dc = &cfg.discord;
    let head = if dc.dm_only { "MESSAGES PRIVÉS" } else { "DISCORD" };
    c.text_t(Wt::Semibold, 11.5, 22.0 + c.measure(Wt::Semibold, 11.5, "DISCORD") + 14.0, 32.0, t.fg3, head, 0.8);
    let st = if !d.winnotif_state.is_empty() && d.winnotif_state != "activées" {
        d.winnotif_state.clone()
    } else if !d.discord_state.is_empty() {
        d.discord_state.clone()
    } else {
        d.winnotif_state.clone()
    };
    let state = c.fit(Wt::Regular, 12.0, &st, 120.0);
    c.text_r(Wt::Regular, 12.0, W - 22.0, 32.0, t.fg3, &state);
    if dc.id.trim().is_empty() && !dc.win_notifs {
        c.text(Wt::Regular, 14.0, 22.0, 70.0, t.fg2, "Active « Lire les notifications");
        c.text(Wt::Regular, 14.0, 22.0, 90.0, t.fg2, "Discord de Windows » dans Paramètres.");
        c.hit(0.0, 0.0, W, 236.0, Action::Accounts);
        return;
    }
    if d.winnotif_state.starts_with("accès refusé") {
        c.text(Wt::Regular, 13.0, 22.0, 70.0, t.fg2, "Autorise Givre à lire les notifications :");
        c.text(Wt::Regular, 13.0, 22.0, 90.0, t.fg2, "Paramètres Windows › Confidentialité");
        c.text(Wt::Regular, 13.0, 22.0, 110.0, t.fg2, "› Notifications (applis de bureau).");
    }
    let important: Vec<_> = d.discord.iter().filter(|n| dc.important(&n.title, &n.body)).collect();
    if important.is_empty() {
        if !d.winnotif_state.starts_with("accès refusé") {
            c.text(Wt::Regular, 14.0, 22.0, 70.0, t.fg3, "Aucun message important");
        }
        return;
    }
    for (i, n) in important.iter().rev().take(5).enumerate() {
        let ry = 44.0 + i as f32 * 37.0;
        let today = chrono::Local::now().date_naive();
        let when = if n.time.date() == today { n.time.format("%H:%M") } else { n.time.format("%d/%m") }.to_string();
        let ww = c.measure(Wt::Regular, 12.0, &when);
        let title = c.fit(Wt::Semibold, 13.5, &n.title, W - 44.0 - ww - 10.0);
        c.text(Wt::Semibold, 13.5, 22.0, ry + 14.0, t.fg, &title);
        c.text_r(Wt::Regular, 12.0, W - 22.0, ry + 14.0, t.fg3, &when);
        let body = c.fit(Wt::Regular, 12.5, &n.body.replace('\n', " "), W - 44.0);
        c.text(Wt::Regular, 12.5, 22.0, ry + 31.0, t.fg2, &body);
    }
}

// ---------- Imprimante 3D ----------

fn printer(c: &mut Ctx, d: &Data) {
    let t = c.t;
    let p = &d.printer;
    c.hit(0.0, 0.0, W, 120.0, Action::OpenPrinter);
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "IMPRIMANTE 3D", 0.8);
    if !p.online {
        c.text(Wt::Regular, 14.0, 22.0, 68.0, t.fg2, "Hors ligne");
        c.text(Wt::Regular, 12.5, 22.0, 92.0, t.fg3, "Clic : ouvrir le widget vidéo");
        return;
    }
    let active = is_active(p.status);
    c.text_r(Wt::Semibold, 12.5, W - 22.0, 32.0, if active { t.accent } else { t.fg2 }, status_label(p.status));
    let name = p.file.trim_end_matches(".gcode");
    let name = c.fit(Wt::Semibold, 15.0, if name.is_empty() { "Aucune impression" } else { name }, W - 100.0);
    c.text(Wt::Semibold, 15.0, 22.0, 60.0, t.fg, &name);
    let pct = p.progress.clamp(0, 100);
    c.text_r(Wt::Semibold, 15.0, W - 22.0, 60.0, t.fg, &format!("{pct} %"));
    c.rr(22.0, 72.0, W - 44.0, 6.0, 3.0, t.track);
    c.rr(22.0, 72.0, ((W - 44.0) * pct as f32 / 100.0).max(6.0), 6.0, 3.0, t.accent);
    let info = if active {
        let left = if p.left_min >= 60 { format!("{} h {:02}", p.left_min / 60, p.left_min % 60) } else { format!("{} min", p.left_min) };
        format!("Couche {} / {}  ·  reste {left}", p.layer, p.total)
    } else {
        "Clic : vidéo en direct".to_string()
    };
    c.text(Wt::Regular, 13.0, 22.0, 100.0, t.fg2, &info);
}

// ---------- Pronote ----------

fn no_pronote(c: &mut Ctx, label: &str, h: f32) {
    let t = c.t;
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, label, 0.8);
    c.text(Wt::Regular, 14.0, 22.0, 70.0, t.fg2, "Connecte Pronote dans");
    c.text(Wt::Regular, 14.0, 22.0, 90.0, t.fg2, "Paramètres › Comptes.");
    c.hit(0.0, 0.0, W, h, Action::Accounts);
}

fn dur_txt(m: i64) -> String {
    if m < 60 { format!("{m} min") } else { format!("{} h {:02}", m / 60, m % 60) }
}

/// « dans 17 min », « fin dans 25 min », « demain 08:30 »…
pub(crate) fn countdown_txt(l: &Lesson, now: NaiveDateTime) -> String {
    let mins = |a: NaiveDateTime, b: NaiveDateTime| ((b - a).num_seconds() + 59).max(0) / 60;
    if l.start <= now {
        return format!("fin dans {}", dur_txt(mins(now, l.end)));
    }
    if l.start.date() == now.date() {
        let m = mins(now, l.start);
        if m <= 0 { "maintenant".into() } else { format!("dans {}", dur_txt(m)) }
    } else {
        format!("{} {}", day_label(l.start.date(), now.date()), l.start.format("%H:%M"))
    }
}

pub(crate) fn room_txt(r: &str) -> String {
    if r.to_lowercase().starts_with("salle") { r.to_string() } else { format!("Salle {r}") }
}

fn next_class(c: &mut Ctx, d: &Data, now: NaiveDateTime) {
    let t = c.t;
    let p = &d.pronote;
    if !p.connected {
        return no_pronote(c, "PROCHAIN COURS", 156.0);
    }
    let (cur, after) = p.next(now);
    let Some(l) = cur else {
        c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "PROCHAIN COURS", 0.8);
        let msg = if p.updated.is_none() { "Chargement de l'emploi du temps…" } else { "Aucun cours prévu d'ici 3 semaines" };
        c.text(Wt::Regular, 14.0, 22.0, 70.0, t.fg2, msg);
        if !p.error.is_empty() {
            let e = c.fit(Wt::Regular, 12.5, &p.error, W - 44.0);
            c.text(Wt::Regular, 12.5, 22.0, 96.0, RED, &e);
        }
        return;
    };
    let ongoing = l.start <= now;
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, if ongoing { "EN CE MOMENT" } else { "PROCHAIN COURS" }, 0.8);
    let soon = !ongoing && (l.start - now).num_minutes() < 10;
    c.text_r(Wt::Semibold, 13.5, W - 22.0, 32.0, if soon { t.accent } else { t.fg }, &countdown_txt(l, now));
    let bad = l.bad();
    c.rr(22.0, 48.0, 4.0, 50.0, 2.0, Rgba::from(l.color));
    let sub = c.fit(Wt::Semibold, 21.0, &l.subject, W - 58.0);
    let sw = c.text(Wt::Semibold, 21.0, 36.0, 70.0, if bad { t.fg2 } else { t.fg }, &sub);
    if bad {
        c.line(36.0, 63.0, 36.0 + sw, 63.0, 1.6, RED);
    }
    let mut info = format!("{} – {}", l.start.format("%H:%M"), l.end.format("%H:%M"));
    if !l.room.is_empty() {
        info = format!("{info}  ·  {}", room_txt(&l.room));
    }
    let info = c.fit(Wt::Regular, 13.5, &info, W - 58.0);
    c.text(Wt::Regular, 13.5, 36.0, 92.0, t.fg2, &info);
    let mut x = 22.0;
    if l.alert() != Alert::None {
        let lab = l.alert_label();
        let lab = c.fit(Wt::Semibold, 12.0, &lab, W - 44.0);
        let w = c.measure(Wt::Semibold, 12.0, &lab) + 20.0;
        let col = alert_color(l, t);
        c.rr(22.0, 106.0, w, 22.0, 11.0, col);
        c.text(Wt::Semibold, 12.0, 32.0, 121.5, WHITE, &lab);
        x += w + 10.0;
    }
    if !l.teacher.is_empty() && x < W - 80.0 {
        let tch = c.fit(Wt::Regular, 13.0, &l.teacher, W - 22.0 - x);
        c.text(Wt::Regular, 13.0, x, 121.5, t.fg2, &tch);
    }
    if let Some(n) = after {
        let mut s = format!("Ensuite : {}  ·  {}", n.subject, n.start.format("%H:%M"));
        if n.start.date() != l.start.date() {
            s = format!("Ensuite : {}  ·  {} {}", n.subject, day_label(n.start.date(), now.date()), n.start.format("%H:%M"));
        }
        if n.alert() != Alert::None {
            s = format!("{s}  ({})", alert_short(n));
        }
        let s = c.fit(Wt::Regular, 12.5, &s, W - 44.0);
        c.text(Wt::Regular, 12.5, 22.0, 146.0, if n.bad() { RED } else { t.fg3 }, &s);
    }
}

fn homework(c: &mut Ctx, d: &Data, now: NaiveDateTime) {
    let t = c.t;
    let p = &d.pronote;
    if !p.connected {
        return no_pronote(c, "DEVOIRS", 300.0);
    }
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "DEVOIRS", 0.8);
    let todo = p.homework.iter().filter(|h| !h.done).count();
    let right = match todo {
        0 if !p.homework.is_empty() => "tout est fait".to_string(),
        0 => String::new(),
        n => format!("{n} à faire"),
    };
    c.text_r(Wt::Semibold, 12.5, W - 22.0, 32.0, if todo > 0 { t.accent } else { GREEN }, &right);
    if p.homework.is_empty() {
        let msg = if p.updated.is_none() { "Chargement des devoirs…" } else { "Aucun devoir à venir. Profites-en !" };
        c.text(Wt::Regular, 14.0, 22.0, 70.0, t.fg2, msg);
        return;
    }
    let today = now.date();
    let mut y = 44.0;
    let mut last: Option<NaiveDate> = None;
    let mut shown = 0;
    for (i, h) in p.homework.iter().enumerate() {
        let head = last != Some(h.due);
        if y + if head { 22.0 } else { 0.0 } + 40.0 > 300.0 - 6.0 {
            break;
        }
        if head {
            let lab = format!("POUR {}", day_label(h.due, today).to_uppercase());
            c.text_t(Wt::Semibold, 11.0, 22.0, y + 15.0, if h.due <= today + Duration::days(1) { ORANGE } else { t.fg3 }, &lab, 0.5);
            y += 22.0;
            last = Some(h.due);
        }
        let cy = y + 12.0;
        if h.done {
            c.circle(31.0, cy, 9.0, t.accent);
            c.line(26.8, cy + 0.2, 29.8, cy + 3.4, 2.0, WHITE);
            c.line(29.8, cy + 3.4, 35.4, cy - 3.2, 2.0, WHITE);
        } else {
            c.ring(31.0, cy, 8.5, t.fg3, 1.6);
        }
        let sub = c.fit(Wt::Semibold, 13.5, &h.subject, W - 70.0);
        c.text(Wt::Semibold, 13.5, 48.0, y + 16.5, if h.done { t.fg3 } else { t.fg }, &sub);
        let txt = c.fit(Wt::Regular, 12.5, &h.text, W - 70.0);
        c.text(Wt::Regular, 12.5, 48.0, y + 33.0, if h.done { t.fg3 } else { t.fg2 }, &txt);
        c.hit(12.0, y - 2.0, W - 24.0, 40.0, Action::Homework(i as u16));
        y += 40.0;
        shown += 1;
    }
    if shown < p.homework.len() {
        c.text_r(Wt::Regular, 12.0, W - 22.0, 292.0, t.fg3, &format!("+{} autres", p.homework.len() - shown));
    }
}

pub(crate) fn grade_color(g: &crate::pronote::Grade, t: &Theme) -> Rgba {
    let (Some(v), Some(o)) = (grade_num(&g.value), grade_num(&g.out_of)) else { return t.fg2 };
    let x = if o > 0.0 { v / o * 20.0 } else { v };
    if x >= 14.0 {
        GREEN
    } else if x < 10.0 {
        RED
    } else {
        t.fg
    }
}

fn grades(c: &mut Ctx, d: &Data) {
    let t = c.t;
    let p = &d.pronote;
    if !p.connected {
        return no_pronote(c, "NOTES", 300.0);
    }
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "NOTES", 0.8);
    let per = c.fit(Wt::Regular, 12.5, &p.period, 180.0);
    c.text_r(Wt::Regular, 12.5, W - 22.0, 32.0, t.fg3, &per);
    let mut y = 44.0;
    if !p.overall.is_empty() {
        let w = c.text(Wt::Light, 44.0, 19.0, 90.0, t.fg, &grade_txt(&p.overall));
        c.text(Wt::Regular, 16.0, 19.0 + w + 4.0, 90.0, t.fg2, "/20");
        c.text_r(Wt::Semibold, 13.5, W - 22.0, 66.0, t.fg, "Moyenne générale");
        if !p.class_overall.is_empty() {
            c.text_r(Wt::Regular, 13.0, W - 22.0, 86.0, t.fg2, &format!("classe : {}", grade_txt(&p.class_overall)));
        }
        y = 106.0;
        c.hline(22.0, W - 22.0, y, t.sep);
        y += 4.0;
    }
    if p.grades.is_empty() {
        let msg = if p.updated.is_none() { "Chargement des notes…" } else { "Pas encore de note sur cette période" };
        c.text(Wt::Regular, 14.0, 22.0, y + 26.0, t.fg2, msg);
        return;
    }
    for g in &p.grades {
        if y + 36.0 > 300.0 - 4.0 {
            break;
        }
        let val = grade_txt(&g.value);
        let out = format!("/{}", g.out_of);
        let ow = c.measure(Wt::Regular, 12.0, &out);
        let vw = c.measure(Wt::Semibold, 16.0, &val);
        c.text_r(Wt::Regular, 12.0, W - 22.0, y + 19.0, t.fg3, &out);
        c.text_r(Wt::Semibold, 16.0, W - 22.0 - ow - 2.0, y + 19.0, grade_color(g, t), &val);
        let sub = c.fit(Wt::Semibold, 13.5, &g.subject, W - 44.0 - ow - vw - 14.0);
        c.text(Wt::Semibold, 13.5, 22.0, y + 15.5, t.fg, &sub);
        let mut info = short_date(g.date);
        if !g.class_avg.is_empty() {
            info = format!("{info}  ·  classe {}", grade_txt(&g.class_avg));
        }
        if !g.coef.is_empty() && g.coef != "1" {
            info = format!("{info}  ·  coef. {}", g.coef);
        }
        c.text(Wt::Regular, 12.0, 22.0, y + 31.0, t.fg3, &info);
        y += 36.0;
    }
}

fn school(c: &mut Ctx, d: &Data) {
    let t = c.t;
    let p = &d.pronote;
    if !p.connected {
        return no_pronote(c, "PRONOTE", 248.0);
    }
    c.hit(0.0, 0.0, W, 248.0, Action::OpenPronote);
    c.text_t(Wt::Semibold, 11.5, 22.0, 32.0, t.fg2, "PRONOTE", 0.8);
    let unread = p.unread();
    if unread > 0 {
        let lab = format!("{unread} message{} non lu{}", if unread > 1 { "s" } else { "" }, if unread > 1 { "s" } else { "" });
        c.text_r(Wt::Semibold, 12.5, W - 22.0, 32.0, t.accent, &lab);
    } else if let Some(u) = p.updated {
        c.text_r(Wt::Regular, 12.0, W - 22.0, 32.0, t.fg3, &format!("à jour {}", u.format("%H:%M")));
    }
    let mut rows: Vec<(bool, String, String, String)> = Vec::new();
    for m in p.talks.iter().filter(|m| m.unread > 0).take(3) {
        rows.push((true, m.subject.clone(), m.when.clone(), format!("Message de {}", m.from)));
    }
    for n in &p.news {
        let sub = if n.text.is_empty() { n.author.clone() } else { format!("{} · {}", n.author, n.text) };
        rows.push((!n.read, n.title.clone(), short_date(n.date), sub));
    }
    let max = if p.error.is_empty() { 5 } else { 4 };
    if rows.is_empty() {
        c.text(Wt::Regular, 14.0, 22.0, 70.0, t.fg3, "Aucune annonce ni message");
    }
    for (i, (bold, title, when, sub)) in rows.iter().take(max).enumerate() {
        let ry = 44.0 + i as f32 * 39.0;
        let ww = c.measure(Wt::Regular, 12.0, when);
        if *bold {
            c.circle(26.0, ry + 9.5, 3.5, t.accent);
        }
        let tt = c.fit(if *bold { Wt::Semibold } else { Wt::Regular }, 13.5, title, W - 36.0 - 22.0 - ww - 10.0);
        c.text(if *bold { Wt::Semibold } else { Wt::Regular }, 13.5, 36.0, ry + 14.0, t.fg, &tt);
        c.text_r(Wt::Regular, 12.0, W - 22.0, ry + 14.0, t.fg3, when);
        let s = c.fit(Wt::Regular, 12.5, sub, W - 58.0);
        c.text(Wt::Regular, 12.5, 36.0, ry + 31.0, t.fg2, &s);
    }
    if !p.error.is_empty() {
        let e = c.fit(Wt::Regular, 12.0, &p.error, W - 44.0);
        c.text(Wt::Regular, 12.0, 22.0, 236.0, RED, &e);
    }
}
