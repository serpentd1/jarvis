//! Fenêtre « Réglages de l'île » : toutes les options de la Dynamic Island, rangées par onglets.
//! Chaque option est décrite une seule fois dans `opts()` (onglet, nom, type, limites).
//! Clic sur un interrupteur : on/off. Flèches ‹ › (ou clic gauche / droit sur la ligne) : valeur suivante / précédente.

use tiny_skia::Pixmap;
use windows_sys::Win32::Foundation::HWND;

use crate::brain::{MODELS, STYLES};
use crate::config::IleCfg;
use crate::gfx::{FONT_NAMES, Fonts, Rgba, Theme, WHITE, Wt};
use crate::island_cards::GRAD_NAMES;
use crate::island_draw::{ACCENT_NAMES, PRESETS};
use crate::widgets::Ctx;

pub const TABS: [&str; 14] = [
    "Apparence",
    "Effets",
    "Taille et position",
    "Texte",
    "Heure et date",
    "Météo",
    "Modules",
    "Minuteurs et alarme",
    "Pronote et mails",
    "IA et Jarvis",
    "Notifications",
    "Animations",
    "Options rigolotes",
    "Disposition et onglets",
];

pub enum K {
    B(fn(&mut IleCfg) -> &mut bool),
    N(fn(&mut IleCfg) -> &mut f32, f32, f32, f32, &'static str),
    C(fn(&mut IleCfg) -> &mut usize, &'static [&'static str]),
}

pub struct Opt {
    pub tab: usize,
    pub label: &'static str,
    pub k: K,
}

macro_rules! b {
    ($t:expr, $l:expr, $f:ident) => {
        Opt { tab: $t, label: $l, k: K::B(|c| &mut c.$f) }
    };
}
macro_rules! n {
    ($t:expr, $l:expr, $f:ident, $min:expr, $max:expr, $step:expr, $u:expr) => {
        Opt { tab: $t, label: $l, k: K::N(|c| &mut c.$f, $min, $max, $step, $u) }
    };
}
macro_rules! ch {
    ($t:expr, $l:expr, $f:ident, $list:expr) => {
        Opt { tab: $t, label: $l, k: K::C(|c| &mut c.$f, $list) }
    };
}

const PRESET_NAMES: [&str; 10] = ["Nuit", "Verre", "Aurore", "Océan", "Forêt", "Coucher de soleil", "Bonbon", "Néon", "Clair", "Café"];

pub fn opts() -> Vec<Opt> {
    vec![
        // 0 Apparence
        ch!(0, "Thème", preset, &PRESET_NAMES),
        ch!(0, "Couleur d'accent", accent, &ACCENT_NAMES),
        b!(0, "Fond en dégradé", gradient),
        n!(0, "Arrondi des coins", radius, 6.0, 40.0, 2.0, "px"),
        b!(0, "Liseré autour de l'île", border),
        b!(0, "Modules dans des cartes", glass_cards),
        b!(0, "Lignes entre les modules", separators),
        // 1 Effets
        n!(1, "Opacité du fond", opacity, 0.2, 1.0, 0.05, "%"),
        b!(1, "Verre flouté (fond d'écran)", blur),
        n!(1, "Force du flou", blur_amount, 0.0, 1.0, 0.1, "%"),
        b!(1, "Lueur colorée", glow),
        n!(1, "Force de la lueur", glow_strength, 0.1, 1.0, 0.1, "%"),
        b!(1, "Ombre", shadow),
        b!(1, "Fondu du contenu", fade),
        // 2 Taille et position
        n!(2, "Taille de l'île", size, 0.7, 1.5, 0.05, "%"),
        ch!(2, "Position", position, &["Centre", "Gauche", "Droite"]),
        n!(2, "Écart avec le haut", top_margin, 0.0, 40.0, 2.0, "px"),
        n!(2, "Largeur de la pilule", pill_width, 200.0, 440.0, 10.0, "px"),
        n!(2, "Hauteur de la pilule", pill_height, 28.0, 52.0, 2.0, "px"),
        n!(2, "Zone de survol autour", hover_zone, 4.0, 60.0, 4.0, "px"),
        b!(2, "Cacher en plein écran (jeux, vidéos)", hide_fullscreen),
        b!(2, "Cacher l'île automatiquement", auto_hide),
        n!(2, "  après N s sans survol", auto_hide_secs, 0.0, 300.0, 5.0, "s"),
        n!(2, "  temps en haut de l'écran pour la faire revenir", reveal_secs, 0.1, 3.0, 0.1, "s"),
        b!(2, "Toujours ouverte", always_open),
        b!(2, "Ouvrir au clic plutôt qu'au survol", open_on_click),
        // 3 Texte
        ch!(3, "Police", font, &FONT_NAMES),
        n!(3, "Taille du texte", text_size, 0.8, 1.3, 0.05, "%"),
        b!(3, "Texte en gras", bold),
        b!(3, "Titres en majuscules", caps_headers),
        b!(3, "Mode pirate (textes)", pirate),
        // 4 Heure et date
        b!(4, "Format 24 h", h24),
        b!(4, "Afficher les secondes", seconds),
        ch!(4, "Format de la date", date_fmt, &["Samedi 14 novembre", "14/11/2026", "sam. 14 nov.", "14 novembre 2026"]),
        b!(4, "Numéro de semaine", week_num),
        n!(4, "Taille de l'heure", clock_size, 32.0, 74.0, 2.0, "px"),
        ch!(4, "Épaisseur de l'heure", clock_weight, &["Fine", "Normale", "Grasse"]),
        ch!(4, "Couleur de l'heure", clock_color, &["Blanche", "Accent", "Arc-en-ciel", "Selon la température"]),
        b!(4, "Date dans la pilule", pill_date),
        b!(4, "Barre de progression de la journée", day_progress),
        b!(4, "Message d'accueil", greeting),
        // 5 Météo
        b!(5, "Degrés Fahrenheit", fahrenheit),
        ch!(5, "Unité du vent", wind_unit, &["km/h", "m/s", "mph"]),
        b!(5, "Icônes animées", weather_anim),
        b!(5, "Fond coloré selon le temps", weather_bg),
        b!(5, "Petite phrase sympa", weather_phrase),
        b!(5, "Température ressentie", show_feels),
        b!(5, "Minimum et maximum", show_minmax),
        b!(5, "Prévisions des prochaines heures", forecast),
        b!(5, "Météo dans la pilule", pill_weather),
        // 6 Modules
        b!(6, "Heure, date et alarme", m_time),
        n!(6, "  position", o_time, 1.0, 7.0, 1.0, ""),
        b!(6, "Chrono et minuteurs", m_timers),
        n!(6, "  position", o_timers, 1.0, 7.0, 1.0, ""),
        b!(6, "Météo", m_weather),
        n!(6, "  position", o_weather, 1.0, 7.0, 1.0, ""),
        b!(6, "Pronote", m_pronote),
        n!(6, "  position", o_pronote, 1.0, 7.0, 1.0, ""),
        b!(6, "Mails importants", m_mail),
        n!(6, "  position", o_mail, 1.0, 7.0, 1.0, ""),
        b!(6, "Discord", m_discord),
        n!(6, "  position", o_discord, 1.0, 7.0, 1.0, ""),
        b!(6, "Assistant IA", m_ai),
        n!(6, "  position", o_ai, 1.0, 7.0, 1.0, ""),
        // 7 Minuteurs et alarme
        b!(7, "Afficher le chronomètre", show_chrono),
        b!(7, "Afficher les minuteurs", show_timers),
        b!(7, "Afficher l'alarme", show_alarm),
        ch!(7, "Boutons rapides", timer_presets, &["1 / 5 / 10 min", "5 / 15 / 25 min", "10 / 30 / 60 min"]),
        b!(7, "Minuteur en cours dans la pilule", pill_timer),
        ch!(7, "Jours de l'alarme", alarm_days, &["Tous les jours", "Lundi au vendredi", "Week-end"]),
        ch!(7, "Son de la sonnerie", ring_sound, &["Bip", "Ding", "Alerte", "Erreur", "Silencieux"]),
        n!(7, "Arrêt automatique après", ring_minutes, 1.0, 15.0, 1.0, "min"),
        n!(7, "Durée de « répéter »", snooze_min, 1.0, 20.0, 1.0, "min"),
        // 8 Pronote et mails
        b!(8, "Afficher les notes Pronote sur l'île", show_grades),
        n!(8, "Devoirs des prochains jours", hw_days, 1.0, 21.0, 1.0, "j"),
        b!(8, "Cacher les devoirs faits", hide_done),
        n!(8, "Nombre de notes affichées", grades_n, 0.0, 6.0, 1.0, ""),
        b!(8, "Prochain cours", show_next_class),
        b!(8, "Moyenne générale", show_avg),
        n!(8, "Vérifier les mails toutes les", mail_refresh, 1.0, 30.0, 1.0, "min"),
        b!(8, "« À répondre » compte comme important", imp_reply),
        b!(8, "Étiquette « Important » de Gmail compte", imp_gmail),
        b!(8, "Seulement les mails urgents", only_urgent),
        n!(8, "Nombre de mails affichés", mails_n, 1.0, 6.0, 1.0, ""),
        b!(8, "Extrait du mail au lieu de l'objet", show_snippet),
        b!(8, "Pastille des mails dans la pilule", pill_mail_badge),
        n!(8, "Messages Discord affichés", discord_n, 1.0, 8.0, 1.0, ""),
        b!(8, "Non-lus Discord via le titre de Chrome", discord_title),
        b!(8, "Discord : afficher aussi les messages des serveurs", discord_servers),
        // 9 IA et Jarvis
        ch!(9, "Modèle", ai_model, &MODELS),
        n!(9, "Mémoire (échanges retenus)", ai_history, 0.0, 10.0, 1.0, ""),
        b!(9, "Lui donner Pronote, mails, météo…", ai_context),
        b!(9, "Peut agir (minuteurs, alarme, ouvrir…)", ai_actions),
        ch!(9, "Style des réponses", ai_style, &STYLES),
        n!(9, "Créativité", ai_temp, 0.0, 1.0, 0.1, "%"),
        b!(9, "API locale pour Jarvis", api_on),
        n!(9, "Port de l'API", api_port, 1024.0, 65000.0, 1.0, ""),
        b!(9, "Montrer l'état de Jarvis", jarvis_show),
        b!(9, "Lancer Jarvis avec Givre", jarvis_autostart),
        // 10 Notifications
        b!(10, "Afficher dans l'île", n_island),
        b!(10, "Notification Windows aussi", n_toast),
        n!(10, "Durée d'affichage", n_secs, 2.0, 20.0, 1.0, "s"),
        b!(10, "Petit son doux", n_sound),
        b!(10, "Ne pas déranger", dnd),
        b!(10, "Île : Pronote", ni_pronote),
        b!(10, "Île : mails", ni_mail),
        b!(10, "Île : Discord", ni_discord),
        b!(10, "Île : minuteurs et alarme", ni_timer),
        b!(10, "Île : Jarvis et autres", ni_jarvis),
        b!(10, "Windows : Pronote", nt_pronote),
        b!(10, "Windows : mails", nt_mail),
        b!(10, "Windows : Discord", nt_discord),
        b!(10, "Windows : minuteurs et alarme", nt_timer),
        b!(10, "Windows : Jarvis et autres", nt_jarvis),
        // 11 Animations
        n!(11, "Vitesse des animations", anim_speed, 0.3, 2.5, 0.1, "%"),
        n!(11, "Délai avant ouverture", open_delay, 0.0, 1000.0, 20.0, "ms"),
        n!(11, "Délai avant fermeture", close_delay, 100.0, 3000.0, 100.0, "ms"),
        b!(11, "Rebond à l'ouverture", bounce),
        b!(11, "Animation de sortie vers le haut", hide_anim),
        b!(11, "Pilule qui respire", breathe),
        b!(11, "Petit son doux à l'ouverture", open_sound),
        // 12 Options rigolotes
        b!(12, "Liseré arc-en-ciel", rainbow),
        b!(12, "Mode disco (couleurs qui changent)", disco),
        b!(12, "Confettis pour une bonne note", confetti),
        n!(12, "  à partir de", confetti_min, 10.0, 20.0, 1.0, "/20"),
        ch!(12, "Petit animal sur la pilule", pet, &["Aucun", "Chat", "Canard", "Escargot"]),
        b!(12, "Compliments", compliments),
        b!(12, "Yeux qui suivent la souris", eyes),
        b!(12, "Neige dans l'île", snow),
        // 13 Disposition et onglets
        ch!(13, "Disposition", layout, &["Cartes + onglets", "Colonnes"]),
        ch!(13, "Carte de gauche", left_card, &["Jarvis", "Devoirs", "Mails", "Météo"]),
        ch!(13, "Couleur de la carte de gauche", left_grad, &GRAD_NAMES),
        ch!(13, "Couleur de la carte Focus", right_grad, &GRAD_NAMES),
        b!(13, "Pochette floutée", art_blur),
        n!(13, "Pomodoro : concentration", pomo_work, 5.0, 90.0, 5.0, "min"),
        n!(13, "Pomodoro : pause", pomo_break, 1.0, 30.0, 1.0, "min"),
        b!(13, "Onglet Jarvis (chat)", t_jarvis),
        n!(13, "  position", ot_jarvis, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Musique", t_music),
        n!(13, "  position", ot_music, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Discord", t_discord),
        n!(13, "  position", ot_discord, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Minuteurs", t_timers),
        n!(13, "  position", ot_timers, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Ne pas déranger", t_dnd),
        n!(13, "  position", ot_dnd, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Notifications", t_notifs),
        n!(13, "  position", ot_notifs, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Presse-papiers", t_clips),
        n!(13, "  position", ot_clips, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Pronote", t_school),
        n!(13, "  position", ot_school, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Agenda", t_calendar),
        n!(13, "  position", ot_calendar, 1.0, 10.0, 1.0, ""),
        b!(13, "Onglet Mails", t_mails),
        n!(13, "  position", ot_mails, 1.0, 10.0, 1.0, ""),

    ]
}

pub struct IleSettings {
    pub hwnd: HWND,
    pub tab: usize,
    pub scroll: usize,
    pub hits: Vec<(f32, f32, f32, f32, SAct)>,
    pub pm: Option<Pixmap>,
}

#[derive(Clone, Copy, Debug)]
pub enum SAct {
    Tab(usize),
    Toggle(usize),
    Dec(usize),
    Inc(usize),
    Reset,
}

pub const W: f32 = 820.0;
pub const H: f32 = 640.0;
const ROW: f32 = 44.0;
const TOP: f32 = 76.0;

fn val_txt(o: &Opt, c: &mut IleCfg) -> String {
    match &o.k {
        K::B(f) => if *f(c) { "Oui".into() } else { "Non".into() },
        K::N(f, _, _, step, unit) => {
            let v = *f(c);
            match *unit {
                "%" => format!("{} %", (v * 100.0).round()),
                u if *step < 1.0 => format!("{v:.1} {u}"),
                u => format!("{} {u}", v.round()),
            }
        }
        K::C(f, list) => list.get(*f(c)).copied().unwrap_or("?").to_string(),
    }
}

/// Applique un clic : `dir` = +1 (suivant / activer), -1 (précédent).
pub fn change(o: &Opt, c: &mut IleCfg, dir: i32) {
    match &o.k {
        K::B(f) => {
            let v = f(c);
            *v = !*v;
        }
        K::N(f, min, max, step, _) => {
            let v = f(c);
            *v = ((*v + *step * dir as f32) / step).round() * step;
            *v = v.clamp(*min, *max);
        }
        K::C(f, list) => {
            let v = f(c);
            let n = list.len().max(1) as i32;
            *v = ((*v as i32 + dir).rem_euclid(n)) as usize;
        }
    }
}

pub fn reset_tab(tab: usize, c: &mut IleCfg) {
    let mut d = IleCfg::default();
    for o in opts().iter().filter(|o| o.tab == tab) {
        match &o.k {
            K::B(f) => *f(c) = *f(&mut d),
            K::N(f, ..) => *f(c) = *f(&mut d),
            K::C(f, _) => *f(c) = *f(&mut d),
        }
    }
}

/// Dessine la fenêtre (taille logique W × H, à l'échelle `s`).
pub fn draw(st: &mut IleSettings, c: &mut IleCfg, fonts: &Fonts, accent: [u8; 3], s: f32) {
    let Some(mut pm) = Pixmap::new((W * s) as u32, (H * s) as u32) else { return };
    let th = Theme::new(true, accent, 1.0);
    let p = &PRESETS[c.preset.min(PRESETS.len() - 1)];
    pm.fill(tiny_skia::Color::from_rgba8(18, 18, 22, 255));
    let mut hits = Vec::new();
    let list = opts();
    let count = list.len();
    {
        let mut x = Ctx::new(&mut pm, fonts, &th, s, 0.0, 0.0);
        let t = x.t;
        // colonne des onglets
        x.grad_rr(0.0, 0.0, 220.0, H, 0.0, Rgba::from(p.c0).a(0.9), Rgba::from(p.c1).a(0.9));
        x.text(Wt::Semibold, 20.0, 22.0, 42.0, WHITE, "Réglages de l'île");
        x.text(Wt::Regular, 12.0, 22.0, 62.0, WHITE.a(0.6), &format!("{count} options, enregistrées tout de suite"));
        for (i, name) in TABS.iter().enumerate() {
            let y = 80.0 + i as f32 * 36.0;
            if i == st.tab {
                x.rr(12.0, y, 196.0, 32.0, 10.0, WHITE.a(0.16));
            }
            let n = list.iter().filter(|o| o.tab == i).count();
            x.text(Wt::Semibold, 13.5, 26.0, y + 21.0, if i == st.tab { WHITE } else { WHITE.a(0.75) }, name);
            x.text_r(Wt::Regular, 11.5, 196.0, y + 21.0, WHITE.a(0.45), &n.to_string());
            hits.push((12.0 * s, y * s, 196.0 * s, 32.0 * s, SAct::Tab(i)));
        }
        // contenu
        let x0 = 250.0;
        let xr = W - 28.0;
        x.text(Wt::Semibold, 22.0, x0, 46.0, t.fg, TABS[st.tab]);
        let rw = x.measure(Wt::Semibold, 12.0, "Réinitialiser l'onglet") + 22.0;
        x.rr(xr - rw, 26.0, rw, 26.0, 13.0, t.fg.a(0.1));
        x.text(Wt::Semibold, 12.0, xr - rw + 11.0, 43.5, t.fg2, "Réinitialiser l'onglet");
        hits.push(((xr - rw) * s, 26.0 * s, rw * s, 26.0 * s, SAct::Reset));
        let rows: Vec<usize> = (0..list.len()).filter(|i| list[*i].tab == st.tab).collect();
        let fit = ((H - TOP - 40.0) / ROW).floor() as usize;
        st.scroll = st.scroll.min(rows.len().saturating_sub(fit));
        for (k, &i) in rows.iter().skip(st.scroll).take(fit).enumerate() {
            let o = &list[i];
            let y = TOP + k as f32 * ROW;
            x.rr(x0 - 10.0, y, xr - x0 + 10.0, ROW - 6.0, 12.0, t.fg.a(0.05));
            let sub = o.label.starts_with("  ");
            x.text(Wt::Regular, 14.0, x0 + if sub { 16.0 } else { 4.0 }, y + 24.0, if sub { t.fg2 } else { t.fg }, o.label.trim());
            match &o.k {
                K::B(f) => {
                    let on = *f(c);
                    let (sx, sy) = (xr - 56.0, y + 8.0);
                    x.rr(sx, sy, 42.0, 22.0, 11.0, if on { t.accent } else { t.track });
                    x.circle(if on { sx + 31.0 } else { sx + 11.0 }, sy + 11.0, 9.0, WHITE);
                    hits.push(((x0 - 10.0) * s, y * s, (xr - x0 + 10.0) * s, (ROW - 6.0) * s, SAct::Toggle(i)));
                }
                _ => {
                    let v = val_txt(o, c);
                    let vw = x.measure(Wt::Semibold, 13.5, &v).max(60.0);
                    let cx = xr - 40.0 - vw / 2.0;
                    x.text_c(Wt::Semibold, 13.5, cx, y + 24.0, t.fg, &v);
                    let (lx, rx) = (cx - vw / 2.0 - 18.0, xr - 16.0);
                    x.circle(lx, y + 19.0, 12.0, t.fg.a(0.1));
                    x.chevron(lx, y + 19.0, 5.0, true, t.fg);
                    x.circle(rx, y + 19.0, 12.0, t.fg.a(0.1));
                    x.chevron(rx, y + 19.0, 5.0, false, t.fg);
                    hits.push(((lx - 14.0) * s, y * s, 28.0 * s, (ROW - 6.0) * s, SAct::Dec(i)));
                    hits.push(((rx - 14.0) * s, y * s, 28.0 * s, (ROW - 6.0) * s, SAct::Inc(i)));
                    hits.push(((x0 - 10.0) * s, y * s, (lx - x0 - 4.0) * s, (ROW - 6.0) * s, SAct::Inc(i)));
                }
            }
        }
        if rows.len() > fit {
            x.text_c(Wt::Regular, 12.0, (x0 + xr) / 2.0, H - 16.0, t.fg3, "Molette pour voir la suite");
        }
        x.text(Wt::Regular, 11.5, 22.0, H - 18.0, WHITE.a(0.5), "Clic droit sur une ligne : valeur précédente");
    }
    st.hits = hits;
    st.pm = Some(pm);
}

pub fn hit(st: &IleSettings, x: f32, y: f32) -> Option<SAct> {
    // les zones ajoutées en dernier (flèches) passent avant la ligne entière
    st.hits.iter().rev().find(|h| x >= h.0 && x < h.0 + h.2 && y >= h.1 && y < h.1 + h.3).map(|h| h.4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beaucoup_d_options_et_toutes_modifiables() {
        let l = opts();
        assert!(l.len() >= 60, "{} options seulement", l.len());
        let mut c = IleCfg::default();
        for o in &l {
            change(o, &mut c, 1);
            change(o, &mut c, -1);
            let _ = val_txt(o, &mut c);
        }
        for t in 0..TABS.len() {
            assert!(l.iter().any(|o| o.tab == t), "onglet vide : {}", TABS[t]);
            reset_tab(t, &mut c);
        }
    }
}
