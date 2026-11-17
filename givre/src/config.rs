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
    pub model: String,
    /// Adresse d'Ollama (vide = celle par défaut, sur ce PC).
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
    pub next: bool,
    pub homework: bool,
    pub grades: bool,
    pub info: bool,
    pub n_grade: bool,
    pub n_hw: bool,
    pub n_cancel: bool,
    pub google: bool,
}

impl Default for PronoteCfg {
    fn default() -> Self {
        Self { next: true, homework: true, grades: true, info: true, n_grade: true, n_hw: true, n_cancel: true, google: false }
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
    pub mail: MailCfg,
    pub music: bool,
    pub discord: DiscordCfg,
    pub printer: PrinterCfg,
    pub pronote: PronoteCfg,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mail: MailCfg::default(),
            music: true,
            discord: DiscordCfg::default(),
            printer: PrinterCfg::default(),
            pronote: PronoteCfg::default(),
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
    let d = base.join("Givre");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Fichiers écrits par un programme lancé depuis l'appli Claude (paquet Windows) :
/// Windows les range dans %LOCALAPPDATA%\\Packages\\Claude_*\\LocalCache\\Roaming\\Givre
/// et le vrai Givre ne les voit pas. On reprend ceux qui manquent ou qui sont plus récents.
pub fn import_hidden() {
    if test_profile().is_some() {
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
    std::fs::read_to_string(dir().join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
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
