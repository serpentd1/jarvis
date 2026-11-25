//! Options de l'île et de la grande application, rangées par onglets (affichées dans la grande
//! application › Paramètres, voir reglages.rs). Chaque option est décrite une seule fois dans `opts()`.

use crate::brain::{MODELS, STYLES};
use crate::config::IleCfg;
use crate::gfx::FONT_NAMES;
use crate::island_cards::GRAD_NAMES;
use crate::island_draw::{ACCENT_NAMES, PRESET_NAMES};

pub const TABS: [&str; 18] = [
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
    "Grande appli",
    "Couleurs et fond",
    "Texte et lisibilité",
    "Animations de la grande appli",
];

pub const NIVEAUX_IA: [&str; 3] = ["Complète", "Semi (légère)", "Coupée"];
pub const BOUTON_IA: [&str; 3] = ["Au survol", "Toujours", "Caché"];
pub const DEPARTS: [&str; 7] = ["Accueil", "Aujourd'hui", "Dernière page ouverte", "Jarvis", "Tâches", "Pronote", "Paramètres"];
pub const FONDS: [&str; 7] = ["Comme l'île", "Plus sombre", "Plus clair", "Dégradé de l'accent", "Noir profond", "Bleu nuit", "Violet"];

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


pub fn opts() -> Vec<Opt> {
    vec![
        // 0 Apparence
        ch!(0, "Thème", preset, &PRESET_NAMES),
        b!(0, "Décors animés du thème (neige à Noël, chauves-souris…)", deco),
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
        b!(2, "Se cacher quand une fenêtre est agrandie", hide_maximized),
        b!(2, "Cacher l'île automatiquement", auto_hide),
        n!(2, "  après N s sans survol", auto_hide_secs, 0.0, 300.0, 5.0, "s"),
        n!(2, "  temps en haut de l'écran pour la faire revenir", reveal_secs, 0.1, 3.0, 0.1, "s"),
        b!(2, "  laisser dépasser une petite languette", languette),
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
        ch!(9, "Niveau de l'IA", ia_niveau, &NIVEAUX_IA),
        ch!(9, "Bouton IA dans la barre du haut", ia_bouton, &BOUTON_IA),
        ch!(9, "Modèle", ai_model, &MODELS),
        n!(9, "Mémoire (échanges retenus)", ai_history, 0.0, 10.0, 1.0, ""),
        b!(9, "Lui donner Pronote, mails, météo…", ai_context),
        b!(9, "Peut agir (minuteurs, alarme, ouvrir…)", ai_actions),
        ch!(9, "Style des réponses", ai_style, &STYLES),
        n!(9, "Créativité", ai_temp, 0.0, 1.0, 0.1, "%"),
        b!(9, "API locale pour Jarvis", api_on),
        n!(9, "Port de l'API", api_port, 1024.0, 65000.0, 1.0, ""),
        b!(9, "Montrer l'état de Jarvis", jarvis_show),
        b!(9, "Démarrer l'assistant vocal Jarvis", jarvis_autostart),
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
        b!(10, "Île : téléphone (appels, SMS)", ni_phone),
        b!(10, "Windows : Pronote", nt_pronote),
        b!(10, "Windows : mails", nt_mail),
        b!(10, "Windows : Discord", nt_discord),
        b!(10, "Windows : minuteurs et alarme", nt_timer),
        b!(10, "Windows : Jarvis et autres", nt_jarvis),
        b!(10, "Windows : téléphone", nt_phone),
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
        ch!(13, "Mode de l'île", grande, &["Normale", "Grande (dépliée)"]),
        b!(13, "Cacher les onglets vides", hide_empty_tabs),
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
        n!(13, "  position", ot_mails, 1.0, 11.0, 1.0, ""),
        b!(13, "Onglet Téléphone", t_phone),
        n!(13, "  position", ot_phone, 1.0, 20.0, 1.0, ""),
        b!(13, "Onglet Tâches", t_tasks),
        n!(13, "  position", ot_tasks, 1.0, 20.0, 1.0, ""),
        b!(13, "Onglet Rappels", t_reminders),
        n!(13, "  position", ot_reminders, 1.0, 20.0, 1.0, ""),
        b!(13, "Onglet Habitudes", t_habits),
        n!(13, "  position", ot_habits, 1.0, 20.0, 1.0, ""),
        b!(13, "Onglet Routines", t_routines),
        n!(13, "  position", ot_routines, 1.0, 20.0, 1.0, ""),
        b!(13, "Onglet Mémoire", t_memory),
        n!(13, "  position", ot_memory, 1.0, 20.0, 1.0, ""),
        b!(13, "Onglet Mon PC", t_pc),
        n!(13, "  position", ot_pc, 1.0, 20.0, 1.0, ""),
        b!(13, "Onglet Batteries", t_batteries),
        n!(13, "  position", ot_batteries, 1.0, 20.0, 1.0, ""),
        // 14 Grande appli
        n!(14, "Largeur de la grande appli", g_largeur, 0.75, 1.3, 0.05, "%"),
        n!(14, "Hauteur de la grande appli", g_hauteur, 0.75, 1.3, 0.05, "%"),
        n!(14, "Largeur de la colonne de gauche", g_colonne, 150.0, 280.0, 10.0, "px"),
        b!(14, "Colonne réduite (icônes seulement)", g_reduite),
        b!(14, "Icônes dans la colonne", g_icones),
        n!(14, "Hauteur des lignes de la colonne", g_espace, 24.0, 40.0, 1.0, "px"),
        ch!(14, "Page qui s'ouvre en premier", g_depart, &DEPARTS),
        b!(14, "Heure en bas de la colonne", g_heure),
        // 15 Couleurs et fond
        ch!(15, "Fond de la grande appli", g_fond, &FONDS),
        n!(15, "Transparence en plus", g_transp, 0.0, 0.7, 0.05, "%"),
        ch!(15, "Verre flouté derrière", g_flou, &["Comme l'île", "Toujours", "Jamais"]),
        n!(15, "Arrondi des coins (0 = comme l'île)", g_arrondi, 0.0, 40.0, 2.0, "px"),
        b!(15, "Liseré de la couleur d'accent", g_bord),
        b!(15, "Clair le jour, sombre la nuit (toute l'île)", g_auto_theme),
        n!(15, "  le jour commence à", g_jour, 4.0, 12.0, 0.5, "h"),
        n!(15, "  la nuit commence à", g_nuit, 15.0, 23.5, 0.5, "h"),
        b!(15, "Une couleur pour chaque section", g_couleurs),
        ch!(15, "Section choisie", g_selection, &["Gris doux", "Couleur d'accent", "Contour", "Barre à gauche"]),
        b!(15, "Fond derrière la colonne", g_fond_colonne),
        b!(15, "Ligne entre la colonne et la page", g_ligne),
        // 16 Texte et lisibilité
        n!(16, "Taille du texte de la grande appli", g_texte, 0.85, 1.35, 0.05, "%"),
        b!(16, "Contraste fort (textes gris plus lisibles)", g_contraste),
        b!(16, "Noms des sections en gras", g_gras),
        // 17 Animations de la grande appli
        ch!(17, "Changement de page", g_anim_page, &["Aucune", "Fondu", "Glissement", "Zoom léger"]),
        n!(17, "  durée", g_anim_duree, 100.0, 800.0, 50.0, "ms"),
        b!(17, "Animer l'ouverture de la grande appli", g_anim_ouverture),
        b!(17, "Section survolée qui glisse", g_survol),
    ]
}

pub fn val_txt(o: &Opt, c: &mut IleCfg) -> String {
    match &o.k {
        K::B(f) => if *f(c) { "Oui".into() } else { "Non".into() },
        K::N(f, _, _, step, unit) => {
            let v = *f(c);
            match *unit {
                "%" => format!("{} %", (v * 100.0).round()),
                "h" => {
                    let m = (v * 60.0).round() as i32;
                    if m % 60 == 0 { format!("{} h", m / 60) } else { format!("{} h {:02}", m / 60, m % 60) }
                }
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
