//! Données en ligne (météo Open-Meteo, vacances scolaires) récupérées sur un fil à part.

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime};
use serde::Deserialize;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use crate::accounts;

pub const WM_APP_NET: u32 = 0x8000 + 2;

pub struct Weather {
    pub temp: f32,
    pub feels: f32,
    pub code: i32,
    pub day: bool,
    pub wind: f32,
    pub humidity: f32,
    pub hours: Vec<Hour>,
    pub days: Vec<Day>,
    pub sunrise: String,
    pub sunset: String,
    pub rain: Option<String>,
}

pub struct Hour {
    pub label: String,
    pub code: i32,
    pub day: bool,
    pub temp: f32,
}

pub struct Day {
    pub date: NaiveDate,
    pub code: i32,
    pub min: f32,
    pub max: f32,
}

#[derive(Clone, Debug)]
pub struct Holiday {
    pub name: String,
    pub start: NaiveDate,
    /// Jour de la reprise (exclu).
    pub end: NaiveDate,
}

/// Événement d'agenda (Google, Pronote… aux prochaines étapes).
#[derive(Clone)]
#[allow(dead_code)]
pub struct Event {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub all_day: bool,
    pub title: String,
    pub color: [u8; 3],
}

/// Catégorie d'un mail donnée par le tri (IA ou règles simples).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Cat {
    #[default]
    None,
    Important,
    Reply,
    Notif,
    Promo,
    Spam,
}

impl Cat {
    pub fn label(self) -> &'static str {
        match self {
            Cat::None => "",
            Cat::Important => "Important",
            Cat::Reply => "À répondre",
            Cat::Notif => "Notification",
            Cat::Promo => "Promo",
            Cat::Spam => "Spam",
        }
    }
}

#[derive(Clone)]
pub struct Mail {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub date: NaiveDateTime,
    pub important: bool,
    pub urgent: bool,
    pub cat: Cat,
    /// Adresse web qui ouvre ce mail (Gmail, ou Outlook.com).
    pub link: String,
    /// Mail Hotmail transféré : texte à chercher dans Outlook.com (vide sinon).
    pub find: String,
}

#[derive(Default)]
#[allow(dead_code)]
pub struct MailBox {
    pub mails: Vec<Mail>,
    pub summary: String,
    pub ai: bool,
    pub accounts: usize,
}

pub struct Media {
    pub title: String,
    pub artist: String,
    pub app: String,
    pub playing: bool,
    pub art: Option<tiny_skia::Pixmap>,
}

pub struct Notif {
    pub title: String,
    pub body: String,
    pub time: NaiveDateTime,
}

#[derive(Clone, Default, PartialEq)]
pub struct Printer {
    pub online: bool,
    pub status: i64,
    pub progress: i64,
    pub layer: i64,
    pub total: i64,
    pub left_min: i64,
    pub file: String,
}

#[derive(Clone, Default)]
pub struct AccountsInfo {
    pub google: Vec<String>,
    pub ms: Option<String>,
    pub google_json: bool,
    pub ai: String,
}

#[derive(Default)]
pub struct Data {
    pub weather: Option<Weather>,
    pub holidays: Vec<Holiday>,
    pub events: Vec<Event>,
    pub mail: MailBox,
    pub media: Option<Media>,
    pub discord: Vec<Notif>,
    pub discord_state: String,
    pub winnotif_state: String,
    pub printer: Printer,
    pub accounts: AccountsInfo,
    pub pronote: crate::pronote::School,
}

pub enum Job {
    Weather(f64, f64),
    Geocode(String),
    Holidays(String),
    Accounts(String, String),
    Sync(accounts::SyncCfg),
    GoogleAdd,
    GoogleDel(usize),
    MsAdd(String),
    MsDel,
    CreateEvent(String, NaiveDate, Option<chrono::NaiveTime>, i64),
}

pub enum Msg {
    Weather(Result<Weather, String>),
    Geo(Result<(String, f64, f64), String>),
    Holidays(Result<Vec<Holiday>, String>),
    Events(Vec<Event>),
    Mail(MailBox),
    Accounts(AccountsInfo),
    /// Un compte a changé : il faut resynchroniser.
    Changed,
    Status(String),
    /// Titre, texte, lien et recherche Outlook ouverts si on clique sur la notification.
    Notify(String, String, String, String),
    Media(Option<Media>),
    Discord(Notif),
    DiscordState(String),
    WinNotifState(String),
    Printer(Printer),
    School(crate::pronote::School),
    /// Notification Pronote : 0 note, 1 devoir, 2 cours annulé.
    PNotify(u8, String, String),
}

/// Envoie un message à la fenêtre principale et la réveille.
#[derive(Clone)]
pub struct Out {
    tx: Sender<Msg>,
    main: isize,
}

impl Out {
    pub fn send(&self, m: Msg) {
        if self.tx.send(m).is_ok() {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(self.main as _, WM_APP_NET, 0, 0);
            }
        }
    }
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Givre/0.1 (widgets de bureau)")
        .build()
}

/// Fil réseau : attend des tâches, renvoie les résultats et réveille la fenêtre principale.
pub fn spawn(main: isize, vault: Arc<Mutex<accounts::Secrets>>) -> (Sender<Job>, Receiver<Msg>, Out) {
    let (jt, jr) = channel::<Job>();
    let (mt, mr) = channel::<Msg>();
    let out = Out { tx: mt, main };
    let o = out.clone();
    let _ = std::thread::Builder::new().name("reseau".into()).spawn(move || {
        let ag = agent();
        let mut st = accounts::State::default();
        for job in jr {
            match job {
                Job::Weather(la, lo) => o.send(Msg::Weather(fetch_weather(&ag, la, lo))),
                Job::Geocode(n) => o.send(Msg::Geo(geocode(&ag, &n))),
                Job::Holidays(a) => o.send(Msg::Holidays(fetch_holidays(&ag, &a))),
                Job::Accounts(model, url) => o.send(Msg::Accounts(accounts::info(&vault, &model, &url))),
                Job::Sync(c) => accounts::sync(&ag, &vault, &mut st, &c, &o),
                Job::GoogleAdd => {
                    let (v, o2) = (vault.clone(), o.clone());
                    std::thread::spawn(move || {
                        o2.send(Msg::Status("Connexion Google : continue dans ton navigateur…".into()));
                        match accounts::google_add(&agent(), &v) {
                            Ok(e) => o2.send(Msg::Status(format!("Compte Google ajouté : {e}"))),
                            Err(e) => o2.send(Msg::Status(e)),
                        }
                        o2.send(Msg::Changed);
                    });
                }
                Job::MsAdd(client) => {
                    let (v, o2) = (vault.clone(), o.clone());
                    std::thread::spawn(move || {
                        match accounts::ms_add(&agent(), &client, &v, &o2) {
                            Ok(e) => o2.send(Msg::Status(format!("Hotmail ajouté : {e}"))),
                            Err(e) => o2.send(Msg::Status(e)),
                        }
                        o2.send(Msg::Changed);
                    });
                }
                Job::GoogleDel(i) => {
                    let mut s = vault.lock().unwrap();
                    if i < s.google.len() {
                        s.google.remove(i);
                        accounts::save(&s);
                    }
                    drop(s);
                    o.send(Msg::Events(Vec::new()));
                    o.send(Msg::Changed);
                }
                Job::MsDel => {
                    let mut s = vault.lock().unwrap();
                    s.ms = None;
                    accounts::save(&s);
                    drop(s);
                    o.send(Msg::Changed);
                }
                Job::CreateEvent(title, date, time, dur) => {
                    match accounts::google_create(&ag, &vault, &title, date, time, dur) {
                        Ok(()) => o.send(Msg::Status(format!("Événement « {title} » créé dans Google Agenda."))),
                        Err(e) => o.send(Msg::Status(e)),
                    }
                    o.send(Msg::Changed);
                }
            }
        }
    });
    (jt, mr, out)
}

pub fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn get<T: serde::de::DeserializeOwned>(ag: &ureq::Agent, url: &str) -> Result<T, String> {
    ag.get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_json::<T>()
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct Om {
    current: OmCur,
    hourly: OmHourly,
    daily: OmDaily,
    #[serde(default)]
    minutely_15: Option<OmMin>,
}

#[derive(Deserialize)]
struct OmCur {
    temperature_2m: f32,
    apparent_temperature: f32,
    weather_code: i32,
    is_day: i32,
    wind_speed_10m: f32,
    relative_humidity_2m: f32,
}

#[derive(Deserialize)]
struct OmHourly {
    time: Vec<String>,
    temperature_2m: Vec<Option<f32>>,
    weather_code: Vec<Option<i32>>,
    is_day: Vec<Option<i32>>,
}

#[derive(Deserialize)]
struct OmDaily {
    time: Vec<String>,
    weather_code: Vec<Option<i32>>,
    temperature_2m_max: Vec<Option<f32>>,
    temperature_2m_min: Vec<Option<f32>>,
    sunrise: Vec<String>,
    sunset: Vec<String>,
}

#[derive(Deserialize)]
struct OmMin {
    time: Vec<String>,
    precipitation: Vec<Option<f32>>,
}

pub fn fetch_weather(ag: &ureq::Agent, lat: f64, lon: f64) -> Result<Weather, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.4}&longitude={lon:.4}\
         &current=temperature_2m,apparent_temperature,weather_code,is_day,wind_speed_10m,relative_humidity_2m\
         &hourly=temperature_2m,weather_code,is_day&minutely_15=precipitation\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset\
         &timezone=auto&forecast_days=6&forecast_hours=7&forecast_minutely_15=8"
    );
    let r: Om = get(ag, &url)?;
    let h = &r.hourly;
    let hours = (0..h.time.len())
        .filter_map(|i| {
            let label = if i == 0 {
                "Maint.".to_string()
            } else {
                format!("{} h", h.time[i].get(11..13)?.parse::<u32>().ok()?)
            };
            Some(Hour {
                label,
                code: (*h.weather_code.get(i)?)?,
                day: h.is_day.get(i).copied().flatten().unwrap_or(1) == 1,
                temp: (*h.temperature_2m.get(i)?)?,
            })
        })
        .collect();
    let d = &r.daily;
    let days = (0..d.time.len())
        .filter_map(|i| {
            Some(Day {
                date: NaiveDate::parse_from_str(&d.time[i], "%Y-%m-%d").ok()?,
                code: (*d.weather_code.get(i)?)?,
                min: (*d.temperature_2m_min.get(i)?)?,
                max: (*d.temperature_2m_max.get(i)?)?,
            })
        })
        .collect();
    let hm = |v: &Vec<String>| v.first().and_then(|s| s.get(11..16)).unwrap_or("--:--").to_string();
    let c = r.current;
    Ok(Weather {
        temp: c.temperature_2m,
        feels: c.apparent_temperature,
        code: c.weather_code,
        day: c.is_day == 1,
        wind: c.wind_speed_10m,
        humidity: c.relative_humidity_2m,
        hours,
        days,
        sunrise: hm(&d.sunrise),
        sunset: hm(&d.sunset),
        rain: r.minutely_15.as_ref().and_then(rain_msg),
    })
}

/// « Pluie dans 20 min » / « Pluie jusqu'à 19:45 » à partir des prévisions au quart d'heure.
fn rain_msg(m: &OmMin) -> Option<String> {
    let now = Local::now().naive_local();
    let pts: Vec<(NaiveDateTime, f32)> = m
        .time
        .iter()
        .zip(&m.precipitation)
        .filter_map(|(t, p)| Some((NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M").ok()?, p.unwrap_or(0.0))))
        .collect();
    let wet = |p: f32| p >= 0.1;
    let first = pts.first()?;
    if wet(first.1) {
        return Some(match pts.iter().find(|(_, p)| !wet(*p)) {
            Some((t, _)) => format!("Pluie jusqu'à {}", t.format("%H:%M")),
            None => "Pluie pendant les 2 prochaines heures".into(),
        });
    }
    pts.iter().find(|(_, p)| wet(*p)).map(|(t, _)| {
        let mins = (*t - now).num_minutes();
        if mins < 8 {
            "Pluie imminente".to_string()
        } else {
            format!("Pluie dans {} min", (mins + 2) / 5 * 5)
        }
    })
}

pub fn geocode(ag: &ureq::Agent, name: &str) -> Result<(String, f64, f64), String> {
    #[derive(Deserialize)]
    struct R {
        results: Option<Vec<G>>,
    }
    #[derive(Deserialize)]
    struct G {
        name: String,
        latitude: f64,
        longitude: f64,
    }
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=fr&format=json",
        enc(name)
    );
    let r: R = get(ag, &url)?;
    r.results
        .and_then(|v| v.into_iter().next())
        .map(|g| (g.name, g.latitude, g.longitude))
        .ok_or_else(|| format!("Ville « {name} » introuvable"))
}

fn local_date(s: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Local).date_naive())
        .or_else(|| NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok())
}

pub fn fetch_holidays(ag: &ureq::Agent, acad: &str) -> Result<Vec<Holiday>, String> {
    #[derive(Deserialize)]
    struct R {
        results: Vec<H>,
    }
    #[derive(Deserialize)]
    struct H {
        description: String,
        start_date: String,
        end_date: String,
        population: Option<String>,
    }
    let since = (Local::now().date_naive() - Duration::days(150)).format("%Y-%m-%d");
    let wh = format!("location=\"{}\" and end_date>=\"{}\"", acad.trim(), since);
    let url = format!(
        "https://data.education.gouv.fr/api/explore/v2.1/catalog/datasets/fr-en-calendrier-scolaire/records\
         ?where={}&order_by=start_date&limit=40&select=description,start_date,end_date,population",
        enc(&wh)
    );
    let r: R = get(ag, &url)?;
    let mut v: Vec<Holiday> = Vec::new();
    for h in r.results {
        if h.population.as_deref() == Some("Enseignants") {
            continue;
        }
        let (Some(s), Some(e)) = (local_date(&h.start_date), local_date(&h.end_date)) else { continue };
        let e = if e <= s { s + Duration::days(1) } else { e };
        if v.iter().any(|x| x.name == h.description && x.start == s) {
            continue;
        }
        v.push(Holiday { name: h.description, start: s, end: e });
    }
    if v.is_empty() {
        return Err(format!("Aucune période de vacances trouvée pour l'académie « {acad} »"));
    }
    v.sort_by_key(|h| h.start);
    Ok(v)
}

pub fn wmo_label(code: i32, day: bool) -> &'static str {
    match code {
        0 => if day { "Ensoleillé" } else { "Dégagé" },
        1 => if day { "Plutôt ensoleillé" } else { "Plutôt dégagé" },
        2 => "Éclaircies",
        3 => "Couvert",
        45 | 48 => "Brouillard",
        51 | 53 | 55 => "Bruine",
        56 | 57 => "Bruine verglaçante",
        61 => "Pluie faible",
        63 => "Pluie",
        65 => "Forte pluie",
        66 | 67 => "Pluie verglaçante",
        71 => "Neige faible",
        73 => "Neige",
        75 => "Forte neige",
        77 => "Grésil",
        80 | 81 => "Averses",
        82 => "Fortes averses",
        85 | 86 => "Averses de neige",
        95 => "Orage",
        96 | 99 => "Orage et grêle",
        _ => "—",
    }
}
