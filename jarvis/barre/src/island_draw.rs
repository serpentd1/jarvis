//! Dessin de la Dynamic Island : pilule, île ouverte (modules), pages de détail, notifications,
//! alarme, effets (verre, lueur, fondu) et options rigolotes.

use chrono::{Datelike, NaiveDateTime, Timelike};
use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, GradientStop, LinearGradient, Paint, Pattern, Pixmap, PixmapPaint, Point, SpreadMode, Stroke,
    Transform,
};

use crate::backdrop::Backdrop;
use crate::config::{Config, IleCfg};
use crate::data::{Cat, Data, Mail, wmo_label};
use crate::gfx::{self, Fonts, Rgba, Theme, WHITE, Wt};
use crate::island::*;
use crate::pronote::grade_txt;
use crate::widgets::{self, Action, Ctx, GREEN, Hit, JOURS, JOURS_C, MOIS, MOIS_C, ORANGE, RED};

pub struct Preset {
    #[allow(dead_code)]
    pub name: &'static str,
    pub c0: [u8; 3],
    pub c1: [u8; 3],
    pub accent: [u8; 3],
    pub light: bool,
    /// Décor animé du thème (réglage « Décors animés du thème ») : 0 aucun, 1 neige et guirlande (Noël),
    /// 2 chauves-souris (Halloween), 3 éclats de givre (Glace), 4 pétales (Sakura), 5 bulles (Plage),
    /// 6 étoiles et étoile filante (Galaxie), 7 grille néon (Rétro 80).
    pub deco: u8,
}

pub const PRESETS: [Preset; 20] = [
    Preset { name: "Nuit", c0: [10, 10, 12], c1: [26, 26, 32], accent: [52, 199, 89], light: false, deco: 0 },
    Preset { name: "Verre", c0: [22, 24, 34], c1: [44, 48, 66], accent: [120, 180, 255], light: false, deco: 0 },
    Preset { name: "Aurore", c0: [18, 12, 44], c1: [8, 74, 84], accent: [110, 240, 190], light: false, deco: 0 },
    Preset { name: "Océan", c0: [4, 24, 54], c1: [0, 86, 130], accent: [90, 200, 250], light: false, deco: 0 },
    Preset { name: "Forêt", c0: [10, 32, 18], c1: [38, 72, 36], accent: [150, 225, 100], light: false, deco: 0 },
    Preset { name: "Coucher de soleil", c0: [72, 16, 52], c1: [172, 72, 30], accent: [255, 175, 70], light: false, deco: 0 },
    Preset { name: "Bonbon", c0: [92, 30, 102], c1: [40, 62, 142], accent: [255, 130, 210], light: false, deco: 0 },
    Preset { name: "Néon", c0: [6, 4, 14], c1: [24, 6, 38], accent: [0, 255, 210], light: false, deco: 0 },
    Preset { name: "Clair", c0: [248, 248, 252], c1: [226, 230, 244], accent: [10, 132, 255], light: true, deco: 0 },
    Preset { name: "Café", c0: [40, 28, 20], c1: [74, 52, 36], accent: [235, 180, 120], light: false, deco: 0 },
    // 1.4 : dix thèmes de plus
    Preset { name: "Neutre", c0: [36, 36, 38], c1: [54, 54, 58], accent: [152, 152, 160], light: false, deco: 0 },
    Preset { name: "Pomme clair", c0: [246, 246, 248], c1: [234, 234, 239], accent: [0, 113, 227], light: true, deco: 0 },
    Preset { name: "Pomme sombre", c0: [28, 28, 30], c1: [44, 44, 46], accent: [10, 132, 255], light: false, deco: 0 },
    Preset { name: "Noël", c0: [110, 14, 24], c1: [14, 74, 40], accent: [255, 210, 90], light: false, deco: 1 },
    Preset { name: "Halloween", c0: [24, 10, 34], c1: [84, 32, 6], accent: [255, 140, 20], light: false, deco: 2 },
    Preset { name: "Glace", c0: [226, 242, 252], c1: [188, 218, 242], accent: [0, 138, 206], light: true, deco: 3 },
    Preset { name: "Sakura", c0: [255, 232, 241], c1: [246, 200, 220], accent: [214, 64, 124], light: true, deco: 4 },
    Preset { name: "Plage", c0: [0, 92, 124], c1: [18, 150, 158], accent: [255, 214, 110], light: false, deco: 5 },
    Preset { name: "Galaxie", c0: [8, 6, 30], c1: [44, 16, 78], accent: [186, 146, 255], light: false, deco: 6 },
    Preset { name: "Rétro 80", c0: [26, 6, 46], c1: [96, 12, 84], accent: [255, 64, 196], light: false, deco: 7 },
];

/// Noms des thèmes (réglage « Thème »), dans l'ordre de PRESETS.
pub const PRESET_NAMES: [&str; 20] = [
    "Nuit", "Verre", "Aurore", "Océan", "Forêt", "Coucher de soleil", "Bonbon", "Néon", "Clair", "Café",
    "Neutre", "Pomme clair", "Pomme sombre", "Noël", "Halloween", "Glace", "Sakura", "Plage", "Galaxie", "Rétro 80",
];

pub const ACCENT_NAMES: [&str; 10] = ["Celle du thème", "Celle de Jarvis", "Vert", "Bleu", "Violet", "Rose", "Orange", "Rouge", "Jaune", "Turquoise"];
const ACCENTS: [[u8; 3]; 8] = [[52, 199, 89], [10, 132, 255], [175, 82, 222], [255, 55, 95], [255, 149, 0], [255, 59, 48], [255, 204, 0], [48, 210, 200]];

pub(crate) const COMPLIMENTS: [&str; 12] = [
    "Tu gères !",
    "Belle journée pour apprendre un truc.",
    "Ton futur toi te remercie.",
    "Rien ne t'arrête.",
    "Tu es plus fort que les maths.",
    "Le boss du collège, c'est toi.",
    "Chaque effort compte.",
    "Aujourd'hui, c'est ton jour.",
    "Tu as un super goût pour les barres.",
    "Même le Wi-Fi t'admire.",
    "Pause bien méritée bientôt.",
    "Garde le cap, champion !",
];

/// Tout ce qu'il faut pour dessiner l'île.
pub struct R<'a> {
    pub cfg: &'a Config,
    pub fonts: &'a Fonts,
    pub dpi: f32,
    pub backdrop: Option<&'a Backdrop>,
    pub d: &'a Data,
    pub st: &'a Island,
    pub clocks: &'a Clocks,
    pub chat: &'a Chat,
    pub ring: Option<&'a Ring>,
    pub jarvis: &'a Jarvis,
    pub notifs: &'a [INotif],
    pub discord_unread: Option<u32>,
    pub now: NaiveDateTime,
    pub sw: i32,
    pub mouse: (i32, i32),
    pub model: String,
    /// Commandes traitées par Jarvis aujourd'hui.
    pub jarvis_cmds: u32,
    /// Derniers textes copiés (onglet presse-papiers).
    pub clips: &'a [String],
    /// Question posée à Jarvis sur le mail ouvert.
    pub mail_ai: &'a MailAi,
    /// Données de Jarvis (tâches, rappels…), carte mentale et état des sections.
    pub jd: &'a crate::jarvis_data::JData,
    pub mem: &'a crate::jarvis_data::MemGraph,
    pub ui: &'a crate::island_big::BigUi,
    /// Hauteur de l'écran (la grande île doit tenir).
    pub sh: i32,
    /// Message d'état (connexion Pronote, Google…).
    pub status: &'a str,
    /// Comptes, Pronote et IA pour la grande appli › Paramètres (seulement quand une page en a besoin).
    pub info: Option<&'a crate::settings::Info>,
}

impl R<'_> {
    pub(crate) fn c(&self) -> &IleCfg {
        &self.cfg.ile
    }
    pub(crate) fn t(&self) -> f32 {
        self.st.secs()
    }
}

// ---------- Couleurs ----------

pub fn hsv(h: f32, s: f32, v: f32) -> Rgba {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Rgba(r, g, b, 1.0)
}

pub(crate) fn preset(c: &IleCfg) -> &'static Preset {
    let p = &PRESETS[c.preset.min(PRESETS.len() - 1)];
    // « Clair le jour, sombre la nuit » : thème clair le jour, « Nuit » le soir
    if c.g_auto_theme {
        let n = chrono::Local::now();
        let h = n.hour() as f32 + n.minute() as f32 / 60.0;
        let jour = h >= c.g_jour && h < c.g_nuit;
        if jour && !p.light {
            if let Some(q) = PRESETS.iter().find(|q| q.light) {
                return q;
            }
        }
        if !jour && p.light {
            return &PRESETS[0];
        }
    }
    p
}

pub(crate) fn accent(r: &R) -> Rgba {
    let c = r.c();
    if c.disco {
        return hsv(r.t() * 0.15, 0.75, 1.0);
    }
    match c.accent {
        0 => Rgba::from(preset(c).accent),
        1 => Rgba::from(r.cfg.accent),
        n => Rgba::from(ACCENTS[(n - 2).min(ACCENTS.len() - 1)]),
    }
}

pub(crate) fn theme(r: &R) -> Theme {
    let a = accent(r);
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
    let c = r.c();
    // fonds sombres de la grande appli : texte clair même avec un thème clair
    let sombre = !preset(c).light || (crate::island_big::is_big(r) && matches!(c.g_fond, 1 | 3 | 4 | 5 | 6));
    Theme::new(sombre, [f(a.0), f(a.1), f(a.2)], 0.4)
}

// ---------- Géométrie ----------

pub(crate) fn scale(r: &R) -> f32 {
    let (mw, _) = main_size(r.c());
    let mut k = (r.dpi * r.c().size).min(r.sw as f32 * 0.97 / mw.max(780.0));
    if crate::island_big::is_big(r) {
        let (bw, bh) = crate::island_big::big_size(r.c());
        k = k.min(r.sw as f32 * 0.96 / bw).min(r.sh as f32 * 0.92 / bh);
    }
    k.max(0.4)
}

/// Taille logique visée pour le mode actuel.
pub fn target(r: &R) -> (Mode, f32, f32) {
    let c = r.c();
    let mode = r.st.mode(r.ring.is_some(), r.jarvis.active(), c);
    let (w, h) = match mode {
        Mode::Pill => {
            let extra = if c.pill_timer && r.clocks.busy() { 78.0 } else { 0.0 } + if impression_dans_pilule(r) { 122.0 } else { 0.0 };
            (c.pill_width + extra, c.pill_height)
        }
        Mode::Toast => (440.0, 68.0),
        Mode::Jarvis => (470.0, 64.0),
        Mode::Ring => (470.0, 118.0),
        Mode::Main | Mode::View if crate::island_big::is_big(r) => crate::island_big::big_size(c),
        Mode::Main | Mode::View if c.layout == 0 => (CARDS_W, CARDS_H),
        Mode::Main => main_size(c),
        Mode::View => (main_size(c).0.max(800.0), 350.0),
    };
    (mode, w, h)
}

/// Une impression 3D est en cours et doit s'afficher dans la pilule.
pub(crate) fn impression_dans_pilule(r: &R) -> bool {
    let p = &r.d.printer;
    r.cfg.printer.on && r.cfg.printer.pilule && p.online && crate::imprimante::is_active(p.status)
}

/// Marges autour de la pilule (zone qui capte la souris, ombre, lueur) et décalage du haut.
pub(crate) fn margins(c: &IleCfg) -> (f32, f32) {
    let m = c.hover_zone.max(if c.shadow { 18.0 } else { 4.0 }).max(if c.glow { 14.0 } else { 0.0 });
    let top = if c.pet > 0 { c.top_margin.max(18.0) } else { c.top_margin };
    (m, top)
}

/// Faut-il redessiner régulièrement (animations) ? Renvoie l'intervalle en ms.
pub fn fx_interval(r: &R) -> Option<u32> {
    let c = r.c();
    let (mode, _, _) = target(r);
    let open = matches!(mode, Mode::Main | Mode::View);
    if open && crate::island_big::anim_page(r).is_some() {
        return Some(16);
    }
    let anim = c.rainbow
        || c.disco
        || c.pet > 0
        || c.eyes
        || c.snow
        || (c.breathe && mode == Mode::Pill)
        || r.st.confetti.is_some_and(|t| t.elapsed().as_secs_f32() < 4.0)
        || r.st.onde.is_some_and(|(_, _, t)| t.elapsed().as_secs_f32() < 0.5)
        || (open && c.weather_anim && (c.m_weather || c.layout == 0))
        || (open && c.clock_color == 2)
        || mode == Mode::Jarvis
        || (c.jarvis_show && r.jarvis.online() && r.jarvis.state != "repos")
        || mode == Mode::Ring
        || (r.st.slide > 0.0 && r.st.slide < 1.0)
        || r.chat.asking
        || r.st.typing
        || (open && r.clocks.chrono_since.is_some())
        || (open && r.st.view == View::Memory && r.mem.moving);
    let decor = c.deco && preset(c).deco > 0;
    if anim {
        Some(if open && crate::island_big::is_big(r) { 80 } else if open { 33 } else { 40 })
    } else if decor {
        Some(if open { 50 } else { 70 })
    } else if r.clocks.busy() || (open && c.seconds) || r.st.toast.is_some() {
        Some(250)
    } else {
        None
    }
}

// ---------- Textes ----------

/// Titre de module (majuscules ou non, mode pirate).
pub(crate) fn head_txt(c: &IleCfg, s: &str) -> String {
    let s = if c.pirate {
        match s {
            "AUJOURD'HUI" => "CE JOUR, MATELOT",
            "MINUTEURS" => "SABLIERS",
            "MÉTÉO" => "LE CIEL",
            "PRONOTE" => "JOURNAL DE BORD",
            "MAILS" => "BOUTEILLES À LA MER",
            "DISCORD" => "PERROQUETS VOYAGEURS",
            "ASSISTANT" => "LE PERROQUET",
            "DEVOIRS" => "CORVÉES",
            "NOTES" => "BUTIN",
            "ALARME" => "CLOCHE DU NAVIRE",
            "NOTIFICATIONS" => "MESSAGES DE LA VIGIE",
            x => x,
        }
    } else {
        s
    };
    if c.caps_headers {
        s.to_string()
    } else {
        let low = s.to_lowercase();
        let mut ch = low.chars();
        ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default()
    }
}

pub(crate) fn greeting(r: &R) -> String {
    let c = r.c();
    let h = r.now.hour();
    let g = if c.pirate {
        "Ahoy, moussaillon"
    } else if h < 5 {
        "Bonne nuit"
    } else if h < 12 {
        "Bonjour"
    } else if h < 18 {
        "Bon après-midi"
    } else {
        "Bonsoir"
    };
    if c.compliments {
        let i = (r.now.ordinal() as usize * 24 + h as usize) % COMPLIMENTS.len();
        format!("{g} !  {}", COMPLIMENTS[i])
    } else {
        format!("{g} !")
    }
}

pub fn time_txt(c: &IleCfg, now: NaiveDateTime) -> (String, String) {
    let (h, m) = (now.hour(), now.minute());
    let main = if c.h24 { format!("{h:02}:{m:02}") } else { format!("{}:{m:02}", if h % 12 == 0 { 12 } else { h % 12 }) };
    let suf = if c.h24 { String::new() } else if h < 12 { "AM".into() } else { "PM".into() };
    (main, suf)
}

pub fn date_txt(c: &IleCfg, now: NaiveDateTime) -> String {
    let wd = now.weekday().num_days_from_monday() as usize;
    let mo = now.month0() as usize;
    match c.date_fmt {
        1 => now.format("%d/%m/%Y").to_string(),
        2 => format!("{} {} {}", JOURS_C[wd].to_lowercase(), now.day(), MOIS_C[mo]),
        3 => format!("{} {} {}", now.day(), MOIS[mo], now.year()),
        _ => format!("{} {} {}", JOURS[wd], now.day(), MOIS[mo]),
    }
}

pub fn temp_txt(c: &IleCfg, t: f32) -> String {
    let v = if c.fahrenheit { t * 9.0 / 5.0 + 32.0 } else { t };
    format!("{}°", v.round() as i32)
}

pub fn wind_txt(c: &IleCfg, kmh: f32) -> String {
    match c.wind_unit {
        1 => format!("{} m/s", (kmh / 3.6).round()),
        2 => format!("{} mph", (kmh / 1.609).round()),
        _ => format!("{} km/h", kmh.round()),
    }
}

/// Catégorie de temps : 0 soleil, 1 nuit claire, 2 éclaircies, 3 nuageux, 4 brouillard, 5 pluie, 6 neige, 7 orage.
pub(crate) fn wcat(code: i32, day: bool) -> usize {
    match code {
        0 | 1 => if day { 0 } else { 1 },
        2 => if day { 2 } else { 1 },
        3 => 3,
        45 | 48 => 4,
        51..=67 | 80..=82 => 5,
        71..=77 | 85 | 86 => 6,
        95..=99 => 7,
        _ => 3,
    }
}

pub(crate) fn wcolors(cat: usize) -> (Rgba, Rgba) {
    let (a, b) = match cat {
        0 => ([255, 190, 60], [255, 112, 64]),
        1 => ([44, 52, 128], [100, 62, 168]),
        2 => ([84, 168, 255], [255, 186, 92]),
        3 => ([118, 138, 170], [76, 92, 126]),
        4 => ([150, 150, 162], [96, 102, 116]),
        5 => ([52, 132, 232], [40, 66, 172]),
        6 => ([168, 210, 255], [116, 156, 236]),
        _ => ([116, 72, 176], [42, 30, 96]),
    };
    (Rgba::from(a), Rgba::from(b))
}

pub fn weather_phrase(c: &IleCfg, code: i32, day: bool, temp: f32, doy: u32) -> String {
    let list: &[&str] = match wcat(code, day) {
        0 => &["Grand soleil : parfait pour sortir !", "Le soleil est de ton côté aujourd'hui.", "Ciel bleu, moral au top !"],
        1 => &["Ciel dégagé : regarde les étoiles !", "Nuit claire et tranquille."],
        2 => &["Un peu de soleil entre les nuages.", "Ciel mitigé, mais ça reste joli."],
        3 => &["Ciel gris, mais bonne humeur quand même !", "Nuages au programme : chocolat chaud ?"],
        4 => &["Brouillard : on se croirait dans un film.", "Tout est flou dehors, reste attentif."],
        5 => &["Prends ton parapluie !", "Temps idéal pour rester au chaud.", "Les flaques t'attendent, prends des bottes."],
        6 => &["Il neige ! Bataille de boules de neige ?", "Couvre-toi bien, ça glisse."],
        _ => &["Orage : reste à l'abri !"],
    };
    let mut s = list[doy as usize % list.len()].to_string();
    if temp >= 28.0 {
        s = "Il fait chaud : pense à boire de l'eau !".into();
    } else if temp <= 0.0 {
        s = "Ça caille ! Gants et bonnet obligatoires.".into();
    }
    if c.pirate { format!("Arr ! {s}") } else { s }
}

pub fn is_important(m: &Mail, c: &IleCfg) -> bool {
    if c.only_urgent {
        return m.urgent;
    }
    m.urgent || m.cat == Cat::Important || (c.imp_reply && m.cat == Cat::Reply) || (c.imp_gmail && m.important)
}

pub fn important_mails(d: &Data, c: &IleCfg) -> Vec<usize> {
    d.mail.mails.iter().enumerate().filter(|(_, m)| is_important(m, c)).map(|(i, _)| i).collect()
}

/// Devoirs affichés (indices dans la liste Pronote).
pub fn homework_list(d: &Data, c: &IleCfg, now: NaiveDateTime) -> Vec<usize> {
    let today = now.date();
    let last = today + chrono::Duration::days(c.hw_days.round() as i64);
    let mut v: Vec<usize> = d
        .pronote
        .homework
        .iter()
        .enumerate()
        .filter(|(_, h)| h.due >= today && h.due <= last && !(c.hide_done && h.done))
        .map(|(i, _)| i)
        .collect();
    v.sort_by_key(|i| d.pronote.homework[*i].due);
    v
}

pub(crate) fn src_color(s: Src) -> Rgba {
    match s {
        Src::Pronote => Rgba::rgb8(48, 176, 120),
        Src::Mail => Rgba::rgb8(10, 132, 255),
        Src::Discord => Rgba::rgb8(88, 101, 242),
        Src::Timer | Src::Alarm => ORANGE,
        Src::Weather => Rgba::rgb8(90, 200, 250),
        Src::Phone => Rgba::rgb8(52, 199, 89),
        Src::Jarvis => Rgba::rgb8(175, 82, 222),
        Src::Other => Rgba::rgb8(142, 142, 147),
    }
}

pub(crate) fn jarvis_color(state: &str) -> Rgba {
    match state {
        "ecoute" => GREEN,
        "parole" => Rgba::rgb8(48, 209, 160),
        "reflechit" => Rgba::rgb8(175, 82, 222),
        "parle" => Rgba::rgb8(10, 132, 255),
        "parle_ecoute" => ORANGE,
        _ => Rgba(0.6, 0.6, 0.65, 1.0),
    }
}

pub(crate) fn jarvis_label(state: &str) -> &'static str {
    match state {
        "ecoute" => "À toi, je t'écoute",
        "parole" => "Tu parles…",
        "reflechit" => "Jarvis réfléchit…",
        "parle" => "Jarvis parle",
        "parle_ecoute" => "Je finis, je t'écoute",
        _ => "Jarvis en veille",
    }
}

/// Le texte qui tient dans `largeur` en gardant la FIN (les derniers mots dits), avec « … » devant si on a coupé.
pub(crate) fn fit_fin(c: &mut Ctx, wt: Wt, taille: f32, texte: &str, largeur: f32) -> String {
    if c.measure(wt, taille, texte) <= largeur {
        return texte.to_string();
    }
    let mots: Vec<&str> = texte.split_whitespace().collect();
    for debut in 1..mots.len() {
        let t = format!("… {}", mots[debut..].join(" "));
        if c.measure(wt, taille, &t) <= largeur {
            return t;
        }
    }
    c.fit(wt, taille, mots.last().copied().unwrap_or(""), largeur)
}

// ---------- Rendu ----------

/// Dessine l'île. Renvoie l'image, sa position (x, y) à l'écran et les zones cliquables.
pub fn render(r: &R) -> Option<(Pixmap, i32, i32, Vec<Hit>)> {
    let c = r.c();
    let s = scale(r);
    let (mode, tw, th) = target(r);
    let (mut gw, mut gh) = if r.st.gw > 0.0 { (r.st.gw, r.st.gh) } else { (tw, th) };
    if c.breathe && mode == Mode::Pill {
        let k = 1.0 + 0.02 * (r.t() * 2.2).sin();
        gw *= k;
        gh *= 1.0 + 0.03 * (r.t() * 2.2).sin();
    }
    let (m, top) = margins(c);
    let (ww, wh) = ((gw + 2.0 * m) * s, (top + gh + m) * s);
    let mut pm = Pixmap::new(ww.ceil().max(1.0) as u32, wh.ceil().max(1.0) as u32)?;
    let wx = match c.position {
        1 => (12.0 * s - m * s).round() as i32,
        2 => (r.sw as f32 - ww - 12.0 * s + m * s).round() as i32,
        _ => ((r.sw as f32 - ww) / 2.0).round() as i32,
    };
    // sortie vers le haut quand l'île se cache toute seule
    let sl = r.st.slide.clamp(0.0, 1.0);
    let wy = -((sl * sl * (3.0 - 2.0 * sl)) * wh).round() as i32;
    // presque transparent mais pas tout à fait : la zone autour de la pilule capte la souris
    pm.fill(Color::from_rgba8(0, 0, 0, 1));
    let big = crate::island_big::is_big(r) && matches!(mode, Mode::Main | Mode::View);
    let mut th_ = theme(r);
    if big && c.g_contraste {
        th_.fg2 = th_.fg.a(0.9);
        th_.fg3 = th_.fg.a(0.74);
        th_.sep = th_.fg.a(0.24);
    }
    let p = preset(c);
    let rad = if big && c.g_arrondi > 0.0 { c.g_arrondi } else { c.radius }.min(gh / 2.0) * s;
    let (px, py, pw, ph) = (m * s, top * s, gw * s, gh * s);
    let path = gfx::rrect(px, py, pw, ph, rad, true)?;
    // ombre et lueur : quelques contours de plus en plus transparents (rapide, pas de flou)
    if c.shadow {
        for i in 1..=7 {
            let e = i as f32 * 2.0 * s;
            if let Some(sp) = gfx::rrect(px - e, py - e + 3.0 * s, pw + 2.0 * e, ph + 2.0 * e, rad + e, true) {
                gfx::fill(&mut pm, &sp, Rgba(0.0, 0.0, 0.0, 0.045 * (1.0 - i as f32 / 8.0)));
            }
        }
    }
    if c.glow {
        let a = accent(r);
        for i in 1..=6 {
            let e = i as f32 * 1.8 * s;
            if let Some(sp) = gfx::rrect(px - e, py - e, pw + 2.0 * e, ph + 2.0 * e, rad + e, true) {
                gfx::stroke(&mut pm, &sp, a.a(c.glow_strength * 0.16 * (1.0 - i as f32 / 7.0)), 2.0 * s);
            }
        }
    }
    // effacer l'intérieur (l'ombre ne doit pas assombrir le verre)
    {
        let mut pt = Paint::default();
        pt.blend_mode = BlendMode::Clear;
        pm.fill_path(&path, &pt, FillRule::Winding, Transform::identity(), None);
    }
    // verre : fond d'écran flouté derrière
    let flou = if big { match c.g_flou { 1 => true, 2 => false, _ => c.blur } } else { c.blur };
    if flou {
        if let Some(b) = r.backdrop {
            let tr = Transform::from_scale(b.scale, b.scale).post_translate(-(wx as f32), -(wy as f32));
            let mut pt = Paint::default();
            pt.anti_alias = true;
            pt.shader = Pattern::new(b.pm.as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, 1.0, tr);
            pm.fill_path(&path, &pt, FillRule::Winding, Transform::identity(), None);
        }
    }
    let mut op = c.opacity.clamp(0.15, 1.0);
    if big {
        op = (op * (1.0 - c.g_transp.clamp(0.0, 0.8))).max(0.08);
    }
    let (mut c0, mut c1) = (Rgba::from(p.c0).a(op), Rgba::from(p.c1).a(op));
    let mut degrade = c.gradient;
    if big && c.g_fond > 0 {
        let mix = |a: Rgba, b: Rgba, k: f32| Rgba(a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k, a.2 + (b.2 - a.2) * k, op);
        let noir = Rgba(0.0, 0.0, 0.0, 1.0);
        let (a0, a1) = (Rgba::from(p.c0), Rgba::from(p.c1));
        match c.g_fond {
            1 => (c0, c1) = (mix(a0, noir, 0.45), mix(a1, noir, 0.45)),
            2 => (c0, c1) = (mix(a0, WHITE, 0.22), mix(a1, WHITE, 0.22)),
            3 => {
                (c0, c1) = (mix(accent(r), noir, 0.55), mix(a1, noir, 0.25));
                degrade = true;
            }
            4 => (c0, c1) = (Rgba(0.03, 0.03, 0.04, op), Rgba(0.03, 0.03, 0.04, op)),
            5 => {
                (c0, c1) = (Rgba(0.05, 0.08, 0.18, op), Rgba(0.10, 0.07, 0.24, op));
                degrade = true;
            }
            _ => {
                (c0, c1) = (Rgba(0.16, 0.06, 0.26, op), Rgba(0.30, 0.10, 0.36, op));
                degrade = true;
            }
        }
    }
    if degrade {
        gfx::grad_fill(&mut pm, &path, px, px + pw, c0, c1);
    } else {
        gfx::fill(&mut pm, &path, c0);
    }
    // reflet en haut (effet verre)
    if flou || op < 0.95 {
        if let Some(hp) = gfx::rrect(px + rad * 0.6, py + 1.0 * s, pw - rad * 1.2, 1.2 * s, 0.6 * s, false) {
            gfx::fill(&mut pm, &hp, WHITE.a(if p.light { 0.5 } else { 0.14 }));
        }
    }
    if c.rainbow {
        rainbow_border(&mut pm, px, py, pw, ph, rad, r.t(), s);
    } else if big && c.g_bord {
        if let Some(bp) = gfx::rrect(px + 0.75, py + 0.75, pw - 1.5, ph - 1.5, (rad - 0.75).max(0.0), true) {
            gfx::stroke(&mut pm, &bp, accent(r).a(0.75), 1.5 * s);
        }
    } else if c.border {
        if let Some(bp) = gfx::rrect(px + 0.5, py + 0.5, pw - 1.0, ph - 1.0, (rad - 0.5).max(0.0), true) {
            gfx::stroke(&mut pm, &bp, if p.light { Rgba(0.0, 0.0, 0.0, 0.10) } else { WHITE.a(0.13) }, 1.0);
        }
    }
    // contenu : calque à part, fondu et découpé à la forme de la pilule
    let dist = (tw - r.st.gw.max(1.0)).abs() + (th - r.st.gh.max(1.0)).abs();
    let alpha = if r.st.gw <= 0.0 { 1.0 } else { (1.0 - dist / 70.0).clamp(0.0, 1.0) };
    let alpha = if c.fade { alpha * alpha } else if alpha > 0.97 { 1.0 } else { 0.0 };
    let mut hits = Vec::new();
    if alpha > 0.01 {
        let mut layer = Pixmap::new(pm.width(), pm.height())?;
        let ox = px + (gw - tw) / 2.0 * s;
        {
            let mut cx = Ctx::new(&mut layer, r.fonts, &th_, s, ox, py);
            cx.tf = c.text_size.clamp(0.7, 1.4) * if big { c.g_texte.clamp(0.8, 1.4) } else { 1.0 };
            cx.bold = c.bold;
            match mode {
                Mode::Pill => pill(&mut cx, r, tw, th),
                Mode::Toast => toast(&mut cx, r, tw, th),
                Mode::Jarvis => jarvis_bar(&mut cx, r, tw, th),
                Mode::Ring => ringing(&mut cx, r, tw),
                Mode::Main | Mode::View if crate::island_big::is_big(r) => crate::island_big::big_view(&mut cx, r, tw, th),
                Mode::Main | Mode::View if c.layout == 0 => crate::island_cards::cards_view(&mut cx, r, tw, th),
                Mode::Main => main_view(&mut cx, r, tw),
                Mode::View => detail(&mut cx, r, tw, th),
            }
            if c.snow {
                snow(&mut cx, r, tw, th);
            }
            if c.deco && p.deco > 0 {
                deco(&mut cx, r, p.deco, tw, th);
            }
            if alpha > 0.9 {
                hits = cx.hits;
            }
        }
        // découpe à la forme de la pilule : un masque (un « DestinationIn » ne touche pas ce qui est
        // hors du tracé, et la neige ou les décors dépassaient)
        let pp = PixmapPaint { opacity: alpha, ..Default::default() };
        match tiny_skia::Mask::new(pm.width(), pm.height()) {
            Some(mut masque) => {
                masque.fill_path(&path, FillRule::Winding, true, Transform::identity());
                pm.draw_pixmap(0, 0, layer.as_ref(), &pp, Transform::identity(), Some(&masque));
            }
            None => {
                let mut pt = Paint::default();
                pt.blend_mode = BlendMode::DestinationIn;
                pt.set_color(Color::BLACK);
                layer.fill_path(&path, &pt, FillRule::Winding, Transform::identity(), None);
                pm.draw_pixmap(0, 0, layer.as_ref(), &pp, Transform::identity(), None);
            }
        }
    }
    if c.pet > 0 {
        pet(&mut pm, c.pet, px, py, pw, r.t(), s);
    }
    if let Some(t0) = r.st.confetti {
        confetti(&mut pm, t0.elapsed().as_secs_f32(), s);
    }
    // idée 15 : une lueur sur le bouton survolé ; idée 14 : une onde de lumière là où on clique
    if c.effets && matches!(mode, Mode::Main | Mode::View) {
        let col = accent(r);
        let aire = pm.width() as f32 * pm.height() as f32;
        if let Some((mx, my)) = r.st.souris {
            if let Some(h) = hits.iter().rev().find(|h| mx >= h.x && mx < h.x + h.w && my >= h.y && my < h.y + h.h) {
                if h.w * h.h < 0.2 * aire && h.h < 80.0 * s {
                    lueur(&mut pm, h, col, s);
                }
            }
        }
        if let Some((ox, oy, t0)) = r.st.onde {
            let k = t0.elapsed().as_secs_f32() / 0.45;
            if k < 1.0 {
                onde(&mut pm, ox, oy, k, col, s);
            }
        }
    }
    Some((pm, wx, wy, hits))
}

/// Lueur douce autour du bouton survolé.
fn lueur(pm: &mut Pixmap, h: &Hit, col: Rgba, s: f32) {
    let rad = (h.h / 2.0).min(12.0 * s);
    let Some(p) = gfx::rrect(h.x, h.y, h.w, h.h, rad, true) else { return };
    let mut pt = Paint::default();
    pt.anti_alias = true;
    pt.set_color(col.a(0.10).color());
    pm.fill_path(&p, &pt, FillRule::Winding, Transform::identity(), None);
    pt.set_color(col.a(0.38).color());
    pm.stroke_path(&p, &pt, &Stroke { width: 1.2 * s, ..Default::default() }, Transform::identity(), None);
}

/// Onde de lumière qui s'élargit et s'efface (k de 0 à 1).
fn onde(pm: &mut Pixmap, x: f32, y: f32, k: f32, col: Rgba, s: f32) {
    let Some(p) = tiny_skia::PathBuilder::from_circle(x, y, (6.0 + 30.0 * k) * s) else { return };
    let mut pt = Paint::default();
    pt.anti_alias = true;
    pt.set_color(col.a(0.16 * (1.0 - k)).color());
    pm.fill_path(&p, &pt, FillRule::Winding, Transform::identity(), None);
    pt.set_color(col.a(0.55 * (1.0 - k)).color());
    pm.stroke_path(&p, &pt, &Stroke { width: 2.0 * s, ..Default::default() }, Transform::identity(), None);
}

pub(crate) fn rainbow_border(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, rad: f32, t: f32, s: f32) {
    let Some(bp) = gfx::rrect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, (rad - 1.0).max(0.0), true) else { return };
    let stops: Vec<GradientStop> = (0..=6).map(|i| GradientStop::new(i as f32 / 6.0, hsv(i as f32 / 6.0 + t * 0.25, 0.8, 1.0).color())).collect();
    let Some(sh) = LinearGradient::new(Point::from_xy(x, y), Point::from_xy(x + w, y + h), stops, SpreadMode::Reflect, Transform::identity()) else {
        return;
    };
    let mut pt = Paint::default();
    pt.anti_alias = true;
    pt.shader = sh;
    let st = Stroke { width: 2.2 * s, ..Default::default() };
    pm.stroke_path(&bp, &pt, &st, Transform::identity(), None);
}

// ---------- Petits dessins ----------

pub(crate) fn bell(c: &mut Ctx, cx: f32, cy: f32, col: Rgba) {
    c.poly(&[(cx - 4.5, cy + 3.0), (cx - 3.5, cy - 3.0), (cx, cy - 5.5), (cx + 3.5, cy - 3.0), (cx + 4.5, cy + 3.0)], col);
    c.circle(cx, cy + 4.5, 1.6, col);
}

pub(crate) fn gear(c: &mut Ctx, cx: f32, cy: f32, col: Rgba) {
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::PI / 4.0;
        c.line(cx + a.cos() * 4.5, cy + a.sin() * 4.5, cx + a.cos() * 8.0, cy + a.sin() * 8.0, 2.6, col);
    }
    c.ring(cx, cy, 5.0, col, 2.2);
}

pub(crate) fn play_icon(c: &mut Ctx, cx: f32, cy: f32, running: bool, col: Rgba) {
    if running {
        c.rr(cx - 5.0, cy - 6.0, 3.5, 12.0, 1.0, col);
        c.rr(cx + 1.5, cy - 6.0, 3.5, 12.0, 1.0, col);
    } else {
        c.poly(&[(cx - 4.0, cy - 6.5), (cx + 6.5, cy), (cx - 4.0, cy + 6.5)], col);
    }
}

pub(crate) fn round_btn(c: &mut Ctx, cx: f32, cy: f32, plus: bool, act: Action) {
    let t = c.t;
    c.circle(cx, cy, 10.0, t.fg.a(0.12));
    c.line(cx - 4.5, cy, cx + 4.5, cy, 1.8, t.fg);
    if plus {
        c.line(cx, cy - 4.5, cx, cy + 4.5, 1.8, t.fg);
    }
    c.hit(cx - 12.0, cy - 12.0, 24.0, 24.0, act);
}

/// Bouton « pilule » avec texte, renvoie sa largeur.
pub(crate) fn chip(c: &mut Ctx, x: f32, y: f32, label: &str, bg: Rgba, fg: Rgba, act: Option<Action>) -> f32 {
    let w = c.measure(Wt::Semibold, 12.0, label) + 20.0;
    c.rr(x, y, w, 24.0, 12.0, bg);
    c.text(Wt::Semibold, 12.0, x + 10.0, y + 16.5, fg, label);
    if let Some(a) = act {
        c.hit(x, y, w, 24.0, a);
    }
    w
}

pub(crate) fn card(c: &mut Ctx, r: &R, x: f32, y: f32, w: f32, h: f32) {
    if r.c().glass_cards {
        let light = preset(r.c()).light;
        c.rr(x, y, w, h, 18.0, if light { Rgba(0.0, 0.0, 0.0, 0.045) } else { WHITE.a(0.065) });
        c.rr_stroke(x, y, w, h, 18.0, if light { WHITE.a(0.5) } else { WHITE.a(0.06) }, 1.0);
    }
}

pub(crate) fn header(c: &mut Ctx, r: &R, x: f32, y: f32, label: &str) {
    let t = c.t;
    let s = head_txt(r.c(), label);
    c.text_t(Wt::Semibold, 11.0, x, y, t.fg2, &s, 0.9);
}

// ---------- Pilule fermée ----------

pub(crate) fn pill(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let cf = r.c();
    let base = h / 2.0 + 6.0;
    let (tm, suf) = time_txt(cf, r.now);
    let mut x = 18.0;
    let clock_col = clock_color(r, t);
    x += c.text(Wt::Semibold, 17.0, x, base, clock_col, &tm) + 4.0;
    if !suf.is_empty() {
        x += c.text(Wt::Semibold, 10.0, x, base, t.fg2, &suf) + 4.0;
    }
    if cf.pill_date {
        let d = format!("{} {}", JOURS_C[r.now.weekday().num_days_from_monday() as usize].to_lowercase(), r.now.day());
        x += c.text(Wt::Regular, 12.0, x + 2.0, base, t.fg2, &d) + 6.0;
    }
    x += 6.0;
    if cf.jarvis_show && r.jarvis.online() {
        jarvis_wave(c, x + 8.0, h / 2.0, &r.jarvis.state, r.t());
        c.hit(x - 4.0, 0.0, 26.0, h, Action::Ile(A_JARVIS_LISTEN));
        x += 26.0;
    }
    if cf.eyes {
        eyes(c, r, x + 10.0, h / 2.0);
        x += 30.0;
    }
    // droite : météo
    let mut xr = w - 16.0;
    if cf.pill_weather {
        if let Some(wt) = &r.d.weather {
            let tw = c.text_r(Wt::Semibold, 15.0, xr, base, t.fg, &temp_txt(cf, wt.temp));
            c.icon(wt.code, wt.day, xr - tw - 26.0, h / 2.0 - 11.0, 22.0);
            xr -= tw + 32.0;
        }
    }
    // activité en cours : minuteur ou chrono
    if cf.pill_timer && r.clocks.busy() {
        let (txt, frac, col) = if let Some(tm) = r.clocks.timers.iter().filter(|t| t.end.is_some()).min_by(|a, b| a.remaining().total_cmp(&b.remaining())) {
            (fmt_dur(tm.remaining()), (tm.remaining() / tm.total as f64) as f32, ORANGE)
        } else {
            (fmt_dur(r.clocks.chrono().as_secs_f64().floor()), 1.0, GREEN)
        };
        let tw = c.measure(Wt::Semibold, 14.0, &txt);
        c.text(Wt::Semibold, 14.0, xr - tw, base, col, &txt);
        let cx = xr - tw - 14.0;
        c.ring(cx, h / 2.0, 7.0, col.a(0.25), 2.4);
        arc(c, cx, h / 2.0, 7.0, frac, col);
        xr -= tw + 28.0;
    }
    // impression 3D en cours : avancement et temps restant
    if impression_dans_pilule(r) {
        let pr = &r.d.printer;
        let pause = crate::imprimante::en_pause(pr.status);
        let txt = if pause { format!("{} % · pause", pr.progress) } else { format!("{} % · {}", pr.progress, crate::imprimante::duree(pr.left_min)) };
        let col = if pause { ORANGE } else { t.accent };
        let tw = c.measure(Wt::Semibold, 13.0, &txt);
        c.text(Wt::Semibold, 13.0, xr - tw, base, col, &txt);
        crate::island_cards::icon(c, Tab::Printer, xr - tw - 15.0, h / 2.0, col);
        xr -= tw + 32.0;
    }
    // badges au milieu
    let n = important_mails(r.d, cf).len();
    if cf.pill_mail_badge && n > 0 && x + 30.0 < xr {
        let lab = n.to_string();
        let bw = c.measure(Wt::Semibold, 11.5, &lab) + 14.0;
        c.rr(x, h / 2.0 - 8.0, bw, 16.0, 8.0, RED);
        c.text(Wt::Semibold, 11.5, x + 7.0, h / 2.0 + 4.5, WHITE, &lab);
        x += bw + 7.0;
    }
    if let Some(u) = r.discord_unread.filter(|u| *u > 0) {
        if x + 30.0 < xr {
            let lab = u.to_string();
            let bw = c.measure(Wt::Semibold, 11.5, &lab) + 14.0;
            c.rr(x, h / 2.0 - 8.0, bw, 16.0, 8.0, src_color(Src::Discord));
            c.text(Wt::Semibold, 11.5, x + 7.0, h / 2.0 + 4.5, WHITE, &lab);
            x += bw + 7.0;
        }
    }
    if cf.alarm_on && x + 56.0 < xr {
        bell(c, x + 6.0, h / 2.0, ORANGE);
        c.text(Wt::Semibold, 11.5, x + 15.0, h / 2.0 + 4.5, ORANGE, &format!("{:02}:{:02}", cf.alarm_h, cf.alarm_m));
    }
}

/// Arc de progression (fraction 0..1), dessiné en petits segments.
pub(crate) fn arc(c: &mut Ctx, cx: f32, cy: f32, rad: f32, frac: f32, col: Rgba) {
    let n = (frac.clamp(0.0, 1.0) * 24.0).ceil() as i32;
    let start = -std::f32::consts::FRAC_PI_2;
    for i in 0..n {
        let a0 = start + i as f32 / 24.0 * std::f32::consts::TAU;
        let a1 = start + ((i + 1) as f32 / 24.0).min(frac) * std::f32::consts::TAU;
        c.line(cx + a0.cos() * rad, cy + a0.sin() * rad, cx + a1.cos() * rad, cy + a1.sin() * rad, 2.4, col);
    }
}

pub(crate) fn clock_color(r: &R, t: &Theme) -> Rgba {
    match r.c().clock_color {
        1 => t.accent,
        2 => hsv(r.t() * 0.08, 0.55, 1.0),
        3 => r.d.weather.as_ref().map(|w| widgets::temp_color(w.temp).mix(t.fg, 0.25)).unwrap_or(t.fg),
        _ => t.fg,
    }
}

pub(crate) fn eyes(c: &mut Ctx, r: &R, cx: f32, cy: f32) {
    // direction de la souris (coordonnées écran → logiques)
    let s = c.s;
    let ex = r.st.win.0 as f32 + c.ox + cx * s;
    let ey = r.st.win.1 as f32 + c.oy + cy * s;
    let (dx, dy) = (r.mouse.0 as f32 - ex, r.mouse.1 as f32 - ey);
    let d = (dx * dx + dy * dy).sqrt().max(1.0);
    let (ux, uy) = (dx / d * 2.4, dy / d * 2.4);
    for ox in [-6.5, 6.5] {
        c.circle(cx + ox, cy, 5.5, WHITE);
        c.circle(cx + ox + ux, cy + uy, 2.5, Rgba(0.05, 0.05, 0.08, 1.0));
    }
}

// ---------- Notification, Jarvis, alarme ----------

pub(crate) fn toast(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let Some((n, _)) = &r.st.toast else { return };
    let t = c.t;
    // téléphone : les codes de vérification sont masqués dans les bulles
    let (title, body) = if n.src == Src::Phone { (mask_codes(&n.title), mask_codes(&n.body)) } else { (n.title.clone(), n.body.clone()) };
    let col = src_color(n.src);
    c.circle(34.0, h / 2.0, 17.0, col);
    let letter = n.src.label().chars().next().unwrap_or('?').to_string();
    c.text_c(Wt::Semibold, 15.0, 34.0, h / 2.0 + 5.5, WHITE, &letter);
    let when = n.time.format("%H:%M").to_string();
    let ww = c.measure(Wt::Regular, 11.5, &when);
    c.text_r(Wt::Regular, 11.5, w - 20.0, 27.0, t.fg3, &when);
    let title = c.fit(Wt::Semibold, 14.0, &title, w - 80.0 - ww - 10.0);
    c.text(Wt::Semibold, 14.0, 62.0, 28.0, t.fg, &title);
    let body = c.fit(Wt::Regular, 12.5, &body, w - 84.0);
    c.text(Wt::Regular, 12.5, 62.0, 48.0, t.fg2, &body);
    c.hit(0.0, 0.0, w, h, Action::IleN(N_VIEW, View::Notifs as u16));
}

/// Bandeau de Jarvis, sur deux lignes : en haut son état (et ta phrase), en bas ce qu'il te dit, mot par mot.
pub(crate) fn jarvis_bar(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let j = &r.jarvis;
    jarvis_wave(c, 28.0, h / 2.0, &j.state, r.t());
    c.hit(0.0, 0.0, w, h, Action::Ile(A_JARVIS_LISTEN));
    let (x, droite) = (50.0, w - 18.0);
    let (y1, y2) = (h / 2.0 - 4.0, h / 2.0 + 15.0);
    let lab = jarvis_label(&j.state);
    let lw = c.text(Wt::Semibold, 13.5, x, y1, jarvis_color(&j.state).mix(t.fg, 0.35), lab);
    if !j.toi.is_empty() {
        let toi = c.fit(Wt::Regular, 12.0, &format!("Toi : « {} »", j.toi), droite - x - lw - 12.0);
        c.text(Wt::Regular, 12.0, x + lw + 12.0, y1, t.fg3, &toi);
    }
    // ce que Jarvis dit (ou vient de dire) : les phrases déjà dites, puis la phrase en cours mot par mot
    let reponse = if crate::island::parle(&j.state) {
        let en_cours = j.shown();
        format!("{} {}", j.dit, en_cours).trim().to_string()
    } else {
        j.dit.clone()
    };
    let (ligne, col) = if !reponse.is_empty() {
        (format!("Jarvis : {reponse}"), if crate::island::parle(&j.state) { t.fg } else { t.fg2 })
    } else if matches!(j.state.as_str(), "ecoute" | "parole" | "reflechit") || j.text.is_empty() {
        (String::new(), t.fg2)
    } else {
        (j.text.clone(), t.fg2)
    };
    if !ligne.is_empty() {
        let txt = fit_fin(c, Wt::Regular, 12.5, &ligne, droite - x);
        c.text(Wt::Regular, 12.5, x, y2, col, &txt);
    }
}

pub(crate) fn ringing(c: &mut Ctx, r: &R, w: f32) {
    let t = c.t;
    let label = r.ring.map(|x| x.label.as_str()).unwrap_or("Alarme");
    let shake = (r.t() * 30.0).sin() * 2.0;
    bell(c, 34.0 + shake, 44.0, ORANGE);
    let lab = c.fit(Wt::Semibold, 12.0, &label.to_uppercase(), 200.0);
    c.text_t(Wt::Semibold, 12.0, 48.0, 48.0, ORANGE, &lab, 0.8);
    let (tm, _) = time_txt(r.c(), r.now);
    c.text(Wt::Light, 42.0, 24.0, 96.0, t.fg, &tm);
    let (bw, bh, by) = (112.0, 38.0, 40.0);
    let x1 = w - 24.0 - bw;
    let x0 = x1 - 12.0 - bw;
    c.rr(x0, by, bw, bh, bh / 2.0, t.fg.a(0.14));
    let snooze = format!("+{} min", r.c().snooze_min.round());
    c.text_c(Wt::Semibold, 13.5, x0 + bw / 2.0, by + 24.0, t.fg, &snooze);
    c.hit(x0, by, bw, bh, Action::Ile(A_SNOOZE));
    c.rr(x1, by, bw, bh, bh / 2.0, RED);
    c.text_c(Wt::Semibold, 13.5, x1 + bw / 2.0, by + 24.0, WHITE, "Arrêter");
    c.hit(x1, by, bw, bh, Action::Ile(A_STOP));
}

// ---------- Île ouverte ----------

pub(crate) fn top_bar(c: &mut Ctx, r: &R, w: f32, title: Option<&str>) {
    let t = c.t;
    let cf = r.c();
    let mut x = SIDE;
    if let Some(ti) = title {
        c.chevron(x + 4.0, 22.0, 6.0, true, t.fg);
        c.text(Wt::Semibold, 13.0, x + 14.0, 27.0, t.fg2, "Retour");
        c.hit(0.0, 0.0, 90.0, 40.0, Action::Ile(A_BACK));
        let h = head_txt(cf, ti);
        c.text_t(Wt::Semibold, 13.0, x + 90.0, 27.0, t.fg, &h, 0.6);
        x += 90.0 + c.measure(Wt::Semibold, 13.0, &h) + 20.0;
    } else if cf.greeting {
        let g = greeting(r);
        let g = c.fit(Wt::Semibold, 13.0, &g, w * 0.45);
        x += c.text(Wt::Semibold, 13.0, x, 27.0, t.fg2, &g) + 18.0;
    }
    // Jarvis : état (clic = parler), micro, dernier texte
    if cf.jarvis_show {
        if r.jarvis.online() {
            jarvis_wave(c, x + 6.0, 22.0, &r.jarvis.state, r.t());
            let pw = chip(c, x + 20.0, 9.0, "Parler", t.accent.a(0.25), t.fg, Some(Action::Ile(A_JARVIS_LISTEN)));
            let mic = if r.jarvis.micro { "Micro : oui" } else { "Micro : coupé" };
            let mw = chip(c, x + 26.0 + pw, 9.0, mic, if r.jarvis.micro { GREEN.a(0.25) } else { RED.a(0.3) }, t.fg, Some(Action::Ile(A_JARVIS_MIC)));
            let mut lab = jarvis_label(&r.jarvis.state).to_string();
            if !r.jarvis.text.is_empty() {
                lab = format!("{lab}  « {} »", r.jarvis.shown());
            }
            let lx = x + 34.0 + pw + mw;
            let lab = c.fit(Wt::Regular, 12.5, &lab, (w - lx - 110.0).max(40.0));
            c.text(Wt::Regular, 12.5, lx, 27.0, t.fg2, &lab);
        } else {
            c.ring(x + 5.0, 22.5, 4.5, t.fg3, 1.5);
            c.text(Wt::Regular, 12.5, x + 16.0, 27.0, t.fg3, "Jarvis hors ligne");
        }
    }
    // droite : notifications + réglages
    let gx = w - SIDE - 8.0;
    gear(c, gx, 21.0, t.fg2);
    c.hit(gx - 16.0, 4.0, 32.0, 34.0, Action::Ile(A_SETTINGS));
    let bx = gx - 34.0;
    bell(c, bx, 21.0, t.fg2);
    if r.st.unseen > 0 {
        c.circle(bx + 7.0, 15.0, 6.5, RED);
        c.text_c(Wt::Semibold, 9.5, bx + 7.0, 18.5, WHITE, &r.st.unseen.min(99).to_string());
    }
    c.hit(bx - 16.0, 4.0, 32.0, 34.0, Action::IleN(N_VIEW, View::Notifs as u16));
}

pub(crate) fn main_view(c: &mut Ctx, r: &R, w: f32) {
    top_bar(c, r, w, None);
    let cf = r.c();
    let mods = modules(cf);
    let oy = c.oy;
    c.oy += HEAD * c.s;
    let mut x = SIDE;
    for (i, md) in mods.iter().enumerate() {
        let mw = md.width();
        if cf.separators && i > 0 {
            let t = c.t;
            c.line(x - GAP / 2.0, 6.0, x - GAP / 2.0, BODY - 16.0, 1.0, t.sep);
        }
        if *md != Module::Weather || !cf.weather_bg {
            card(c, r, x - 10.0, -2.0, mw + 20.0, BODY - 12.0);
        }
        match md {
            Module::Time => time_col(c, r, x, mw),
            Module::Timers => timers_col(c, r, x, mw),
            Module::Weather => weather_col(c, r, x, mw),
            Module::Pronote => pronote_col(c, r, x, mw),
            Module::Mail => mail_col(c, r, x, mw),
            Module::Discord => discord_col(c, r, x, mw),
            Module::Ai => ai_col(c, r, x, mw),
        }
        x += mw + GAP;
    }
    c.oy = oy;
}

pub(crate) fn time_col(c: &mut Ctx, r: &R, x: f32, w: f32) {
    let t = c.t;
    let cf = r.c();
    header(c, r, x, 18.0, "AUJOURD'HUI");
    let wt = match cf.clock_weight {
        0 => Wt::Light,
        1 => Wt::Regular,
        _ => Wt::Semibold,
    };
    let size = cf.clock_size.clamp(30.0, 74.0);
    let base = 22.0 + size * 0.82;
    let (tm, suf) = time_txt(cf, r.now);
    let col = clock_color(r, t);
    let mut cx = x - 2.0 + c.text(wt, size, x - 2.0, base, col, &tm);
    if cf.seconds {
        cx += 4.0 + c.text(Wt::Regular, size * 0.38, cx + 4.0, base, t.fg2, &format!("{:02}", r.now.second()));
    }
    if !suf.is_empty() {
        c.text(Wt::Semibold, 13.0, cx + 5.0, base, t.fg2, &suf);
    }
    let date = date_txt(cf, r.now);
    let date = c.fit(Wt::Semibold, 15.0, &date, w);
    c.text(Wt::Semibold, 15.0, x, base + 24.0, t.fg, &date);
    let mut y = base + 42.0;
    if cf.week_num {
        c.text(Wt::Regular, 12.0, x, y, t.fg3, &format!("Semaine {}", r.now.iso_week().week()));
        y += 10.0;
    }
    if cf.day_progress {
        let f = (r.now.num_seconds_from_midnight() as f32 / 86400.0).clamp(0.0, 1.0);
        c.rr(x, y, w, 4.0, 2.0, t.track);
        c.grad_rr(x, y, (w * f).max(4.0), 4.0, 2.0, t.accent.a(0.6), t.accent);
    }
    if !cf.show_alarm {
        return;
    }
    // alarme
    let y = BODY - 92.0;
    c.rr(x - 4.0, y, w + 8.0, 76.0, 14.0, t.fg.a(0.07));
    bell(c, x + 8.0, y + 16.0, if cf.alarm_on { ORANGE } else { t.fg2 });
    let lab = head_txt(cf, "ALARME");
    c.text_t(Wt::Semibold, 10.5, x + 18.0, y + 20.0, t.fg2, &lab, 0.8);
    let days = ["tous les jours", "lun.-ven.", "week-end"][cf.alarm_days.min(2)];
    let dw = c.text(Wt::Regular, 10.5, x + 30.0 + c.measure(Wt::Semibold, 10.5, &lab), y + 20.0, t.fg3, days);
    c.hit(x + 18.0, y + 4.0, 80.0 + dw, 22.0, Action::Ile(A_ALARM_DAYS));
    let (sx, sy) = (x + w - 40.0, y + 7.0);
    c.rr(sx, sy, 36.0, 20.0, 10.0, if cf.alarm_on { GREEN } else { t.track });
    c.circle(if cf.alarm_on { sx + 26.0 } else { sx + 10.0 }, sy + 10.0, 8.0, WHITE);
    c.hit(sx - 6.0, sy - 6.0, 48.0, 32.0, Action::Ile(A_ALARM));
    let cy = y + 51.0;
    let col = if cf.alarm_on { t.fg } else { t.fg2 };
    let k = w / 160.0;
    round_btn(c, x + 10.0, cy, false, Action::Ile(A_H_DOWN));
    c.text_c(Wt::Semibold, 22.0, x + 38.0 * k, cy + 8.0, col, &format!("{:02}", cf.alarm_h));
    round_btn(c, x + 66.0 * k, cy, true, Action::Ile(A_H_UP));
    c.text_c(Wt::Semibold, 18.0, x + 81.0 * k, cy + 6.0, t.fg3, ":");
    round_btn(c, x + 96.0 * k, cy, false, Action::Ile(A_M_DOWN));
    c.text_c(Wt::Semibold, 22.0, x + 124.0 * k, cy + 8.0, col, &format!("{:02}", cf.alarm_m));
    round_btn(c, x + w - 10.0, cy, true, Action::Ile(A_M_UP));
}

pub(crate) fn timers_col(c: &mut Ctx, r: &R, x: f32, w: f32) {
    let t = c.t;
    let cf = r.c();
    header(c, r, x, 18.0, "MINUTEURS");
    let mut y = 32.0;
    if cf.show_chrono {
        let run = r.clocks.chrono_since.is_some();
        let d = r.clocks.chrono();
        let secs = d.as_secs();
        let txt = if secs >= 3600 {
            format!("{}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
        } else {
            format!("{:02}:{:02}.{}", secs / 60, secs % 60, d.subsec_millis() / 100)
        };
        c.text(Wt::Regular, 11.5, x, y + 14.0, t.fg2, "Chronomètre");
        c.text(Wt::Semibold, 27.0, x, y + 46.0, if run { GREEN } else { t.fg }, &txt);
        let (bx, by) = (x + w - 46.0, y + 30.0);
        c.circle(bx, by, 15.0, if run { ORANGE } else { GREEN });
        play_icon(c, bx + if run { 0.0 } else { 1.0 }, by, run, WHITE);
        c.hit(bx - 16.0, by - 16.0, 32.0, 32.0, Action::Ile(A_CHRONO));
        let rx = x + w - 10.0;
        c.circle(rx, by, 11.0, t.fg.a(0.12));
        c.rr(rx - 4.0, by - 4.0, 8.0, 8.0, 1.5, t.fg2);
        c.hit(rx - 12.0, by - 12.0, 24.0, 24.0, Action::Ile(A_CHRONO_RESET));
        y += 62.0;
    }
    if !cf.show_timers {
        return;
    }
    c.hline(x, x + w, y, t.sep);
    y += 8.0;
    if r.clocks.timers.is_empty() {
        c.text(Wt::Regular, 12.5, x, y + 18.0, t.fg3, "Aucun minuteur en cours.");
    }
    let rows = ((BODY - 52.0 - y) / 31.0).floor().max(0.0) as usize;
    for (i, tm) in r.clocks.timers.iter().enumerate().rev().take(rows) {
        let run = tm.end.is_some();
        let rem = tm.remaining();
        let name = c.fit(Wt::Regular, 12.0, &tm.name, w - 120.0);
        c.text(Wt::Regular, 12.0, x, y + 15.0, t.fg2, &name);
        c.text_r(Wt::Semibold, 15.0, x + w - 48.0, y + 16.0, if run { ORANGE } else { t.fg3 }, &fmt_dur(rem));
        let f = (rem / tm.total as f64) as f32;
        c.rr(x, y + 22.0, w - 50.0, 3.0, 1.5, t.track);
        c.rr(x, y + 22.0, ((w - 50.0) * f).max(3.0), 3.0, 1.5, ORANGE);
        let (bx, by) = (x + w - 32.0, y + 12.0);
        c.circle(bx, by, 10.0, t.fg.a(0.12));
        play_icon(c, bx, by, run, t.fg);
        c.hit(bx - 11.0, by - 11.0, 22.0, 22.0, Action::IleN(N_TIMER_PAUSE, i as u16));
        let dx = x + w - 8.0;
        c.line(dx - 4.0, by - 4.0, dx + 4.0, by + 4.0, 1.8, t.fg2);
        c.line(dx - 4.0, by + 4.0, dx + 4.0, by - 4.0, 1.8, t.fg2);
        c.hit(dx - 10.0, by - 11.0, 20.0, 22.0, Action::IleN(N_TIMER_DEL, i as u16));
        y += 31.0;
    }
    let presets: [i64; 3] = match cf.timer_presets {
        1 => [5, 15, 25],
        2 => [10, 30, 60],
        _ => [1, 5, 10],
    };
    let bw = (w - 12.0) / 3.0;
    let by = BODY - 46.0;
    for (i, mn) in presets.iter().enumerate() {
        let bx = x + i as f32 * (bw + 6.0);
        c.rr(bx, by, bw, 26.0, 13.0, t.accent.a(0.22));
        c.text_c(Wt::Semibold, 12.0, bx + bw / 2.0, by + 17.5, t.fg, &format!("+{mn} min"));
        c.hit(bx, by, bw, 26.0, Action::IleN(N_TIMER_ADD, (mn * 60) as u16));
    }
}

pub(crate) fn weather_col(c: &mut Ctx, r: &R, x: f32, w: f32) {
    let t = c.t;
    let cf = r.c();
    let wt = r.d.weather.as_ref();
    if let (true, Some(wt)) = (cf.weather_bg, wt) {
        let (a, b) = wcolors(wcat(wt.code, wt.day));
        c.grad_rr(x - 10.0, -2.0, w + 20.0, BODY - 12.0, 18.0, a.a(0.62), b.a(0.62));
        c.rr_stroke(x - 10.0, -2.0, w + 20.0, BODY - 12.0, 18.0, WHITE.a(0.12), 1.0);
    } else if cf.weather_bg {
        card(c, r, x - 10.0, -2.0, w + 20.0, BODY - 12.0);
    }
    let on_color = cf.weather_bg && wt.is_some();
    let (fg, fg2) = if on_color { (WHITE, WHITE.a(0.8)) } else { (t.fg, t.fg2) };
    let hd = head_txt(cf, "MÉTÉO");
    c.text_t(Wt::Semibold, 11.0, x, 18.0, fg2, &hd, 0.9);
    let city = c.fit(Wt::Semibold, 12.0, &r.cfg.weather.city, w - c.measure(Wt::Semibold, 11.0, &hd) - 14.0);
    c.text_r(Wt::Semibold, 12.0, x + w, 18.0, fg, &city);
    c.hit(x - 10.0, -2.0, w + 20.0, BODY - 12.0, Action::Refresh);
    let Some(wt) = wt else {
        c.text(Wt::Regular, 13.0, x, 60.0, t.fg2, "Chargement de la météo…");
        return;
    };
    let cat = wcat(wt.code, wt.day);
    let (ix, iy, isz) = (x - 4.0, 30.0, 60.0);
    if cf.weather_anim {
        weather_fx(c, r, cat, ix + isz / 2.0, iy + isz / 2.0, isz);
    }
    c.icon(wt.code, wt.day, ix, iy, isz);
    let tc = if on_color { WHITE } else { widgets::temp_color(wt.temp).mix(t.fg, 0.15) };
    c.text(Wt::Light, 50.0, x + 66.0, 82.0, tc, &temp_txt(cf, wt.temp));
    let lab = c.fit(Wt::Semibold, 15.0, wmo_label(wt.code, wt.day), w);
    c.text(Wt::Semibold, 15.0, x, 110.0, fg, &lab);
    let mut y = 128.0;
    if cf.weather_phrase {
        let ph = weather_phrase(cf, wt.code, wt.day, wt.temp, r.now.ordinal());
        for l in c.wrap(Wt::Regular, 12.5, &ph, w).iter().take(2) {
            c.text(Wt::Regular, 12.5, x, y, fg, l);
            y += 16.0;
        }
    }
    let mut info: Vec<String> = Vec::new();
    if cf.show_minmax {
        if let Some(d) = wt.days.first() {
            info.push(format!("{} / {}", temp_txt(cf, d.min), temp_txt(cf, d.max)));
        }
    }
    if cf.show_feels {
        info.push(format!("ressenti {}", temp_txt(cf, wt.feels)));
    }
    info.push(wind_txt(cf, wt.wind));
    let line = c.fit(Wt::Regular, 11.5, &info.join("  ·  "), w);
    c.text(Wt::Regular, 11.5, x, y + 2.0, fg2, &line);
    if let Some(rain) = &wt.rain {
        let rl = c.fit(Wt::Semibold, 11.5, rain, w);
        c.text(Wt::Semibold, 11.5, x, y + 18.0, if on_color { WHITE } else { t.rain }, &rl);
    } else if cf.forecast && !wt.hours.is_empty() {
        let cw = w / 4.0;
        let fy = BODY - 68.0;
        for (i, h) in wt.hours.iter().take(4).enumerate() {
            let cx = x + i as f32 * cw + cw / 2.0;
            c.text_c(Wt::Regular, 10.5, cx, fy + 12.0, fg2, &h.label);
            c.icon(h.code, h.day, cx - 9.0, fy + 15.0, 18.0);
            c.text_c(Wt::Semibold, 11.5, cx, fy + 46.0, fg, &temp_txt(cf, h.temp));
        }
    }
}

/// Animations autour de l'icône : rayons qui tournent, pluie, neige, étoiles, éclairs.
pub(crate) fn weather_fx(c: &mut Ctx, r: &R, cat: usize, cx: f32, cy: f32, sz: f32) {
    let t = r.t();
    match cat {
        0 | 2 => {
            let col = Rgba::rgb8(255, 214, 80).a(0.55);
            for i in 0..10 {
                let a = t * 0.6 + i as f32 * std::f32::consts::TAU / 10.0;
                let (r0, r1) = (sz * 0.40, sz * 0.40 + 6.0 + 2.0 * (t * 3.0 + i as f32).sin());
                c.line(cx - 6.0 + a.cos() * r0, cy - 6.0 + a.sin() * r0, cx - 6.0 + a.cos() * r1, cy - 6.0 + a.sin() * r1, 2.0, col);
            }
        }
        1 => {
            for i in 0..6 {
                let k = (t * 2.0 + i as f32 * 1.7).sin() * 0.5 + 0.5;
                let (sx, sy) = (cx - sz * 0.6 + (i * 23 % 70) as f32, cy - sz * 0.5 + (i * 17 % 30) as f32);
                c.circle(sx, sy, 1.0 + k, WHITE.a(0.3 + 0.6 * k));
            }
        }
        5 | 7 => {
            for i in 0..8 {
                let off = (t * 60.0 + i as f32 * 13.0) % 26.0;
                let dx = cx - sz * 0.3 + (i * 9 % 36) as f32;
                let dy = cy + sz * 0.22 + off;
                c.line(dx, dy, dx - 2.0, dy + 6.0, 1.6, Rgba::rgb8(140, 200, 255).a(0.8 * (1.0 - off / 26.0)));
            }
            if cat == 7 && (t * 1.3).fract() < 0.08 {
                c.poly(&[(cx + 2.0, cy), (cx - 6.0, cy + 16.0), (cx, cy + 14.0), (cx - 4.0, cy + 28.0), (cx + 8.0, cy + 10.0), (cx + 2.0, cy + 12.0)], Rgba::rgb8(255, 230, 90));
            }
        }
        6 => {
            for i in 0..9 {
                let off = (t * 14.0 + i as f32 * 9.0) % 30.0;
                let dx = cx - sz * 0.35 + (i * 11 % 42) as f32 + (t * 2.0 + i as f32).sin() * 3.0;
                c.circle(dx, cy + sz * 0.2 + off, 1.8, WHITE.a(0.9 * (1.0 - off / 30.0)));
            }
        }
        _ => {}
    }
}

pub(crate) fn pronote_col(c: &mut Ctx, r: &R, x: f32, w: f32) {
    let t = c.t;
    let cf = r.c();
    header(c, r, x, 18.0, "PRONOTE");
    let p = &r.d.pronote;
    if !p.connected || !r.cfg.pronote.on {
        let lignes = if r.cfg.pronote.on {
            ["Pronote n'est pas connecté.", "Clique ici pour le connecter", "(Paramètres › Comptes)."]
        } else {
            ["Pronote est désactivé.", "Clique ici pour le réactiver", "(Comptes)."]
        };
        for (i, l) in lignes.iter().enumerate() {
            c.text(Wt::Regular, 13.0, x, 50.0 + i as f32 * 18.0, t.fg2, l);
        }
        c.hit(x - 10.0, -2.0, w + 20.0, BODY - 12.0, Action::Accounts);
        return;
    }
    // fond : un clic = détails des devoirs, double-clic = Pronote
    c.hit(x - 10.0, -2.0, w + 20.0, BODY - 12.0, Action::IleN(N_VIEW, View::Homework as u16));
    let mut y = 30.0;
    if cf.show_next_class {
        if let Some(les) = p.next(r.now).0 {
            let cd = widgets::countdown_txt(les, r.now);
            c.text_r(Wt::Semibold, 11.5, x + w, 18.0, t.accent, &cd);
            c.rr(x, y + 2.0, 4.0, 36.0, 2.0, Rgba::from(les.color));
            let sub = c.fit(Wt::Semibold, 15.0, &les.subject, w - 12.0);
            c.text(Wt::Semibold, 15.0, x + 12.0, y + 17.0, if les.bad() { t.fg2 } else { t.fg }, &sub);
            let mut info = format!("{} – {}", les.start.format("%H:%M"), les.end.format("%H:%M"));
            if !les.room.is_empty() {
                info = format!("{info}  ·  {}", widgets::room_txt(&les.room));
            }
            let info = c.fit(Wt::Regular, 12.0, &info, w - 12.0);
            c.text(Wt::Regular, 12.0, x + 12.0, y + 34.0, t.fg2, &info);
        } else {
            c.text(Wt::Regular, 12.5, x, y + 16.0, t.fg2, "Aucun cours à venir.");
        }
        y += 50.0;
    }
    let hw = homework_list(r.d, cf, r.now);
    let hd = head_txt(cf, "DEVOIRS");
    c.text_t(Wt::Semibold, 10.5, x, y + 10.0, t.fg3, &hd, 0.8);
    c.text_r(Wt::Semibold, 11.0, x + w, y + 10.0, if hw.is_empty() { t.fg3 } else { ORANGE }, &hw.len().to_string());
    y += 16.0;
    let rows = if cf.grades_n >= 1.0 && cf.show_grades { 2 } else { 5 };
    for &i in hw.iter().take(rows) {
        let h = &p.homework[i];
        c.circle(x + 4.0, y + 9.0, 3.5, Rgba::from(h.color));
        let due = widgets::day_label(h.due, r.now.date());
        let dw = c.text_r(Wt::Regular, 11.0, x + w, y + 13.0, t.fg3, &due);
        let sub = c.fit(Wt::Regular, 12.5, &format!("{} · {}", h.subject, h.text), w - 16.0 - dw - 6.0);
        c.text(Wt::Regular, 12.5, x + 12.0, y + 13.0, if h.done { t.fg3 } else { t.fg }, &sub);
        c.hit(x - 4.0, y, w + 8.0, 18.0, Action::IleN(N_HW, i as u16));
        y += 18.0;
    }
    if hw.is_empty() {
        c.text(Wt::Regular, 12.5, x, y + 13.0, t.fg2, "Rien à faire, profite !");
        y += 18.0;
    }
    let ng = cf.grades_n.round() as usize;
    if ng == 0 || !cf.show_grades {
        return;
    }
    y += 8.0;
    let hd = head_txt(cf, "NOTES");
    c.text_t(Wt::Semibold, 10.5, x, y + 10.0, t.fg3, &hd, 0.8);
    if cf.show_avg && !p.overall.is_empty() {
        c.text_r(Wt::Semibold, 11.0, x + w, y + 10.0, t.fg2, &format!("moyenne {}", p.overall));
    }
    y += 16.0;
    let room = ((BODY - 18.0 - y) / 18.0).floor().max(0.0) as usize;
    for (i, g) in p.grades.iter().take(ng.min(room)).enumerate() {
        let val = format!("{}/{}", grade_txt(&g.value), g.out_of);
        let vw = c.text_r(Wt::Semibold, 12.5, x + w, y + 13.0, widgets::grade_color(g, t), &val);
        let sub = c.fit(Wt::Regular, 12.5, &g.subject, w - vw - 10.0);
        c.text(Wt::Regular, 12.5, x, y + 13.0, t.fg, &sub);
        c.hit(x - 4.0, y, w + 8.0, 18.0, Action::IleN(N_GRADE, i as u16));
        y += 18.0;
    }
}

pub(crate) fn mail_row(c: &mut Ctx, m: &Mail, x: f32, y: f32, w: f32, snippet: bool) {
    let t = c.t;
    let dot = if m.urgent || m.cat == Cat::Important { RED } else if m.cat == Cat::Reply { ORANGE } else { t.accent };
    c.circle(x + 4.0, y + 11.0, 3.5, dot);
    let when = m.date.format("%H:%M").to_string();
    let ww = c.measure(Wt::Regular, 11.5, &when);
    let from = c.fit(Wt::Semibold, 12.5, &m.from, w - 14.0 - ww - 8.0);
    c.text(Wt::Semibold, 12.5, x + 14.0, y + 15.0, t.fg, &from);
    c.text_r(Wt::Regular, 11.5, x + w, y + 15.0, t.fg3, &when);
    let line = if snippet && !m.snippet.is_empty() { &m.snippet } else if m.subject.is_empty() { &m.snippet } else { &m.subject };
    let subj = c.fit(Wt::Regular, 12.0, line, w - 14.0);
    c.text(Wt::Regular, 12.0, x + 14.0, y + 31.0, t.fg2, &subj);
}

pub(crate) fn mail_col(c: &mut Ctx, r: &R, x: f32, w: f32) {
    let t = c.t;
    let cf = r.c();
    header(c, r, x, 18.0, "MAILS");
    let mb = &r.d.mail;
    if mb.accounts == 0 {
        for (i, l) in ["Aucun compte mail.", "Clique ici pour connecter Gmail", "(Paramètres › Comptes)."].iter().enumerate() {
            c.text(Wt::Regular, 13.0, x, 50.0 + i as f32 * 18.0, t.fg2, l);
        }
        c.hit(x - 10.0, -2.0, w + 20.0, BODY - 12.0, Action::Accounts);
        return;
    }
    c.hit(x - 10.0, -2.0, w + 20.0, BODY - 12.0, Action::IleN(N_VIEW, View::Mails as u16));
    let n = mb.mails.len();
    let list = important_mails(r.d, cf);
    c.text_r(Wt::Regular, 11.5, x + w, 18.0, t.fg3, &format!("{} important{} · {n} non lu{}", list.len(), if list.len() > 1 { "s" } else { "" }, if n > 1 { "s" } else { "" }));
    if list.is_empty() {
        c.text(Wt::Semibold, 14.0, x, 56.0, t.fg, "Rien d'important.");
        c.text(Wt::Regular, 12.5, x, 76.0, t.fg2, "Tranquille !");
        return;
    }
    let mut y = 28.0;
    let max = (cf.mails_n.round() as usize).min(((BODY - 16.0 - y) / 42.0) as usize);
    for &i in list.iter().take(max) {
        mail_row(c, &mb.mails[i], x, y, w, cf.show_snippet);
        c.hit(x - 4.0, y, w + 8.0, 40.0, Action::IleN(N_MAIL, i as u16));
        y += 42.0;
    }
}

pub(crate) fn discord_col(c: &mut Ctx, r: &R, x: f32, w: f32) {
    let t = c.t;
    let cf = r.c();
    header(c, r, x, 18.0, "DISCORD");
    if let Some(u) = r.discord_unread.filter(|_| cf.discord_title) {
        let lab = if u == 0 { "Chrome : 0 non lu".to_string() } else { format!("{u} non lu{}", if u > 1 { "s" } else { "" }) };
        c.text_r(Wt::Semibold, 11.5, x + w, 18.0, if u > 0 { src_color(Src::Discord).mix(WHITE, 0.3) } else { t.fg3 }, &lab);
    }
    c.hit(x - 10.0, -2.0, w + 20.0, BODY - 12.0, Action::IleN(N_VIEW, View::Discord as u16));
    let list: Vec<_> = r.d.discord.iter().rev().take(cf.discord_n.round() as usize).collect();
    if list.is_empty() {
        let txt = "Pas de message reçu. Les MP apparaissent ici quand Discord (ou Chrome) affiche une notification Windows.";
        for (i, l) in c.wrap(Wt::Regular, 12.0, txt, w).iter().take(6).enumerate() {
            c.text(Wt::Regular, 12.0, x, 48.0 + i as f32 * 16.0, t.fg2, l);
        }
        return;
    }
    let mut y = 28.0;
    for (i, n) in list.iter().enumerate() {
        if y + 40.0 > BODY - 14.0 {
            break;
        }
        c.circle(x + 4.0, y + 11.0, 3.5, src_color(Src::Discord));
        let when = n.time.format("%H:%M").to_string();
        let ww = c.measure(Wt::Regular, 11.5, &when);
        let ti = c.fit(Wt::Semibold, 12.5, &n.title, w - 14.0 - ww - 8.0);
        c.text(Wt::Semibold, 12.5, x + 14.0, y + 15.0, t.fg, &ti);
        c.text_r(Wt::Regular, 11.5, x + w, y + 15.0, t.fg3, &when);
        let b = c.fit(Wt::Regular, 12.0, &n.body, w - 14.0);
        c.text(Wt::Regular, 12.0, x + 14.0, y + 31.0, t.fg2, &b);
        c.hit(x - 4.0, y, w + 8.0, 40.0, Action::IleN(N_DISCORD, i as u16));
        y += 42.0;
    }
}

pub(crate) fn ai_status(r: &R) -> (Rgba, &'static str) {
    let ai = &r.d.accounts.ai;
    if ai.starts_with("En service") {
        (GREEN, "Prête")
    } else if ai.contains("ne répond pas") {
        (RED, "IA éteinte")
    } else {
        (Rgba(0.6, 0.6, 0.6, 1.0), "Vérification…")
    }
}

pub(crate) fn input_field(c: &mut Ctx, r: &R, x: f32, y: f32, w: f32) {
    let t = c.t;
    let st = r.st;
    let fh = 32.0;
    c.rr(x, y, w, fh, 16.0, t.fg.a(if st.typing { 0.16 } else { 0.08 }));
    if st.typing {
        c.rr_stroke(x, y, w, fh, 16.0, t.accent.a(0.85), 1.3);
    }
    let fw = w - 48.0;
    let mut v: Vec<char> = st.input.chars().collect();
    while !v.is_empty() && c.measure(Wt::Regular, 12.5, &v.iter().collect::<String>()) > fw {
        v.remove(0);
    }
    let shown: String = v.into_iter().collect();
    let tw = if st.input.is_empty() && !st.typing {
        let ph = if r.st.view == View::Mails && r.st.sel.is_some() {
            "Demande à Jarvis sur ce mail…"
        } else if r.c().pirate {
            "Parle au perroquet…"
        } else {
            "Pose une question…"
        };
        c.text(Wt::Regular, 12.5, x + 12.0, y + 20.5, t.fg3, ph);
        0.0
    } else {
        c.text(Wt::Regular, 12.5, x + 12.0, y + 20.5, t.fg, &shown)
    };
    if st.typing && (r.t() * 2.0).fract() < 0.6 {
        c.line(x + 13.0 + tw, y + 9.0, x + 13.0 + tw, y + fh - 9.0, 1.3, t.fg);
    }
    c.hit(x, y, w - 30.0, fh, Action::Ile(A_ASK));
    let (bx, by) = (x + w - 17.0, y + fh / 2.0);
    let on = !st.input.trim().is_empty() && !r.chat.asking;
    c.circle(bx, by, 12.0, if on { t.accent } else { t.track });
    c.line(bx, by + 5.5, bx, by - 5.5, 1.9, WHITE);
    c.line(bx - 4.5, by - 1.0, bx, by - 5.5, 1.9, WHITE);
    c.line(bx + 4.5, by - 1.0, bx, by - 5.5, 1.9, WHITE);
    c.hit(bx - 15.0, y, 30.0, fh, Action::Ile(A_SEND));
}

pub(crate) fn thinking(c: &mut Ctx, r: &R, x: f32, y: f32) {
    let t = c.t;
    for i in 0..3 {
        let k = ((r.t() * 4.0 - i as f32 * 0.6).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        c.circle(x + 6.0 + i as f32 * 12.0, y - 4.0 - 3.0 * k, 3.5, t.fg.a(0.35 + 0.6 * k));
    }
}

pub(crate) fn ai_col(c: &mut Ctx, r: &R, x: f32, w: f32) {
    let t = c.t;
    let cf = r.c();
    header(c, r, x, 18.0, "ASSISTANT");
    let (dot, state) = ai_status(r);
    let line = if r.jarvis.online() { "Jarvis répond".to_string() } else { format!("{state} · {}", r.model) };
    let dot = if r.jarvis.online() { Rgba::rgb8(175, 82, 222) } else { dot };
    let lw = c.measure(Wt::Regular, 11.5, &line).min(w - 70.0);
    let line = c.fit(Wt::Regular, 11.5, &line, w - 70.0);
    c.circle(x + w - lw - 10.0, 14.0, 3.5, dot);
    c.text_r(Wt::Regular, 11.5, x + w, 18.0, t.fg2, &line);
    c.hit(x + w - lw - 20.0, 2.0, lw + 20.0, 22.0, Action::Ile(A_IA));
    input_field(c, r, x - 2.0, 28.0, w + 4.0);
    let mut y = 80.0;
    if let Some((q, a)) = r.chat.log.last() {
        if !q.is_empty() {
            let q = c.fit(Wt::Semibold, 12.0, &format!("« {q} »"), w);
            c.text(Wt::Semibold, 12.0, x, y, t.fg2, &q);
            y += 18.0;
        }
        if a.is_empty() && r.chat.asking {
            thinking(c, r, x, y + 4.0);
        } else {
            let lines = c.wrap(Wt::Regular, 12.5, a, w);
            let max = ((BODY - 46.0 - y) / 16.0).floor().max(1.0) as usize;
            for (i, ln) in lines.iter().take(max).enumerate() {
                let txt = if i + 1 == max && lines.len() > max { c.fit(Wt::Regular, 12.5, &format!("{ln} …"), w) } else { ln.clone() };
                c.text(Wt::Regular, 12.5, x, y + i as f32 * 16.0, t.fg, &txt);
            }
        }
    } else if !r.chat.error.is_empty() {
        for (i, l) in c.wrap(Wt::Regular, 12.0, &r.chat.error, w).iter().take(4).enumerate() {
            c.text(Wt::Regular, 12.0, x, y + i as f32 * 16.0, RED, l);
        }
    } else {
        let tip = "Demande-moi tes devoirs, la météo, un minuteur de 10 minutes, une alarme à 7 h…";
        for (i, l) in c.wrap(Wt::Regular, 12.0, tip, w).iter().take(4).enumerate() {
            c.text(Wt::Regular, 12.0, x, y + i as f32 * 16.0, t.fg3, l);
        }
    }
    let by = BODY - 42.0;
    let mem = format!("{} échange{}", cf.ai_history.round(), if cf.ai_history >= 2.0 { "s" } else { "" });
    let cw = chip(c, x - 2.0, by, "Agrandir", t.fg.a(0.1), t.fg, Some(Action::IleN(N_VIEW, View::Chat as u16)));
    chip(c, x + cw + 4.0, by, "Nouvelle", t.fg.a(0.1), t.fg, Some(Action::Ile(A_NEWCHAT)));
    let mem = c.fit(Wt::Regular, 10.5, &mem, w - cw - 90.0);
    c.text_r(Wt::Regular, 10.5, x + w, by + 16.0, t.fg3, &mem);
}

// ---------- Pages de détail ----------

pub(crate) fn detail(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let title = match r.st.view {
        View::Homework => "DEVOIRS",
        View::Grades => "NOTES",
        View::Mails => "MAILS",
        View::Discord => "DISCORD",
        View::Notifs => "NOTIFICATIONS",
        View::Chat => "ASSISTANT",
        View::Music => "MUSIQUE",
        View::Timers => "MINUTEURS",
        View::Dnd => "NE PAS DÉRANGER",
        View::School => "PRONOTE",
        View::Calendar => "AGENDA",
        View::Clips => "PRESSE-PAPIERS",
        View::Phone => "TÉLÉPHONE",
        _ => "",
    };
    top_bar(c, r, w, Some(title));
    let oy = c.oy;
    c.oy += HEAD * c.s;
    let bh = h - HEAD - 12.0;
    match r.st.view {
        View::Homework => homework_page(c, r, w, bh),
        View::Grades => grades_page(c, r, w, bh),
        View::Mails => mails_page(c, r, w, bh),
        View::Discord => discord_page(c, r, w, bh),
        View::Notifs => notifs_page(c, r, w, bh),
        View::Chat => chat_page(c, r, w, bh),
        _ => {}
    }
    c.oy = oy;
}

pub(crate) fn homework_page(c: &mut Ctx, r: &R, w: f32, bh: f32) {
    let t = c.t;
    let list = homework_list(r.d, r.c(), r.now);
    if list.is_empty() {
        c.text(Wt::Regular, 14.0, SIDE, 30.0, t.fg2, "Aucun devoir dans les jours choisis. Profite !");
        return;
    }
    c.text_r(Wt::Regular, 11.5, w - SIDE, bh - 4.0, t.fg3, "clic sur le rond : cocher · double-clic : ouvrir Pronote");
    let mut y = 0.0;
    let start = r.st.scroll.min(list.len().saturating_sub(1));
    for &i in &list[start..] {
        let hwk = &r.d.pronote.homework[i];
        let sel = r.st.sel == Some(i);
        let lines = c.wrap(Wt::Regular, 12.5, &hwk.text, w - 2.0 * SIDE - 60.0);
        let n = if sel { lines.len().min(6) } else { lines.len().min(2) };
        let rh = 30.0 + n as f32 * 16.0;
        if y + rh > bh {
            break;
        }
        c.rr(SIDE - 8.0, y, w - 2.0 * SIDE + 16.0, rh - 6.0, 14.0, if sel { t.accent.a(0.16) } else { t.fg.a(0.05) });
        let (bx, by) = (SIDE + 10.0, y + 18.0);
        if hwk.done {
            c.circle(bx, by, 9.0, GREEN);
            c.line(bx - 4.0, by, bx - 1.0, by + 3.5, 2.0, WHITE);
            c.line(bx - 1.0, by + 3.5, bx + 4.5, by - 3.5, 2.0, WHITE);
        } else {
            c.ring(bx, by, 8.0, t.fg2, 1.6);
        }
        c.hit(bx - 14.0, y, 30.0, rh - 6.0, Action::IleN(N_HW_DONE, i as u16));
        let due = widgets::day_label(hwk.due, r.now.date());
        c.text_r(Wt::Semibold, 12.0, w - SIDE, y + 22.0, ORANGE, &due);
        c.rr(SIDE + 26.0, y + 10.0, 3.0, 16.0, 1.5, Rgba::from(hwk.color));
        c.text(Wt::Semibold, 14.0, SIDE + 36.0, y + 23.0, if hwk.done { t.fg3 } else { t.fg }, &hwk.subject);
        for (k, l) in lines.iter().take(n).enumerate() {
            c.text(Wt::Regular, 12.5, SIDE + 36.0, y + 41.0 + k as f32 * 16.0, t.fg2, l);
        }
        c.hit(SIDE + 20.0, y, w - 2.0 * SIDE - 20.0, rh - 6.0, Action::IleN(N_HW, i as u16));
        y += rh;
    }
}

pub(crate) fn grades_page(c: &mut Ctx, r: &R, w: f32, bh: f32) {
    let t = c.t;
    let p = &r.d.pronote;
    if !r.c().show_grades {
        c.text(Wt::Regular, 14.0, SIDE, 30.0, t.fg2, "Les notes sont masquées (option « Afficher les notes Pronote sur l'île »).");
        return;
    }
    let lw = w * 0.56;
    c.text_t(Wt::Semibold, 10.5, SIDE, 4.0, t.fg3, "DERNIÈRES NOTES", 0.8);
    let mut y = 14.0;
    for (i, g) in p.grades.iter().enumerate().skip(r.st.scroll) {
        if y + 34.0 > bh {
            break;
        }
        let sel = r.st.sel == Some(i);
        c.rr(SIDE - 8.0, y, lw - SIDE, 32.0, 10.0, if sel { t.accent.a(0.16) } else { t.fg.a(0.04) });
        let val = format!("{}/{}", grade_txt(&g.value), g.out_of);
        let vw = c.text_r(Wt::Semibold, 15.0, lw - 14.0, y + 21.0, widgets::grade_color(g, t), &val);
        let sub = c.fit(Wt::Semibold, 13.0, &g.subject, lw - SIDE - vw - 130.0);
        c.text(Wt::Semibold, 13.0, SIDE, y + 20.5, t.fg, &sub);
        let mut info = g.date.format("%d/%m").to_string();
        if !g.class_avg.is_empty() {
            info += &format!(" · classe {}", g.class_avg);
        }
        if !g.coef.is_empty() && g.coef != "1" {
            info += &format!(" · coef {}", g.coef);
        }
        c.text_r(Wt::Regular, 11.5, lw - 22.0 - vw, y + 20.5, t.fg3, &info);
        c.hit(SIDE - 8.0, y, lw - SIDE, 32.0, Action::IleN(N_GRADE, i as u16));
        y += 36.0;
    }
    if p.grades.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 34.0, t.fg2, "Pas encore de note.");
    }
    let x = lw + 20.0;
    c.text_t(Wt::Semibold, 10.5, x, 4.0, t.fg3, "MOYENNES", 0.8);
    if !p.overall.is_empty() {
        c.text(Wt::Light, 34.0, x, 46.0, t.fg, &p.overall);
        if !p.class_overall.is_empty() {
            c.text(Wt::Regular, 12.0, x, 64.0, t.fg2, &format!("classe : {}", p.class_overall));
        }
    }
    let mut y = 84.0;
    for a in &p.averages {
        if y + 18.0 > bh {
            break;
        }
        let v = c.text_r(Wt::Semibold, 12.5, w - SIDE, y, t.fg, &a.student);
        let s = c.fit(Wt::Regular, 12.5, &a.subject, w - SIDE - x - v - 8.0);
        c.text(Wt::Regular, 12.5, x, y, t.fg2, &s);
        y += 18.0;
    }
}

pub(crate) fn mails_page(c: &mut Ctx, r: &R, w: f32, bh: f32) {
    let t = c.t;
    let cf = r.c();
    let mb = &r.d.mail;
    let imp = important_mails(r.d, cf);
    let mut order = imp.clone();
    order.extend((0..mb.mails.len()).filter(|i| !imp.contains(i)));
    let lw = w * 0.46;
    let mut y = 0.0;
    for &i in order.iter().skip(r.st.scroll) {
        if y + 44.0 > bh {
            break;
        }
        if r.st.sel == Some(i) {
            c.rr(SIDE - 8.0, y - 2.0, lw - SIDE + 8.0, 42.0, 10.0, t.accent.a(0.16));
        }
        mail_row(c, &mb.mails[i], SIDE, y, lw - SIDE - 8.0, false);
        c.hit(SIDE - 8.0, y - 2.0, lw - SIDE + 8.0, 42.0, Action::IleN(N_MAIL, i as u16));
        y += 44.0;
    }
    if order.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 20.0, t.fg2, "Aucun mail non lu.");
    }
    let x = lw + 16.0;
    let pw = w - SIDE - x;
    c.rr(x - 8.0, -2.0, pw + 16.0, bh, 16.0, t.fg.a(0.05));
    match r.st.sel.and_then(|i| mb.mails.get(i).map(|m| (i, m))) {
        Some((i, m)) => {
            let from = c.fit(Wt::Semibold, 15.0, &m.from, pw);
            c.text(Wt::Semibold, 15.0, x + 6.0, 24.0, t.fg, &from);
            c.text(Wt::Regular, 11.5, x + 6.0, 42.0, t.fg3, &format!("{} · {}", m.date.format("%d/%m à %H:%M"), if m.cat.label().is_empty() { "mail" } else { m.cat.label() }));
            let mut y = 64.0;
            let subj = c.fit(Wt::Semibold, 13.0, &m.subject, pw - 12.0);
            c.text(Wt::Semibold, 13.0, x + 6.0, y, t.fg, &subj);
            y += 8.0;
            // zone « Demander à Jarvis » : réponse (ou extrait du mail), boutons rapides, champ libre
            let ai = r.mail_ai;
            let bottom = bh - 76.0;
            let lines_max = ((bottom - y) / 16.0).floor().max(0.0) as usize;
            if ai.n == Some(i) && (ai.asking || !ai.answer.is_empty()) {
                let q = c.fit(Wt::Semibold, 11.5, &format!("Jarvis · « {} »", ai.question), pw - 12.0);
                c.text(Wt::Semibold, 11.5, x + 6.0, y + 12.0, Rgba::rgb8(190, 150, 255), &q);
                y += 14.0;
                if ai.asking {
                    thinking(c, r, x + 6.0, y + 18.0);
                } else {
                    let lines = c.wrap(Wt::Regular, 12.5, &ai.answer, pw - 12.0);
                    let max = lines_max.saturating_sub(1).max(1);
                    for (k, l) in lines.iter().take(max).enumerate() {
                        let txt = if k + 1 == max && lines.len() > max { c.fit(Wt::Regular, 12.5, &format!("{l} …"), pw - 12.0) } else { l.clone() };
                        c.text(Wt::Regular, 12.5, x + 6.0, y + 14.0 + k as f32 * 16.0, t.fg, &txt);
                    }
                }
            } else {
                for l in c.wrap(Wt::Regular, 12.5, &m.snippet, pw - 12.0).iter().take(lines_max) {
                    c.text(Wt::Regular, 12.5, x + 6.0, y + 12.0, t.fg2, l);
                    y += 16.0;
                }
            }
            let cy = bh - 70.0;
            let mut cx = x + 6.0;
            cx += chip(c, cx, cy, "Résumer", Rgba::rgb8(140, 100, 255).a(0.35), t.fg, Some(Action::IleN(N_MAIL_ASK, 0))) + 6.0;
            cx += chip(c, cx, cy, "Que répondre ?", Rgba::rgb8(140, 100, 255).a(0.35), t.fg, Some(Action::IleN(N_MAIL_ASK, 1))) + 6.0;
            chip(c, cx, cy, "Ouvrir", t.accent, WHITE, Some(Action::OpenMail(i as u16)));
            input_field(c, r, x, bh - 38.0, pw);
        }
        None => {
            c.text_t(Wt::Semibold, 10.5, x + 6.0, 20.0, t.fg3, "RÉSUMÉ DE L'IA", 0.8);
            let s = if mb.summary.is_empty() { "Clique sur un mail pour le lire ici. Double-clic : l'ouvrir dans le navigateur." } else { &mb.summary };
            for (k, l) in c.wrap(Wt::Regular, 13.0, s, pw - 12.0).iter().take(10).enumerate() {
                c.text(Wt::Regular, 13.0, x + 6.0, 44.0 + k as f32 * 18.0, t.fg, l);
            }
            chip(c, x + 6.0, bh - 36.0, "Boîte de réception", t.fg.a(0.12), t.fg, Some(Action::OpenInbox));
        }
    }
}

pub(crate) fn discord_page(c: &mut Ctx, r: &R, w: f32, bh: f32) {
    let t = c.t;
    let list: Vec<_> = r.d.discord.iter().rev().collect();
    let mut info = "Lecture seule : messages lus dans les notifications Windows (Discord ou Chrome).".to_string();
    if let Some(u) = r.discord_unread {
        info += &format!(" Onglet Discord de Chrome : {u} non lu{}.", if u > 1 { "s" } else { "" });
    }
    let info = c.fit(Wt::Regular, 11.5, &info, w - 2.0 * SIDE - 150.0);
    c.text(Wt::Regular, 11.5, SIDE, 6.0, t.fg3, &info);
    chip(c, w - SIDE - 130.0, -10.0, "Ouvrir Discord", src_color(Src::Discord), WHITE, Some(Action::Ile(A_DISCORD_OPEN)));
    let mut y = 22.0;
    if list.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 44.0, t.fg2, "Aucun message pour l'instant.");
    }
    for (i, n) in list.iter().enumerate().skip(r.st.scroll) {
        let lines = c.wrap(Wt::Regular, 12.5, &n.body, w - 2.0 * SIDE - 30.0);
        let k = lines.len().clamp(1, 3);
        let rh = 26.0 + k as f32 * 16.0;
        if y + rh > bh {
            break;
        }
        c.rr(SIDE - 8.0, y, w - 2.0 * SIDE + 16.0, rh - 6.0, 12.0, t.fg.a(0.05));
        c.circle(SIDE + 6.0, y + 16.0, 6.0, src_color(Src::Discord));
        c.text(Wt::Semibold, 13.5, SIDE + 22.0, y + 20.0, t.fg, &n.title);
        c.text_r(Wt::Regular, 11.5, w - SIDE, y + 20.0, t.fg3, &n.time.format("%d/%m %H:%M").to_string());
        for (j, l) in lines.iter().take(k).enumerate() {
            c.text(Wt::Regular, 12.5, SIDE + 22.0, y + 37.0 + j as f32 * 16.0, t.fg2, l);
        }
        c.hit(SIDE - 8.0, y, w - 2.0 * SIDE + 16.0, rh - 6.0, Action::IleN(N_DISCORD, i as u16));
        y += rh;
    }
}

pub(crate) fn notifs_page(c: &mut Ctx, r: &R, w: f32, bh: f32) {
    let t = c.t;
    chip(c, w - SIDE - 100.0, -10.0, "Tout effacer", t.fg.a(0.12), t.fg, Some(Action::Ile(A_CLEAR_NOTIFS)));
    if r.notifs.is_empty() {
        c.text(Wt::Regular, 13.0, SIDE, 30.0, t.fg2, "Aucune notification.");
        return;
    }
    let mut y = 22.0;
    for n in r.notifs.iter().rev().skip(r.st.scroll) {
        if y + 44.0 > bh {
            break;
        }
        let col = src_color(n.src);
        c.circle(SIDE + 8.0, y + 16.0, 9.0, col);
        let letter = n.src.label().chars().next().unwrap_or('?').to_string();
        c.text_c(Wt::Semibold, 10.5, SIDE + 8.0, y + 20.0, WHITE, &letter);
        let ti = c.fit(Wt::Semibold, 13.5, &format!("{} — {}", n.src.label(), n.title), w - 2.0 * SIDE - 120.0);
        c.text(Wt::Semibold, 13.5, SIDE + 26.0, y + 15.0, t.fg, &ti);
        c.text_r(Wt::Regular, 11.5, w - SIDE, y + 15.0, t.fg3, &n.time.format("%d/%m %H:%M").to_string());
        let b = c.fit(Wt::Regular, 12.5, &n.body, w - 2.0 * SIDE - 30.0);
        c.text(Wt::Regular, 12.5, SIDE + 26.0, y + 32.0, t.fg2, &b);
        y += 44.0;
    }
}

pub(crate) fn chat_page(c: &mut Ctx, r: &R, w: f32, bh: f32) {
    let t = c.t;
    let cf = r.c();
    let (dot, state) = ai_status(r);
    let info = if r.jarvis.online() {
        "Jarvis répond (même conversation qu'à la voix)".to_string()
    } else {
        format!("{state} · {} · mémoire : {} derniers échanges · {}", r.model, cf.ai_history.round(), crate::brain::STYLES[cf.ai_style.min(4)])
    };
    let dot = if r.jarvis.online() { Rgba::rgb8(175, 82, 222) } else { dot };
    c.circle(SIDE + 4.0, 0.0, 4.0, dot);
    let info = c.fit(Wt::Regular, 11.5, &info, w - 2.0 * SIDE - 190.0);
    c.text(Wt::Regular, 11.5, SIDE + 14.0, 4.0, t.fg3, &info);
    chip(c, w - SIDE - 170.0, -10.0, "Nouvelle conversation", t.fg.a(0.12), t.fg, Some(Action::Ile(A_NEWCHAT)));
    // bulles, de bas en haut
    let input_y = bh - 38.0;
    input_field(c, r, SIDE - 4.0, input_y, w - 2.0 * SIDE + 8.0);
    let maxw = w * 0.62;
    let mut y = input_y - 10.0;
    let mut items: Vec<(bool, String)> = Vec::new();
    for (q, a) in &r.chat.log {
        if !q.is_empty() {
            items.push((true, q.clone()));
        }
        items.push((false, a.clone()));
    }
    if !r.chat.error.is_empty() && !r.chat.asking {
        items.push((false, r.chat.error.clone()));
    }
    for (user, txt) in items.iter().rev() {
        if !user && txt.is_empty() {
            if r.chat.asking {
                c.rr(SIDE, y - 30.0, 60.0, 28.0, 14.0, t.fg.a(0.1));
                thinking(c, r, SIDE + 8.0, y - 10.0);
                y -= 36.0;
            }
            continue;
        }
        let lines = c.wrap(Wt::Regular, 13.0, txt, maxw - 24.0);
        let bhh = 14.0 + lines.len() as f32 * 17.0;
        if y - bhh < 16.0 {
            break;
        }
        let bw = lines.iter().map(|l| c.measure(Wt::Regular, 13.0, l)).fold(0.0, f32::max) + 24.0;
        let bx = if *user { w - SIDE - bw } else { SIDE };
        c.rr(bx, y - bhh, bw, bhh, 14.0, if *user { t.accent.a(0.85) } else { t.fg.a(0.1) });
        for (k, l) in lines.iter().enumerate() {
            c.text(Wt::Regular, 13.0, bx + 12.0, y - bhh + 20.0 + k as f32 * 17.0, if *user { WHITE } else { t.fg }, l);
        }
        y -= bhh + 8.0;
    }
}

// ---------- Effets rigolos ----------

pub(crate) fn snow(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = r.t();
    for i in 0..26 {
        let sp = 8.0 + (i % 5) as f32 * 4.0;
        let x = (i as f32 * 73.3 + (t * 0.7 + i as f32).sin() * 6.0).rem_euclid(w);
        let y = (t * sp + i as f32 * 37.0).rem_euclid(h + 10.0) - 5.0;
        c.circle(x, y, 1.0 + (i % 3) as f32 * 0.6, WHITE.a(0.55));
    }
}

/// Petit nombre pseudo-aléatoire stable (0..1) pour placer les décors.
fn h01(i: u32, k: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// Décors animés des thèmes de fête (dessinés par-dessus le contenu, très discrets).
pub(crate) fn deco(c: &mut Ctx, r: &R, kind: u8, w: f32, h: f32) {
    let t = r.t();
    let acc = accent(r);
    match kind {
        1 => {
            // Noël : neige + guirlande lumineuse le long du haut
            snow(c, r, w, h);
            let cols = [Rgba::rgb8(255, 70, 70), Rgba::rgb8(90, 220, 110), Rgba::rgb8(255, 210, 90), Rgba::rgb8(110, 170, 255)];
            let n = ((w - 30.0) / 22.0).max(1.0) as usize;
            for i in 0..n {
                let x = 15.0 + i as f32 * 22.0 + 11.0;
                let y = 4.0 + ((x / w) * std::f32::consts::PI * 6.0).sin().abs() * 3.0;
                let on = 0.35 + 0.65 * ((t * 2.2 + i as f32 * 1.3).sin() * 0.5 + 0.5);
                let col = cols[i % 4];
                c.circle(x, y, 4.5, col.a(0.18 * on));
                c.circle(x, y, 2.0, col.a(on));
            }
        }
        2 => {
            // Halloween : chauves-souris qui traversent en battant des ailes
            for i in 0..4u32 {
                let v = 14.0 + h01(i, 1) * 18.0;
                let x = (t * v + h01(i, 2) * w * 1.6).rem_euclid(w + 40.0) - 20.0;
                let y = h * (0.2 + 0.6 * h01(i, 3)) + (t * 1.7 + i as f32).sin() * 4.0;
                let bat = (t * 9.0 + i as f32 * 2.0).sin() * 3.2;
                let col = Rgba(0.05, 0.03, 0.06, 0.75);
                c.circle(x, y, 2.4, col);
                c.line(x - 1.5, y, x - 7.0, y - bat, 1.6, col);
                c.line(x - 7.0, y - bat, x - 10.0, y - bat * 0.2, 1.4, col);
                c.line(x + 1.5, y, x + 7.0, y - bat, 1.6, col);
                c.line(x + 7.0, y - bat, x + 10.0, y - bat * 0.2, 1.4, col);
            }
            // quelques braises orange qui montent
            for i in 0..8u32 {
                let x = h01(i, 4) * w + (t + i as f32).sin() * 3.0;
                let y = h - (t * (6.0 + h01(i, 5) * 8.0) + h01(i, 6) * h).rem_euclid(h);
                c.circle(x, y, 1.1, acc.a(0.45 * (y / h)));
            }
        }
        3 => {
            // Glace : éclats de givre qui scintillent
            for i in 0..16u32 {
                let (x, y) = (h01(i, 7) * w, h01(i, 8) * h);
                let k = ((t * 1.6 + h01(i, 9) * 6.3).sin() * 0.5 + 0.5).powi(3);
                if k < 0.05 {
                    continue;
                }
                let l = 2.0 + 3.5 * k;
                let col = Rgba(1.0, 1.0, 1.0, 0.85 * k);
                c.line(x - l, y, x + l, y, 1.0, col);
                c.line(x, y - l, x, y + l, 1.0, col);
                c.line(x - l * 0.5, y - l * 0.5, x + l * 0.5, y + l * 0.5, 0.8, col.a(0.6 * k));
                c.line(x - l * 0.5, y + l * 0.5, x + l * 0.5, y - l * 0.5, 0.8, col.a(0.6 * k));
            }
        }
        4 => {
            // Sakura : pétales roses qui tombent en biais
            for i in 0..14u32 {
                let v = 7.0 + h01(i, 10) * 8.0;
                let y = (t * v + h01(i, 11) * (h + 20.0)).rem_euclid(h + 20.0) - 10.0;
                let x = (h01(i, 12) * w + t * v * 0.6 + (t * 1.3 + i as f32).sin() * 5.0).rem_euclid(w);
                let a = 0.55 + 0.3 * (t * 2.0 + i as f32).sin();
                let ww = 3.0 + 2.0 * ((t * 2.5 + i as f32).sin().abs());
                c.rr(x - ww / 2.0, y - 1.8, ww, 3.6, 1.8, Rgba::rgb8(255, 150, 190).a(a));
            }
        }
        5 => {
            // Plage : bulles qui remontent en ondulant
            for i in 0..12u32 {
                let v = 8.0 + h01(i, 13) * 10.0;
                let y = h + 6.0 - (t * v + h01(i, 14) * (h + 12.0)).rem_euclid(h + 12.0);
                let x = h01(i, 15) * w + (t * 2.0 + i as f32).sin() * 4.0;
                let rr = 1.5 + h01(i, 16) * 2.5;
                c.ring(x, y, rr, WHITE.a(0.4), 1.0);
                c.circle(x - rr * 0.35, y - rr * 0.35, rr * 0.25, WHITE.a(0.5));
            }
        }
        6 => {
            // Galaxie : étoiles qui scintillent et une étoile filante de temps en temps
            for i in 0..26u32 {
                let (x, y) = (h01(i, 17) * w, h01(i, 18) * h);
                let k = 0.25 + 0.75 * ((t * (0.8 + h01(i, 19) * 1.5) + h01(i, 20) * 6.3).sin() * 0.5 + 0.5);
                c.circle(x, y, 0.6 + h01(i, 21) * 0.9, WHITE.a(0.7 * k));
            }
            let cycle = t.rem_euclid(7.0);
            if cycle < 0.9 {
                let n = (t / 7.0).floor() as u32;
                let k = cycle / 0.9;
                let (x0, y0) = (w * (0.3 + 0.6 * h01(n, 22)), h * 0.15);
                let (x, y) = (x0 - k * w * 0.25, y0 + k * h * 0.6);
                let fade = 1.0 - k;
                c.line(x, y, x + 26.0, y - 9.0, 1.4, WHITE.a(0.25 * fade));
                c.line(x, y, x + 10.0, y - 3.5, 1.6, WHITE.a(0.8 * fade));
            }
        }
        7 => {
            // Rétro 80 : grille néon qui défile vers le bas, en perspective (plus basse dans la pilule)
            let y0 = h * if h < 70.0 { 0.74 } else { 0.62 };
            let col = acc.a(0.22);
            let off = (t * 0.6).rem_euclid(1.0);
            for k in 0..6 {
                let f = (k as f32 + off) / 6.0;
                let y = y0 + (h - y0) * f * f;
                c.line(0.0, y, w, y, 1.0, col.a(0.10 + 0.25 * f));
            }
            let cx = w / 2.0;
            for k in -8..=8 {
                let xb = cx + k as f32 * w / 10.0;
                c.line(cx + k as f32 * 6.0, y0, xb, h, 1.0, col.a(0.16));
            }
            c.line(0.0, y0, w, y0, 1.2, acc.a(if h < 70.0 { 0.22 } else { 0.45 }));
        }
        _ => {}
    }
}

pub(crate) fn pet(pm: &mut Pixmap, kind: usize, px: f32, py: f32, pw: f32, t: f32, s: f32) {
    let span = (pw - 40.0 * s).max(10.0);
    let period = span / (22.0 * s);
    let ph = (t / period).rem_euclid(2.0);
    let (k, right) = if ph < 1.0 { (ph, true) } else { (2.0 - ph, false) };
    let x = px + 20.0 * s + k * span;
    let y = py; // les pattes sur le bord de la pilule
    let dir = if right { 1.0 } else { -1.0 };
    let step = (t * 10.0).sin();
    let u = s;
    match kind {
        1 => {
            // chat
            let col = Rgba::rgb8(255, 170, 80);
            gfx::line(pm, x - 5.0 * u, y - 3.0 * u, x - 5.0 * u + step * u, y, 1.6 * u, col);
            gfx::line(pm, x + 4.0 * u, y - 3.0 * u, x + 4.0 * u - step * u, y, 1.6 * u, col);
            if let Some(b) = gfx::rrect(x - 8.0 * u, y - 10.0 * u, 16.0 * u, 8.0 * u, 4.0 * u, false) {
                gfx::fill(pm, &b, col);
            }
            let hx = x + dir * 9.0 * u;
            gfx::circle(pm, hx, y - 11.0 * u, 4.5 * u, col);
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(hx - 4.0 * u, y - 13.0 * u);
            pb.line_to(hx - 3.0 * u, y - 18.0 * u);
            pb.line_to(hx - 0.5 * u, y - 14.5 * u);
            pb.move_to(hx + 4.0 * u, y - 13.0 * u);
            pb.line_to(hx + 3.0 * u, y - 18.0 * u);
            pb.line_to(hx + 0.5 * u, y - 14.5 * u);
            if let Some(p) = pb.finish() {
                gfx::fill(pm, &p, col);
            }
            gfx::circle(pm, hx + dir * 1.8 * u, y - 12.0 * u, 0.9 * u, Rgba(0.1, 0.1, 0.1, 1.0));
            gfx::line(pm, x - dir * 8.0 * u, y - 8.0 * u, x - dir * 12.0 * u, y - 14.0 * u + step * u, 1.6 * u, col);
        }
        2 => {
            // canard
            let col = Rgba::rgb8(255, 214, 10);
            gfx::line(pm, x - 2.0 * u, y - 3.0 * u, x - 2.0 * u + step * u, y, 1.4 * u, ORANGE);
            gfx::line(pm, x + 2.0 * u, y - 3.0 * u, x + 2.0 * u - step * u, y, 1.4 * u, ORANGE);
            if let Some(b) = gfx::rrect(x - 7.0 * u, y - 11.0 * u, 14.0 * u, 9.0 * u, 4.5 * u, false) {
                gfx::fill(pm, &b, col);
            }
            let hx = x + dir * 6.0 * u;
            gfx::circle(pm, hx, y - 14.0 * u, 4.0 * u, col);
            gfx::line(pm, hx + dir * 3.0 * u, y - 13.5 * u, hx + dir * 7.0 * u, y - 13.5 * u, 2.2 * u, ORANGE);
            gfx::circle(pm, hx + dir * 1.2 * u, y - 15.0 * u, 0.9 * u, Rgba(0.1, 0.1, 0.1, 1.0));
        }
        _ => {
            // escargot (lent)
            let col = Rgba::rgb8(170, 220, 120);
            if let Some(b) = gfx::rrect(x - 9.0 * u, y - 3.5 * u, 18.0 * u, 3.5 * u, 1.7 * u, false) {
                gfx::fill(pm, &b, col);
            }
            gfx::circle(pm, x - dir * 1.0 * u, y - 8.0 * u, 5.5 * u, Rgba::rgb8(190, 120, 60));
            if let Some(p) = tiny_skia::PathBuilder::from_circle(x - dir * 1.0 * u, y - 8.0 * u, 2.6 * u) {
                gfx::stroke(pm, &p, Rgba::rgb8(120, 70, 30), 1.2 * u);
            }
            gfx::line(pm, x + dir * 8.0 * u, y - 3.0 * u, x + dir * 10.0 * u, y - 9.0 * u, 1.2 * u, col);
            gfx::circle(pm, x + dir * 10.0 * u, y - 9.5 * u, 1.2 * u, col);
        }
    }
}

pub(crate) fn confetti(pm: &mut Pixmap, el: f32, s: f32) {
    if el > 4.0 {
        return;
    }
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let fade = (1.0 - (el - 3.0).max(0.0)).clamp(0.0, 1.0);
    for i in 0..70 {
        let fi = i as f32;
        let x = (fi * 97.13).rem_euclid(w) + (el * 2.0 + fi).sin() * 12.0 * s;
        let y = -10.0 * s + el * (60.0 + (i % 7) as f32 * 18.0) * s + (fi * 13.7).rem_euclid(h * 0.3);
        if y > h {
            continue;
        }
        let col = hsv(fi * 0.137, 0.75, 1.0).a(fade);
        let a = el * (3.0 + (i % 5) as f32) + fi;
        let (dx, dy) = (a.cos() * 4.0 * s, a.sin() * 2.0 * s);
        gfx::line(pm, x - dx, y - dy, x + dx, y + dy, 2.6 * s, col);
    }
}

/// Indicateur animé de Jarvis : point (repos), point qui pulse (écoute), points qui tournent (réfléchit), ondes (parle).
pub(crate) fn jarvis_wave(c: &mut Ctx, cx: f32, cy: f32, state: &str, t: f32) {
    let col = jarvis_color(state);
    match state {
        "ecoute" => {
            let k = (t * 3.0).fract();
            c.ring(cx, cy, 4.0 + 6.0 * k, col.a(0.7 * (1.0 - k)), 1.6);
            c.circle(cx, cy, 4.5, col);
        }
        "reflechit" => {
            for i in 0..3 {
                let a = t * 5.0 + i as f32 * std::f32::consts::TAU / 3.0;
                c.circle(cx + a.cos() * 5.0, cy + a.sin() * 5.0, 2.0, col);
            }
        }
        "parole" => {
            // micro qui capte ta voix : deux petites ondes de chaque côté
            micro_glyph(c, cx, cy, 1.0, col);
            for (i, sens) in [(0, -1.0f32), (1, 1.0)] {
                let k = ((t * 6.0 + i as f32 * 0.5).sin() * 0.5 + 0.5) * 0.8 + 0.2;
                c.line(cx + sens * 7.5, cy - 2.5 * k - 1.0, cx + sens * 7.5, cy + 2.5 * k - 1.0, 1.6, col.a(0.85));
                c.line(cx + sens * 10.5, cy - 4.0 * k - 1.0, cx + sens * 10.5, cy + 4.0 * k - 1.0, 1.4, col.a(0.55));
            }
        }
        "parle" => {
            for i in 0..4 {
                let hh = 3.0 + 5.0 * ((t * 9.0 + i as f32 * 1.3).sin() * 0.5 + 0.5);
                c.rr(cx - 7.0 + i as f32 * 4.0, cy - hh, 2.6, hh * 2.0, 1.3, col);
            }
        }
        "parle_ecoute" => {
            // il continue de parler (ondes bleues) ET il t'écoute (petit micro orange qui clignote)
            let bleu = jarvis_color("parle");
            for i in 0..3 {
                let hh = 2.5 + 4.0 * ((t * 9.0 + i as f32 * 1.3).sin() * 0.5 + 0.5);
                c.rr(cx - 11.0 + i as f32 * 4.0, cy - hh, 2.4, hh * 2.0, 1.2, bleu);
            }
            let k = (t * 4.0).sin() * 0.5 + 0.5;
            c.circle(cx + 6.0, cy - 1.0, 7.0, col.a(0.18 + 0.2 * k));
            micro_glyph(c, cx + 6.0, cy, 0.8, col);
        }
        _ => c.circle(cx, cy, 3.5, col),
    }
}

/// Petit micro (capsule, arceau et pied), centré en (cx, cy), à l'échelle `k`.
pub(crate) fn micro_glyph(c: &mut Ctx, cx: f32, cy: f32, k: f32, col: Rgba) {
    c.rr(cx - 2.6 * k, cy - 8.0 * k, 5.2 * k, 9.5 * k, 2.6 * k, col);
    let w = 1.4 * k;
    c.line(cx - 4.6 * k, cy - 2.0 * k, cx - 4.6 * k, cy - 0.5 * k, w, col);
    c.line(cx + 4.6 * k, cy - 2.0 * k, cx + 4.6 * k, cy - 0.5 * k, w, col);
    c.line(cx - 4.6 * k, cy - 0.5 * k, cx - 2.0 * k, cy + 3.2 * k, w, col);
    c.line(cx + 4.6 * k, cy - 0.5 * k, cx + 2.0 * k, cy + 3.2 * k, w, col);
    c.line(cx - 2.0 * k, cy + 3.2 * k, cx + 2.0 * k, cy + 3.2 * k, w, col);
    c.line(cx, cy + 3.2 * k, cx, cy + 6.5 * k, w, col);
    c.line(cx - 3.0 * k, cy + 6.5 * k, cx + 3.0 * k, cy + 6.5 * k, w, col);
}
