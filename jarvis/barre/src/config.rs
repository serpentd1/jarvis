//! Réglages de Givre, enregistrés dans %APPDATA%\Givre\config.json.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x: i32,
    pub y: i32,
    #[serde(default = "one")]
    pub scale: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct ClockCfg {
    pub on: bool,
    pub seconds: bool,
    pub h24: bool,
    pub date: bool,
    pub week: bool,
}

impl Default for ClockCfg {
    fn default() -> Self {
        Self { on: true, seconds: false, h24: true, date: true, week: true }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct WeatherCfg {
    pub on: bool,
    pub city: String,
    pub lat: f64,
    pub lon: f64,
    pub hourly: bool,
    pub daily: bool,
}

impl Default for WeatherCfg {
    fn default() -> Self {
        Self {
            on: true,
            city: "Paris".into(),
            lat: 48.85661,
            lon: 2.35222,
            hourly: true,
            daily: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct CalCfg {
    pub on: bool,
    pub agenda: bool,
    pub holidays: bool,
}

impl Default for CalCfg {
    fn default() -> Self {
        Self { on: true, agenda: true, holidays: true }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct CountCfg {
    pub on: bool,
    pub academie: String,
    pub exam_name: String,
    pub exam_date: String,
}

impl Default for CountCfg {
    fn default() -> Self {
        Self { on: true, academie: "Paris".into(), exam_name: "Brevet".into(), exam_date: String::new() }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct MailCfg {
    pub on: bool,
    pub ai: bool,
    /// Ancien réglage (modèle des mails), gardé pour lire les vieux fichiers. Le choix se fait dans ia.bin.
    #[allow(dead_code)]
    pub model: String,
    /// Ancienne adresse d'Ollama : sert seulement à la première lecture de ia.bin (migration).
    pub ollama_url: String,
    pub notify: bool,
    pub ms_client: String,
}

impl Default for MailCfg {
    fn default() -> Self {
        Self { on: true, ai: true, model: "gemma3:4b".into(), ollama_url: String::new(), notify: true, ms_client: String::new() }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct DiscordCfg {
    pub on: bool,
    pub id: String,
    pub secret: String,
    pub notify: bool,
    /// Lire les notifications Discord gardées par Windows (centre de notifications).
    pub win_notifs: bool,
    /// Ne garder que les messages privés…
    pub dm_only: bool,
    /// … plus les mentions sur les serveurs si activé.
    pub mentions: bool,
    /// Mots-clés séparés par des virgules (vide = tous les messages).
    pub keywords: String,
    /// Pseudos séparés par des virgules (vide = tout le monde).
    pub people: String,
}

impl Default for DiscordCfg {
    fn default() -> Self {
        Self {
            on: true,
            id: String::new(),
            secret: String::new(),
            notify: false,
            win_notifs: true,
            dm_only: true,
            mentions: false,
            keywords: String::new(),
            people: String::new(),
        }
    }
}

/// Une notification Discord vient d'un serveur (et pas d'un MP) quand son titre a la forme
/// « Pseudo (#salon, Serveur) » ou contient un nom de salon « #… ».
pub fn discord_from_server(title: &str) -> bool {
    title.contains("(#") || title.contains(" #") || title.starts_with('#')
}

fn list(s: &str) -> Vec<String> {
    s.split([',', ';']).map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect()
}

impl DiscordCfg {
    /// Vrai si la notification est « importante » d'après les réglages (MP, mentions, mots-clés, personnes).
    pub fn important(&self, title: &str, body: &str) -> bool {
        let server = discord_from_server(title);
        if self.dm_only && server && !self.mentions {
            return false;
        }
        let (kw, people) = (list(&self.keywords), list(&self.people));
        if kw.is_empty() && people.is_empty() {
            return true;
        }
        let sender = title.split(" (").next().unwrap_or(title).to_lowercase();
        let text = format!("{title}\n{body}").to_lowercase();
        people.iter().any(|p| sender.contains(p.as_str())) || kw.iter().any(|k| text.contains(k.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_filter() {
        let mut c = DiscordCfg::default();
        assert!(c.important("Lucas", "salut"));
        assert!(!c.important("Lucas (#général, Classe 3eB)", "@toi regarde"));
        c.mentions = true;
        assert!(c.important("Lucas (#général, Classe 3eB)", "@toi regarde"));
        c.keywords = "brevet, urgent".into();
        assert!(!c.important("Lucas", "salut"));
        assert!(c.important("Lucas", "C'est URGENT"));
        c.people = "maman".into();
        assert!(c.important("Maman", "à table"));
        c.dm_only = false;
        c.mentions = false;
        c.keywords.clear();
        c.people.clear();
        assert!(c.important("Lucas (#général, Classe 3eB)", "x"));
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PrinterCfg {
    pub on: bool,
    pub ip: String,
    pub app: String,
}

impl Default for PrinterCfg {
    fn default() -> Self {
        Self {
            on: false,
            ip: String::new(),
            app: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PronoteCfg {
    /// Faux = Pronote désactivé (page Comptes) : pas d'alerte « pas connecté », Jarvis n'en parle pas.
    pub on: bool,
    pub next: bool,
    pub homework: bool,
    pub grades: bool,
    pub info: bool,
    pub n_grade: bool,
    pub n_hw: bool,
    pub n_cancel: bool,
    pub google: bool,
    /// PAI : mon emploi du temps à moi (voir pai.rs).
    pub pai: crate::pai::Pai,
}

impl Default for PronoteCfg {
    fn default() -> Self {
        Self { on: true, next: true, homework: true, grades: true, info: true, n_grade: true, n_hw: true, n_cancel: true, google: false, pai: Default::default() }
    }
}

/// Dynamic Island : barre en haut de l'écran qui s'ouvre au survol. Toutes ses options
/// (réglables dans « Réglages de l'île », voir ile_settings.rs où chaque option est décrite).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct IleCfg {
    /// Mode Dynamic Island actif (les widgets du bureau sont alors cachés).
    pub on: bool,
    // --- alarme ---
    pub alarm_on: bool,
    pub alarm_h: u32,
    pub alarm_m: u32,
    /// 0 tous les jours, 1 lundi-vendredi, 2 week-end
    pub alarm_days: usize,
    // --- disposition « Cartes » (par défaut) ---
    /// 0 cartes + onglets, 1 colonnes (ancienne disposition)
    pub layout: usize,
    /// carte de gauche : 0 Jarvis, 1 devoirs, 2 mails, 3 météo
    pub left_card: usize,
    /// dégradés des cartes (voir island_cards::GRADS)
    pub left_grad: usize,
    pub right_grad: usize,
    /// pochette de la musique floutée
    pub art_blur: bool,
    /// Pomodoro (minutes)
    pub pomo_work: f32,
    pub pomo_break: f32,
    /// onglets visibles et leur ordre
    pub t_jarvis: bool,
    pub t_music: bool,
    pub t_discord: bool,
    pub t_timers: bool,
    pub t_dnd: bool,
    pub t_notifs: bool,
    pub t_clips: bool,
    pub t_school: bool,
    pub t_calendar: bool,
    pub t_mails: bool,
    pub ot_jarvis: f32,
    pub ot_music: f32,
    pub ot_discord: f32,
    pub ot_timers: f32,
    pub ot_dnd: f32,
    pub ot_notifs: f32,
    pub ot_clips: f32,
    pub ot_school: f32,
    pub ot_calendar: f32,
    pub ot_mails: f32,
    pub t_phone: bool,
    pub ot_phone: f32,
    pub t_tasks: bool,
    pub ot_tasks: f32,
    pub t_reminders: bool,
    pub ot_reminders: f32,
    pub t_habits: bool,
    pub ot_habits: f32,
    pub t_routines: bool,
    pub ot_routines: f32,
    pub t_memory: bool,
    pub ot_memory: f32,
    pub t_pc: bool,
    pub ot_pc: f32,
    pub t_batteries: bool,
    pub ot_batteries: f32,
    pub t_jsettings: bool,
    pub ot_jsettings: f32,
    /// Mode de l'île : 0 normale, 1 grande (dépliée)
    pub grande: usize,
    /// Cacher les onglets sans contenu (Discord sans message, téléphone jamais relié…).
    pub hide_empty_tabs: bool,
    /// Version des onglets par défaut (migration des anciens réglages).
    pub tabs_v: u32,
    // --- apparence ---
    /// thème prédéfini (voir island_draw::PRESETS)
    pub preset: usize,
    /// 0 = couleur du thème, sinon couleur de la liste ACCENTS
    pub accent: usize,
    pub opacity: f32,
    pub blur: bool,
    pub blur_amount: f32,
    pub glow: bool,
    pub glow_strength: f32,
    pub border: bool,
    pub shadow: bool,
    pub gradient: bool,
    pub glass_cards: bool,
    pub separators: bool,
    pub fade: bool,
    pub radius: f32,
    // --- taille et position ---
    pub size: f32,
    /// 0 centre, 1 gauche, 2 droite
    pub position: usize,
    pub top_margin: f32,
    pub pill_width: f32,
    pub pill_height: f32,
    pub hover_zone: f32,
    pub hide_fullscreen: bool,
    /// Se cacher aussi quand la fenêtre au premier plan est agrandie (Chrome en grand…).
    pub hide_maximized: bool,
    /// Cacher l'île automatiquement quand la souris ne passe pas dessus.
    pub auto_hide: bool,
    /// Cacher l'île après N s sans survol (0 = jamais).
    pub auto_hide_secs: f32,
    /// Temps (s) avec la souris collée en haut de l'écran pour la faire revenir.
    pub reveal_secs: f32,
    /// Animation de sortie vers le haut.
    pub hide_anim: bool,
    pub always_open: bool,
    pub open_on_click: bool,
    // --- texte ---
    pub font: usize,
    pub text_size: f32,
    pub bold: bool,
    pub caps_headers: bool,
    pub pirate: bool,
    // --- heure et date ---
    pub h24: bool,
    pub seconds: bool,
    /// 0 « Samedi 14 novembre », 1 « 14/11/2026 », 2 « sam. 14 nov. », 3 « 14 novembre 2026 »
    pub date_fmt: usize,
    pub week_num: bool,
    pub clock_size: f32,
    /// 0 fin, 1 normal, 2 gras
    pub clock_weight: usize,
    /// 0 blanc, 1 accent, 2 arc-en-ciel, 3 selon la température
    pub clock_color: usize,
    pub pill_date: bool,
    pub day_progress: bool,
    pub greeting: bool,
    // --- météo ---
    pub fahrenheit: bool,
    /// 0 km/h, 1 m/s, 2 mph
    pub wind_unit: usize,
    pub weather_anim: bool,
    pub weather_bg: bool,
    pub weather_phrase: bool,
    pub show_feels: bool,
    pub show_minmax: bool,
    pub pill_weather: bool,
    pub forecast: bool,
    // --- modules (affichés et ordre) ---
    pub m_time: bool,
    pub m_timers: bool,
    pub m_weather: bool,
    pub m_pronote: bool,
    pub m_mail: bool,
    pub m_discord: bool,
    pub m_ai: bool,
    pub o_time: f32,
    pub o_timers: f32,
    pub o_weather: f32,
    pub o_pronote: f32,
    pub o_mail: f32,
    pub o_discord: f32,
    pub o_ai: f32,
    // --- minuteurs ---
    pub show_chrono: bool,
    pub show_timers: bool,
    pub show_alarm: bool,
    /// 0 « 1/5/10 min », 1 « 5/15/25 min », 2 « 10/30/60 min »
    pub timer_presets: usize,
    pub pill_timer: bool,
    /// 0 bip, 1 ding, 2 alerte, 3 erreur, 4 silencieux
    pub ring_sound: usize,
    pub ring_minutes: f32,
    pub snooze_min: f32,
    // --- Pronote ---
    /// Afficher les notes Pronote sur l'île.
    pub show_grades: bool,
    pub hw_days: f32,
    pub hide_done: bool,
    pub grades_n: f32,
    pub show_next_class: bool,
    pub show_avg: bool,
    // --- mails ---
    pub mail_refresh: f32,
    pub imp_reply: bool,
    pub imp_gmail: bool,
    pub only_urgent: bool,
    pub mails_n: f32,
    pub show_snippet: bool,
    pub pill_mail_badge: bool,
    // --- Discord ---
    pub discord_n: f32,
    pub discord_title: bool,
    /// Discord : afficher aussi les messages des serveurs (sinon seulement les messages privés).
    pub discord_servers: bool,
    // --- IA ---
    /// 0 qwen3.5:4b, 1 modèle des réglages mails, 2 qwen2.5:7b, 3 llama3.2, 4 gemma3:4b, 5 mistral
    pub ai_model: usize,
    pub ai_history: f32,
    pub ai_context: bool,
    pub ai_actions: bool,
    /// 0 normal, 1 très court, 2 détaillé, 3 pirate, 4 majordome (Jarvis)
    pub ai_style: usize,
    pub ai_temp: f32,
    // --- notifications ---
    pub n_island: bool,
    pub n_toast: bool,
    pub n_secs: f32,
    /// Petit son doux pour les notifications de l'île (nouvelle clé : l'ancienne « n_sound » est ignorée).
    #[serde(rename = "son_notif_doux")]
    pub n_sound: bool,
    pub dnd: bool,
    pub ni_pronote: bool,
    pub ni_mail: bool,
    pub ni_discord: bool,
    pub ni_timer: bool,
    pub ni_jarvis: bool,
    pub ni_phone: bool,
    pub nt_phone: bool,
    pub nt_pronote: bool,
    pub nt_mail: bool,
    pub nt_discord: bool,
    pub nt_timer: bool,
    pub nt_jarvis: bool,
    // --- Jarvis / API locale ---
    pub api_on: bool,
    pub api_port: f32,
    pub jarvis_show: bool,
    /// Lancer Jarvis (assistant vocal) avec Givre s'il ne tourne pas.
    pub jarvis_autostart: bool,
    /// Chemin de Jarvis.exe (vide = recherche automatique à côté de Givre).
    pub jarvis_path: String,
    // --- animations ---
    pub anim_speed: f32,
    pub open_delay: f32,
    pub close_delay: f32,
    pub bounce: bool,
    pub breathe: bool,
    /// Petit son doux à l'ouverture (nouvelle clé : l'ancienne « open_sound » est ignorée).
    #[serde(rename = "son_ouverture_doux")]
    pub open_sound: bool,
    /// Petits sons de verre sur les boutons (idées 1 à 10), leur volume (0 à 1, plus bas après 21 h),
    /// et l'onde au clic + la lueur au survol.
    pub sons: bool,
    pub sons_volume: f32,
    pub effets: bool,
    // --- grande application (Paramètres › Grande appli, Couleurs et fond, Texte, Animations) ---
    /// Largeur et hauteur de la grande appli (1 = 1100 × 720), colonne de gauche (px), icônes seulement.
    pub g_largeur: f32,
    pub g_hauteur: f32,
    pub g_colonne: f32,
    pub g_reduite: bool,
    pub g_icones: bool,
    /// Hauteur d'une ligne de la colonne (px), page d'ouverture, heure en bas de la colonne.
    pub g_espace: f32,
    pub g_depart: usize,
    pub g_heure: bool,
    /// Sections cachées et ordre choisi (n° dans island_big::SECTIONS).
    pub g_cachees: Vec<u8>,
    pub g_ordre: Vec<u8>,
    /// Fond, transparence en plus (0 à 0,7), flou (comme l'île / toujours / jamais), arrondi (0 = comme l'île).
    pub g_fond: usize,
    pub g_transp: f32,
    pub g_flou: usize,
    pub g_arrondi: f32,
    pub g_bord: bool,
    /// Clair le jour, sombre la nuit (heures de début du jour et de la nuit).
    pub g_auto_theme: bool,
    pub g_jour: f32,
    pub g_nuit: f32,
    /// Une couleur par section, style de la section choisie, fond de la colonne, ligne de séparation.
    pub g_couleurs: bool,
    pub g_selection: usize,
    pub g_fond_colonne: bool,
    pub g_ligne: bool,
    /// Taille du texte de la grande appli, contraste fort, noms des sections en gras.
    pub g_texte: f32,
    pub g_contraste: bool,
    pub g_gras: bool,
    /// Animation au changement de page (aucune / fondu / glissement / zoom), sa durée (ms),
    /// animation à l'ouverture, section survolée qui glisse.
    pub g_anim_page: usize,
    pub g_anim_duree: f32,
    pub g_anim_ouverture: bool,
    pub g_survol: bool,
    // --- options rigolotes ---
    pub rainbow: bool,
    pub disco: bool,
    pub confetti: bool,
    pub confetti_min: f32,
    /// 0 aucun, 1 chat, 2 canard, 3 escargot
    pub pet: usize,
    pub compliments: bool,
    pub eyes: bool,
    pub snow: bool,
}

impl Default for IleCfg {
    fn default() -> Self {
        Self {
            on: false,
            alarm_on: false,
            alarm_h: 7,
            alarm_m: 0,
            alarm_days: 0,
            layout: 0,
            left_card: 0,
            left_grad: 0,
            right_grad: 1,
            art_blur: false,
            pomo_work: 25.0,
            pomo_break: 5.0,
            t_jarvis: true,
            t_music: true,
            t_discord: false,
            t_timers: true,
            t_dnd: false,
            t_notifs: false,
            t_clips: false,
            t_school: true,
            t_calendar: false,
            t_mails: true,
            ot_jarvis: 1.0,
            ot_music: 2.0,
            ot_discord: 3.0,
            ot_timers: 4.0,
            ot_dnd: 5.0,
            ot_notifs: 6.0,
            ot_clips: 7.0,
            ot_school: 8.0,
            ot_calendar: 9.0,
            ot_mails: 10.0,
            t_phone: false,
            ot_phone: 11.0,
            t_tasks: true,
            ot_tasks: 12.0,
            t_reminders: false,
            ot_reminders: 13.0,
            t_habits: false,
            ot_habits: 14.0,
            t_routines: false,
            ot_routines: 15.0,
            t_memory: false,
            ot_memory: 16.0,
            t_pc: false,
            ot_pc: 17.0,
            t_batteries: false,
            ot_batteries: 18.0,
            t_jsettings: false,
            ot_jsettings: 19.0,
            grande: 0,
            hide_empty_tabs: true,
            tabs_v: 2,
            preset: 1,
            accent: 0,
            opacity: 0.78,
            blur: true,
            blur_amount: 0.7,
            glow: true,
            glow_strength: 0.5,
            border: true,
            shadow: true,
            gradient: true,
            glass_cards: true,
            separators: false,
            fade: true,
            radius: 30.0,
            size: 1.0,
            position: 0,
            top_margin: 6.0,
            pill_width: 270.0,
            pill_height: 36.0,
            hover_zone: 20.0,
            hide_fullscreen: true,
            hide_maximized: true,
            auto_hide: true,
            auto_hide_secs: 30.0,
            reveal_secs: 0.5,
            hide_anim: true,
            always_open: false,
            open_on_click: false,
            font: 0,
            text_size: 1.0,
            bold: false,
            caps_headers: true,
            pirate: false,
            h24: true,
            seconds: false,
            date_fmt: 0,
            week_num: true,
            clock_size: 54.0,
            clock_weight: 1,
            clock_color: 0,
            pill_date: false,
            day_progress: true,
            greeting: true,
            fahrenheit: false,
            wind_unit: 0,
            weather_anim: true,
            weather_bg: true,
            weather_phrase: true,
            show_feels: true,
            show_minmax: true,
            pill_weather: true,
            forecast: true,
            m_time: true,
            m_timers: true,
            m_weather: true,
            m_pronote: true,
            m_mail: true,
            m_discord: true,
            m_ai: true,
            o_time: 1.0,
            o_timers: 2.0,
            o_weather: 3.0,
            o_pronote: 4.0,
            o_mail: 5.0,
            o_discord: 6.0,
            o_ai: 7.0,
            show_chrono: true,
            show_timers: true,
            show_alarm: true,
            timer_presets: 0,
            pill_timer: true,
            ring_sound: 2,
            ring_minutes: 5.0,
            snooze_min: 5.0,
            show_grades: true,
            hw_days: 7.0,
            hide_done: true,
            grades_n: 3.0,
            show_next_class: true,
            show_avg: true,
            mail_refresh: 2.0,
            imp_reply: true,
            imp_gmail: false,
            only_urgent: false,
            mails_n: 4.0,
            show_snippet: true,
            pill_mail_badge: true,
            discord_n: 4.0,
            discord_title: true,
            discord_servers: false,
            ai_model: 0,
            ai_history: 3.0,
            ai_context: true,
            ai_actions: true,
            ai_style: 0,
            ai_temp: 0.4,
            n_island: true,
            n_toast: false,
            n_secs: 6.0,
            n_sound: false,
            dnd: false,
            ni_pronote: true,
            ni_mail: true,
            ni_discord: true,
            ni_timer: true,
            ni_jarvis: true,
            ni_phone: true,
            nt_phone: false,
            nt_pronote: true,
            nt_mail: true,
            nt_discord: false,
            nt_timer: true,
            nt_jarvis: false,
            api_on: true,
            api_port: 47802.0,
            jarvis_show: true,
            jarvis_autostart: true,
            jarvis_path: String::new(),
            anim_speed: 1.0,
            open_delay: 60.0,
            close_delay: 400.0,
            bounce: true,
            breathe: false,
            open_sound: false,
            sons: true,
            sons_volume: 0.5,
            effets: true,
            g_largeur: 1.0,
            g_hauteur: 1.0,
            g_colonne: 212.0,
            g_reduite: false,
            g_icones: true,
            g_espace: 31.0,
            g_depart: 0,
            g_heure: false,
            g_cachees: Vec::new(),
            g_ordre: Vec::new(),
            g_fond: 0,
            g_transp: 0.0,
            g_flou: 0,
            g_arrondi: 0.0,
            g_bord: false,
            g_auto_theme: false,
            g_jour: 7.0,
            g_nuit: 20.0,
            g_couleurs: false,
            g_selection: 0,
            g_fond_colonne: false,
            g_ligne: true,
            g_texte: 1.0,
            g_contraste: false,
            g_gras: true,
            g_anim_page: 0,
            g_anim_duree: 250.0,
            g_anim_ouverture: false,
            g_survol: false,
            rainbow: false,
            disco: false,
            confetti: true,
            confetti_min: 15.0,
            pet: 0,
            compliments: true,
            eyes: false,
            snow: false,
        }
    }
}

/// Brouillon du formulaire « Nouvel événement » (non enregistré).
#[derive(Clone, Default)]
pub struct Draft {
    pub title: String,
    pub date: String,
    pub time: String,
    pub dur: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    /// Mode de l'IA : 0 auto (ultra performant s'il y a une clé NVIDIA), 1 ultra performant, 2 ultra léger.
    pub ia_mode: u8,
    /// Profil matériel choisi à la main (voir profils.rs) ; vide = celui trouvé par la détection.
    pub profil: String,
    pub mail: MailCfg,
    pub music: bool,
    pub discord: DiscordCfg,
    pub printer: PrinterCfg,
    pub pronote: PronoteCfg,
    pub ile: IleCfg,
    #[serde(skip)]
    pub draft: Draft,
    /// QR code Pronote lu dans le presse-papiers et code à 4 chiffres (jamais enregistrés).
    #[serde(skip)]
    pub pn_qr: String,
    #[serde(skip)]
    pub pn_pin: String,
    /// 0 sombre, 1 clair, 2 comme Windows
    pub theme: usize,
    /// 0 fins, 1 normaux, 2 gras
    pub clock_weight: usize,
    pub scale: f32,
    pub blur: bool,
    pub blur_amount: f32,
    pub opacity: f32,
    pub border: bool,
    pub shadow: f32,
    pub radius: f32,
    pub continuous: bool,
    pub accent: [u8; 3],
    pub accent_auto: bool,
    pub hide_when_windows: bool,
    #[serde(skip)]
    pub autostart: bool,
    pub grid: f32,
    pub locked: bool,
    pub clock: ClockCfg,
    pub weather: WeatherCfg,
    pub calendar: CalCfg,
    pub countdown: CountCfg,
    pub places: BTreeMap<String, Place>,
    /// Widgets du bureau (l'ancien Givre) affichés en plus de l'île.
    pub widgets: bool,
    /// Les widgets de Givre ont déjà été repris une fois (appli unique).
    pub givre_merged: bool,
    /// Pouvoirs de Jarvis sur le PC (fichiers, PowerShell, programmes, extinction).
    pub pouvoirs: crate::pouvoirs::PouvoirsCfg,
}

/// Réglages des widgets repris de %APPDATA%\Givre\config.json au passage à l'appli unique.
const WIDGET_KEYS: [&str; 20] = [
    "theme", "clock_weight", "scale", "blur", "blur_amount", "opacity", "border", "shadow", "radius", "continuous",
    "accent", "accent_auto", "hide_when_windows", "grid", "locked", "clock", "weather", "calendar", "countdown", "places",
];

/// Une seule fois : reprend la disposition et le style des widgets de Givre (Givre n'est plus lancé à part).
fn merge_givre(c: Config) -> Config {
    let Some(appdata) = std::env::var_os("APPDATA") else { return c };
    let file = PathBuf::from(appdata).join("Givre").join("config.json");
    let Some(old) = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) else {
        return Config { givre_merged: true, ..c };
    };
    let Ok(mut v) = serde_json::to_value(&c) else { return c };
    for k in WIDGET_KEYS {
        if let Some(x) = old.get(k) {
            v[k] = x.clone();
        }
    }
    v["givre_merged"] = serde_json::Value::Bool(true);
    match serde_json::from_value::<Config>(v) {
        Ok(mut n) => {
            n.draft = c.draft;
            crate::sys::log("widgets de Givre repris dans Jarvis (appli unique)");
            n
        }
        Err(_) => Config { givre_merged: true, ..c },
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ia_mode: 0,
            profil: String::new(),
            mail: MailCfg::default(),
            music: true,
            discord: DiscordCfg::default(),
            printer: PrinterCfg::default(),
            pronote: PronoteCfg::default(),
            ile: IleCfg::default(),
            draft: Draft { dur: "60".into(), ..Default::default() },
            pn_qr: String::new(),
            pn_pin: String::new(),
            theme: 0,
            clock_weight: 0,
            scale: 1.0,
            blur: true,
            blur_amount: 0.6,
            opacity: 0.4,
            border: true,
            shadow: 0.6,
            radius: 22.0,
            continuous: true,
            accent: [52, 199, 89],
            accent_auto: false,
            hide_when_windows: true,
            autostart: false,
            grid: 8.0,
            locked: false,
            clock: ClockCfg::default(),
            weather: WeatherCfg::default(),
            calendar: CalCfg::default(),
            countdown: CountCfg::default(),
            places: BTreeMap::new(),
            widgets: true,
            givre_merged: false,
            pouvoirs: crate::pouvoirs::PouvoirsCfg::default(),
        }
    }
}

/// Profil de test (variable GIVRE_PROFIL = dossier) : réglages, comptes et journal à part,
/// et Givre peut tourner à côté de l'instance normale. Vide = profil normal.
pub fn test_profile() -> Option<PathBuf> {
    std::env::var_os("GIVRE_PROFIL").filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn dir() -> PathBuf {
    if let Some(p) = test_profile() {
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    let d = base.join("Jarvis").join("barre");
    if !d.exists() {
        // premier lancement : on reprend les réglages et les connexions de Givre (Pronote, Google…)
        let _ = std::fs::create_dir_all(&d);
        let old = base.join("Givre");
        for name in ["config.json", "comptes.bin", "pronote.bin", "google.json"] {
            if std::fs::copy(old.join(name), d.join(name)).is_ok() {
                crate::sys::log(&format!("repris de Givre : {name}"));
            }
        }
    }
    d
}

/// Fichiers écrits par un programme lancé depuis l'appli Claude (paquet Windows) :
/// Windows les range dans %LOCALAPPDATA%\\Packages\\Claude_*\\LocalCache\\Roaming\\Givre
/// et le vrai Givre ne les voit pas. On reprend ceux qui manquent ou qui sont plus récents.
pub fn import_hidden() {
    // la barre de Jarvis ne reprend rien des dossiers cachés de Givre
    if true {
        return;
    }
    let Some(la) = std::env::var_os("LOCALAPPDATA") else { return };
    let Ok(rd) = std::fs::read_dir(PathBuf::from(la).join("Packages")) else { return };
    let real = dir();
    for e in rd.flatten() {
        if !e.file_name().to_string_lossy().starts_with("Claude_") {
            continue;
        }
        let hidden = e.path().join("LocalCache").join("Roaming").join("Givre");
        for name in ["google.json", "comptes.bin", "pronote.bin", "config.json"] {
            let (src, dst) = (hidden.join(name), real.join(name));
            let Some(ts) = std::fs::metadata(&src).ok().and_then(|m| m.modified().ok()) else { continue };
            let newer = match std::fs::metadata(&dst).ok().and_then(|m| m.modified().ok()) {
                Some(td) => ts > td,
                None => true,
            };
            if newer && std::fs::copy(&src, &dst).is_ok() {
                crate::sys::log(&format!("fichier repris du dossier de Claude : {name}"));
            }
        }
    }
}

pub fn exists() -> bool {
    dir().join("config.json").exists()
}

pub fn load() -> Config {
    let mut c: Config = std::fs::read_to_string(dir().join("config.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    // anciens réglages (tous les onglets visibles) : on passe aux onglets utiles par défaut, une seule fois
    if c.ile.tabs_v < 2 {
        let i = &mut c.ile;
        for t in [&mut i.t_discord, &mut i.t_dnd, &mut i.t_notifs, &mut i.t_clips, &mut i.t_calendar, &mut i.t_phone, &mut i.t_reminders, &mut i.t_habits, &mut i.t_routines, &mut i.t_memory, &mut i.t_pc, &mut i.t_batteries, &mut i.t_jsettings] {
            *t = false;
        }
        i.hide_empty_tabs = true;
        i.tabs_v = 2;
    }
    // Jarvis n'affiche plus de widgets sur le bureau : ils sont dans Givre, une appli à part.
    c.widgets = false;
    if false {
        c = merge_givre(c);
        save(&c);
    }
    c
}

pub fn save(c: &Config) {
    if let Ok(s) = serde_json::to_string_pretty(c) {
        let p = dir().join("config.json");
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}
