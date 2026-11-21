//! Pronote : connexion par QR code (comme l'appli mobile), emploi du temps, devoirs, notes,
//! annonces et messagerie. Protocole repris de Pawnote 1.6.3 (correctifs « Pronote 2026 ») et
//! vérifié sur le code JavaScript servi par Pronote 2026.2.8. Aucun mot de passe n'est gardé :
//! seulement le jeton de l'appli mobile, renouvelé à chaque connexion, dans le coffre chiffré.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDur, Instant};

use aes::Aes128;
use base64::Engine;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, Timelike};
use md5::Md5;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::accounts::{self, Secrets};
use crate::data::{Msg, Out, enc};
use crate::sys;

const UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15 \
(KHTML, like Gecko) Mobile/15E148 PRONOTE Mobile APP Version/2.0.11";
/// Préfixe des erreurs « identifiant / jeton refusé » (on retente alors l'autre variante de calcul).
const BAD: &str = "Pronote a refusé la connexion";
const REDO: &str = "Pronote : connexion à refaire (QR code)";
const SUSPENDED: &str = "Pronote a bloqué les essais pour un moment (trop d'échecs). Attends environ une heure, puis refais un QR code tout neuf.";

/// Refus de Pronote (et pas une simple coupure réseau) : on arrête les essais automatiques.
pub fn is_refusal(e: &str) -> bool {
    let l = e.to_lowercase();
    e.starts_with(BAD) || ["refus", "jeton", "erreur du serveur", "suspend", "bloqu", "adresse ip", "désactivé"].iter().any(|k| l.contains(k))
}

/// Message à afficher pour un refus (spécial si Pronote parle de suspension ou de blocage).
pub fn refusal_msg(e: &str) -> String {
    let l = e.to_lowercase();
    if ["suspend", "bloqu", "adresse ip"].iter().any(|k| l.contains(k)) { SUSPENDED.to_string() } else { REDO.to_string() }
}
const GCAL: &str = "Pronote (Givre)";

// ---------- Données affichées ----------

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Lesson {
    pub id: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub subject: String,
    pub teacher: String,
    pub room: String,
    pub status: String,
    pub canceled: bool,
    pub color: [u8; 3],
    /// PAI : pourquoi je ne vais pas à ce cours (vide = j'y vais). Rempli par `pai::appliquer`.
    #[serde(default)]
    pub manque: String,
    /// Créneau ajouté par moi dans mon emploi du temps (pas un cours de Pronote).
    #[serde(default)]
    pub perso: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Alert {
    None,
    Canceled,
    Absent,
    Room,
    Info,
    /// PAI : l'élève ne va pas à ce cours.
    Skip,
}

impl Lesson {
    pub fn alert(&self) -> Alert {
        if !self.manque.is_empty() {
            return Alert::Skip;
        }
        self.alert_pronote()
    }
    fn alert_pronote(&self) -> Alert {
        let s = self.status.to_lowercase();
        if s.contains("absent") {
            Alert::Absent
        } else if self.canceled || s.contains("annul") {
            Alert::Canceled
        } else if s.contains("salle") || s.contains("déplac") {
            Alert::Room
        } else if !s.trim().is_empty() {
            Alert::Info
        } else {
            Alert::None
        }
    }
    pub fn alert_label(&self) -> String {
        match self.alert() {
            Alert::Canceled => "ANNULÉ".into(),
            Alert::Absent => "PROF ABSENT".into(),
            Alert::Room => "CHANGEMENT DE SALLE".into(),
            Alert::Info => self.status.to_uppercase(),
            Alert::Skip => "TU N'Y VAS PAS".into(),
            Alert::None => String::new(),
        }
    }
    pub fn bad(&self) -> bool {
        matches!(self.alert_pronote(), Alert::Canceled | Alert::Absent)
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Homework {
    pub id: String,
    pub subject: String,
    pub text: String,
    pub due: NaiveDate,
    pub done: bool,
    pub color: [u8; 3],
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Grade {
    pub id: String,
    pub subject: String,
    pub value: String,
    pub out_of: String,
    pub date: NaiveDate,
    pub class_avg: String,
    pub coef: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Avg {
    pub subject: String,
    pub student: String,
    pub class: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct News {
    pub id: String,
    pub title: String,
    pub author: String,
    pub date: NaiveDate,
    pub text: String,
    pub read: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Talk {
    pub subject: String,
    pub from: String,
    pub when: String,
    pub unread: i64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct School {
    pub connected: bool,
    pub name: String,
    pub class: String,
    pub base: String,
    pub lessons: Vec<Lesson>,
    pub homework: Vec<Homework>,
    pub grades: Vec<Grade>,
    pub averages: Vec<Avg>,
    pub overall: String,
    pub class_overall: String,
    pub period: String,
    pub news: Vec<News>,
    pub talks: Vec<Talk>,
    pub updated: Option<NaiveDateTime>,
    pub error: String,
}

impl School {
    /// Cours en cours ou prochain cours, et celui d'après. Si deux cours se superposent
    /// (cours annulé + remplacement), on garde celui qui a lieu.
    pub fn next(&self, now: NaiveDateTime) -> (Option<&Lesson>, Option<&Lesson>) {
        // PAI : les cours où je ne vais pas ne sont jamais « le prochain cours ».
        let mut v: Vec<&Lesson> = self.lessons.iter().filter(|l| l.end > now && l.manque.is_empty()).collect();
        v.sort_by_key(|l| (l.start, l.bad()));
        v.dedup_by(|b, a| a.start == b.start);
        (v.first().copied(), v.get(1).copied())
    }
    pub fn lessons_on(&self, d: NaiveDate) -> Vec<&Lesson> {
        let mut v: Vec<&Lesson> = self.lessons.iter().filter(|l| l.start.date() == d).collect();
        v.sort_by_key(|l| (l.start, l.bad()));
        v
    }
    pub fn unread(&self) -> i64 {
        self.talks.iter().map(|t| t.unread).sum()
    }
}

/// « 15,5 », « Abs », « Disp »… à partir de la valeur Pronote (« 15,5 » ou « |1 »).
pub fn grade_txt(v: &str) -> String {
    match v.split_once('|') {
        Some((_, k)) => match k.trim() {
            "1" => "Abs".into(),
            "2" => "Disp".into(),
            "3" => "N.Not".into(),
            "4" => "Inap".into(),
            "5" => "N.Rdu".into(),
            "6" => "Abs 0".into(),
            "7" => "N.Rdu 0".into(),
            "8" => "Félicit.".into(),
            _ => "—".into(),
        },
        None => v.trim().to_string(),
    }
}

pub fn grade_num(v: &str) -> Option<f32> {
    if v.contains('|') {
        return None;
    }
    v.trim().replace(',', ".").parse().ok()
}

// ---------- Compte (dans le coffre chiffré) ----------

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct PAcct {
    pub base: String,
    pub kind: i64,
    pub login: String,
    pub token: String,
    pub uuid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub class: String,
    /// 0 : calcul officiel de la clé ; 1 : variante Pawnote (si la première est refusée).
    #[serde(default)]
    pub variant: u8,
    /// Refus de Pronote (jeton refusé, serveur, adresse IP bloquée) : plus aucun essai automatique
    /// tant qu'une nouvelle connexion par QR code n'a pas réussi. Enregistré avec le compte.
    #[serde(default)]
    pub blocked: String,
    #[serde(default)]
    pub gcal: String,
    /// Événements déjà copiés dans Google : identifiant → (empreinte, date).
    #[serde(default)]
    pub gsync: HashMap<String, (u64, String)>,
}

// ---------- Chiffrement ----------

fn md5(b: &[u8]) -> [u8; 16] {
    Md5::digest(b).into()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// AES-128-CBC comme Pronote : clé = MD5(clé), IV = MD5(IV) (ou 16 zéros si IV vide).
fn aes_enc(data: &[u8], key: &[u8], iv: &[u8]) -> String {
    let k = md5(key);
    let i = if iv.is_empty() { [0u8; 16] } else { md5(iv) };
    hex(&cbc::Encryptor::<Aes128>::new(&k.into(), &i.into()).encrypt_padded_vec_mut::<Pkcs7>(data))
}

fn aes_dec(hexs: &str, key: &[u8], iv: &[u8]) -> Option<Vec<u8>> {
    let k = md5(key);
    let i = if iv.is_empty() { [0u8; 16] } else { md5(iv) };
    cbc::Decryptor::<Aes128>::new(&k.into(), &i.into()).decrypt_padded_vec_mut::<Pkcs7>(&unhex(hexs)?).ok()
}

// ---------- Outils ----------

fn net_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(c, _) => format!("Pronote répond par une erreur {c}."),
        ureq::Error::Transport(_) => "Pronote est injoignable (pas d'internet ?).".into(),
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Null => false,
        _ => true,
    }
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn pdate(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%d/%m/%Y").ok()
}

fn pdatetime(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%d/%m/%Y %H:%M:%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s, "%d/%m/%y %Hh%M").ok())
        .or_else(|| pdate(s)?.and_hms_opt(0, 0, 0))
}

fn color(v: &Value) -> [u8; 3] {
    let h = v.as_str().unwrap_or("").trim_start_matches('#');
    let p = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    match (p(0), p(2), p(4)) {
        (Some(r), Some(g), Some(b)) => [r, g, b],
        _ => [90, 200, 250],
    }
}

/// Texte simple à partir du HTML de Pronote.
pub fn strip_html(h: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    let mut name = String::new();
    for ch in h.chars() {
        match ch {
            '<' => {
                tag = true;
                name.clear();
            }
            '>' if tag => {
                tag = false;
                let n = name.trim_start_matches('/').to_lowercase();
                if ["br", "br/", "p", "div", "li", "tr"].contains(&n.split_whitespace().next().unwrap_or("")) {
                    out.push(' ');
                }
            }
            c if tag => name.push(c),
            c => out.push(c),
        }
    }
    accounts::html_unescape(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn kind_path(k: i64) -> &'static str {
    match k {
        7 => "parent",
        8 => "professeur",
        _ => "eleve",
    }
}

/// Adresse lue dans le QR code → (base « https://…/pronote », type de compte).
fn split_url(url: &str) -> Option<(String, i64)> {
    let url = url.split('?').next()?;
    let i = url.rfind('/')?;
    let page = url[i + 1..].to_lowercase();
    let kind = if page.contains("parent") {
        7
    } else if page.contains("professeur") {
        8
    } else {
        6
    };
    Some((url[..i].trim_end_matches('/').to_string(), kind))
}

fn new_uuid() -> String {
    let r = hex(&sys::random(16));
    format!("{}-{}-4{}-a{}-{}", &r[0..8], &r[8..12], &r[13..16], &r[17..20], &r[20..32])
}

/// Lit le QR code dans une image en niveaux de gris (capture d'écran).
pub fn read_qr(w: usize, h: usize, luma: &[u8]) -> Result<String, String> {
    let mut tries: Vec<(usize, usize, Vec<u8>)> = vec![(w, h, luma.to_vec())];
    if w * h < 900_000 {
        // petite capture : on l'agrandit ×2 (modules du QR plus faciles à repérer)
        let (w2, h2) = (w * 2, h * 2);
        let mut big = vec![0u8; w2 * h2];
        for y in 0..h2 {
            for x in 0..w2 {
                big[y * w2 + x] = luma[(y / 2) * w + x / 2];
            }
        }
        tries.push((w2, h2, big));
    }
    for (w, h, px) in tries {
        let mut img = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| px[y * w + x]);
        for g in img.detect_grids() {
            if let Ok((_, txt)) = g.decode() {
                let v: Value = serde_json::from_str(&txt).map_err(|_| "Ce QR code n'est pas un QR code de connexion Pronote.".to_string())?;
                if v["jeton"].is_string() && v["login"].is_string() && v["url"].is_string() {
                    return Ok(txt);
                }
                return Err("Ce QR code n'est pas un QR code de connexion Pronote.".into());
            }
        }
    }
    Err("Aucun QR code trouvé dans l'image. Capture-le en entier, bien net (Win+Maj+S), puis réessaie.".into())
}

// ---------- Session ----------

pub struct Sess {
    ag: ureq::Agent,
    base: String,
    kind: i64,
    id: i64,
    iv: Vec<u8>,
    key: Vec<u8>,
    order: u32,
    v2025: bool,
    v2024: bool,
    crypt: bool,
    zip: bool,
    monday: NaiveDate,
    places: i64,
    ends: Vec<String>,
    periods: Vec<Value>,
    res: Value,
    used: Instant,
    pub version: String,
}

fn parse_start(html: &str) -> Option<Value> {
    let body: String = html.chars().filter(|c| *c != ' ' && *c != '\n' && *c != '\r').collect();
    let start = body.find("Start({")? + "Start(".len();
    let b = body.as_bytes();
    let (mut depth, mut quote, mut end) = (0i32, 0u8, 0usize);
    let mut i = start;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == b'\\' {
                i += 1;
            } else if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                end = i + 1;
                break;
            }
        }
        i += 1;
    }
    let raw = body.get(start..end)?;
    if let Ok(v) = serde_json::from_str(raw) {
        return Some(v);
    }
    // JSON « relâché » : clés sans guillemets, apostrophes
    let mut out = String::new();
    let mut word = String::new();
    for ch in raw.replace('\'', "\"").chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            word.push(ch);
            continue;
        }
        if ch == ':' && !word.is_empty() && !out.ends_with('"') {
            out.push_str(&format!("\"{word}\""));
        } else {
            out.push_str(&word);
        }
        word.clear();
        out.push(ch);
    }
    out.push_str(&word);
    serde_json::from_str(&out).ok()
}

fn parse_version(html: &str) -> Option<Vec<u32>> {
    html.match_indices(">PRONOTE ").find_map(|(i, m)| {
        let rest = &html[i + m.len()..];
        let v: Vec<u32> = rest.split(|c: char| !c.is_ascii_digit() && c != '.').next()?.split('.').filter_map(|x| x.parse().ok()).collect();
        (v.len() == 3).then_some(v)
    })
}

fn acces_msg(n: i64, a: &Value) -> String {
    match n {
        1 => format!("{BAD} (identifiant ou jeton incorrect)."),
        2 | 3 | 4 | 5 | 7 | 8 => "Pronote refuse l'accès à ce compte depuis l'appli mobile.".into(),
        6 | 10 => "Ce compte Pronote est bloqué ou désactivé.".into(),
        _ => {
            let m = &a["AccesMessage"];
            let t = format!("{} {}", s(&m["titre"]), s(&m["message"])).trim().to_string();
            if t.is_empty() { format!("Pronote refuse la connexion (code {n}).") } else { format!("Pronote : {t}") }
        }
    }
}

impl Sess {
    fn dk(&self) -> &'static str {
        if self.v2024 { "data" } else { "donnees" }
    }

    /// Ouvre une session : page d'accueil mobile, puis « FonctionParametres ».
    fn open(base: &str, kind: i64) -> Result<Sess, String> {
        let ag = ureq::AgentBuilder::new().timeout(StdDur::from_secs(25)).user_agent(UA).build();
        let url = format!("{base}/mobile.{}.html?fd=1&login=true", kind_path(kind));
        let html = ag.get(&url).set("Cookie", "appliMobile=1").call().map_err(net_err)?.into_string().map_err(|e| e.to_string())?;
        if html.contains("adresse IP est provisoirement suspendue") {
            return Err("Pronote a bloqué ton adresse IP pour un moment (trop d'essais). Réessaie dans une heure.".into());
        }
        if html.contains("momentanément indisponible") || html.contains("pas disponible") {
            return Err("Pronote est momentanément indisponible.".into());
        }
        let ver = parse_version(&html).ok_or("Pronote ne répond pas comme prévu (page indisponible ?).")?;
        let st = parse_start(&html).ok_or("Pronote ne répond pas comme prévu (session introuvable).")?;
        let ge = |a: u32, b: u32, c: u32| (ver[0], ver[1], ver[2]) >= (a, b, c);
        let v2025 = ge(2025, 1, 3);
        let (crypt, zip) = if v2025 {
            (truthy(&st["CrA"]), truthy(&st["CoA"]))
        } else {
            (!truthy(&st["sCrA"]), !truthy(&st["sCoA"]))
        };
        let mut s = Sess {
            ag,
            base: base.to_string(),
            kind,
            id: st["h"].as_i64().or_else(|| s(&st["h"]).parse().ok()).ok_or("Pronote : numéro de session manquant.")?,
            iv: sys::random(16),
            key: Vec::new(),
            order: 0,
            v2025,
            v2024: ge(2024, 3, 9),
            crypt,
            zip,
            monday: Local::now().date_naive(),
            places: 20,
            ends: Vec::new(),
            periods: Vec::new(),
            res: Value::Null,
            used: Instant::now(),
            version: ver.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("."),
        };
        let rsa_const = st.get("MR").is_none() && st.get("ER").is_none();
        if !rsa_const || truthy(&st["http"]) {
            return Err(format!("Cette version de Pronote ({}) demande un chiffrement que Givre ne gère pas encore.", s.version));
        }
        let uuid = base64::engine::general_purpose::STANDARD.encode(&s.iv);
        let d = json!({"identifiantNav": null, "Uuid": uuid});
        let r = s.call("FonctionParametres", json!({"data": d, "donnees": d}))?;
        let p = if r["data"].is_object() { &r["data"] } else { &r["donnees"] };
        let g = &p["General"];
        if let Some(d) = g["PremierLundi"]["V"].as_str().and_then(pdate) {
            s.monday = d;
        }
        s.places = g["PlacesParJour"].as_i64().unwrap_or(20).max(1);
        s.ends = g["ListeHeuresFin"]["V"].as_array().into_iter().flatten().map(|e| s_l(e)).collect();
        s.periods = g["ListePeriodes"].as_array().cloned().unwrap_or_default();
        Ok(s)
    }

    fn req(&self, onglet: Option<i64>, data: Value) -> Value {
        let mut m = Map::new();
        if let Some(o) = onglet {
            m.insert(if self.v2024 { "Signature" } else { "_Signature_" }.into(), json!({"onglet": o}));
        }
        m.insert(self.dk().into(), data);
        Value::Object(m)
    }

    /// Appel d'une fonction Pronote (numéro d'ordre chiffré, données éventuellement compressées/chiffrées).
    fn call(&mut self, name: &str, data: Value) -> Result<Value, String> {
        self.order += 1;
        let iv = if self.order == 1 { Vec::new() } else { self.iv.clone() };
        let no = aes_enc(self.order.to_string().as_bytes(), &self.key, &iv);
        let url = format!("{}/appelfonction/{}/{}/{}", self.base, self.kind, self.id, no);
        let sec = if self.zip || self.crypt {
            let mut b = data.to_string().into_bytes();
            if self.zip {
                b = miniz_oxide::deflate::compress_to_vec(hex(&b).as_bytes(), 6);
            }
            Value::String(if self.crypt { aes_enc(&b, &self.key, &iv) } else { hex(&b) })
        } else {
            data
        };
        let (kn, kid, ks) = if self.v2025 { ("no", "id", "dataSec") } else { ("numeroOrdre", "nom", "donneesSec") };
        let mut body = Map::new();
        body.insert("session".into(), json!(self.id));
        body.insert(kn.into(), json!(no));
        body.insert(kid.into(), json!(name));
        body.insert(ks.into(), sec);
        let text = self
            .ag
            .post(&url)
            .set("Content-Type", "application/json")
            .send_string(&Value::Object(body).to_string())
            .map_err(net_err)?
            .into_string()
            .map_err(|e| e.to_string())?;
        self.order += 1;
        self.used = Instant::now();
        let v: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                return Err(if text.contains("La page a expir") {
                    "EXPIRE".into()
                } else if text.contains("Votre adresse IP") {
                    "Pronote a bloqué ton adresse IP pour un moment (trop d'essais).".into()
                } else if text.contains("Vous avez d") {
                    "Trop de requêtes vers Pronote : réessaie dans quelques minutes.".into()
                } else {
                    format!("Réponse inattendue de Pronote pour {name}.")
                });
            }
        };
        if let Some(e) = v.get("Erreur") {
            let t = s(&e["Titre"]);
            return Err(if t.contains("expir") { "EXPIRE".into() } else { format!("Pronote : {}", if t.is_empty() { "erreur du serveur" } else { &t }) });
        }
        let mut d = v[ks].clone();
        if self.crypt || self.zip {
            let st = d.as_str().ok_or("Pronote : données chiffrées manquantes.")?.to_string();
            let mut b = if self.crypt {
                aes_dec(&st, &self.key, &self.iv).ok_or("Pronote : impossible de déchiffrer la réponse.")?
            } else {
                unhex(&st).ok_or("Pronote : réponse illisible.")?
            };
            if self.zip {
                b = miniz_oxide::inflate::decompress_to_vec(&b).map_err(|_| "Pronote : réponse illisible.")?;
            }
            d = serde_json::from_slice(&b).map_err(|_| "Pronote : réponse illisible.")?;
        }
        if let Some(t) = d.as_str() {
            d = serde_json::from_str(t).unwrap_or(Value::Null);
        }
        let sig = if self.v2024 { "Signature" } else { "_Signature_" };
        if d[sig].get("Erreur").is_some() {
            return Err(format!("Pronote : {}", s(&d[sig]["MessageErreur"])));
        }
        Ok(d)
    }

    fn identify(&mut self, login: &str, uuid: &str, qr: bool, sav: &str) -> Result<Value, String> {
        let d = json!({
            "genreConnexion": 0,
            "genreEspace": self.kind,
            "identifiant": login,
            "pourENT": false,
            "enConnexionAuto": false,
            "demandeConnexionAuto": false,
            "enConnexionAppliMobile": !qr,
            "demandeConnexionAppliMobile": qr,
            "demandeConnexionAppliMobileJeton": qr,
            "uuidAppliMobile": uuid,
            "loginTokenSAV": sav,
            "informationsAppareil": {"modele": "iPhone18,3", "platforme": "iOS"}
        });
        let r = self.call("Identification", self.req(None, d))?;
        Ok(r[self.dk()].clone())
    }

    /// Résout le défi (clé = identifiant + SHA-256(alea + jeton)) puis récupère la clé de session.
    fn authenticate(&mut self, id: &Value, login: &str, token: &str, variant: u8) -> Result<Value, String> {
        let alea = s(&id["alea"]);
        let comp_log = id["modeCompLog"].as_i64().unwrap_or(0) > 0;
        let comp_mdp = id["modeCompMdp"].as_i64().unwrap_or(0) > 0;
        let mut lg = login.to_string();
        if comp_log {
            lg = lg.to_lowercase().trim().to_string();
        } else if variant == 1 {
            lg = lg.to_lowercase();
        }
        let pw = if comp_mdp { token.to_lowercase() } else { token.to_string() };
        let mut h = Sha256::new();
        h.update(alea.as_bytes());
        h.update(pw.trim().as_bytes());
        let key = format!("{lg}{}", hex(&h.finalize()).to_uppercase()).into_bytes();
        let ch = id["challenge"].as_str().ok_or("Pronote n'a pas envoyé de défi de connexion.")?;
        let sol = aes_enc(ch.as_bytes(), &key, &self.iv);
        let r = self.call("Authentification", self.req(None, json!({"connexion": 0, "challenge": sol, "espace": self.kind})))?;
        let a = r[self.dk()].clone();
        if let Some(n) = a["Acces"].as_i64().filter(|n| *n != 0) {
            return Err(acces_msg(n, &a));
        }
        let cle = a["cle"].as_str().ok_or(format!("{BAD} (pas de clé de session)."))?;
        let dec = aes_dec(cle, &key, &self.iv).ok_or(format!("{BAD} (clé de session illisible)."))?;
        self.key = String::from_utf8_lossy(&dec).split(',').filter_map(|x| x.trim().parse::<u8>().ok()).collect();
        Ok(a)
    }

    fn user(&mut self) -> Result<(String, String), String> {
        let r = self.call("ParametresUtilisateur", json!({}))?;
        self.res = r[self.dk()]["ressource"].clone();
        Ok((s(&self.res["L"]), s(&self.res["classeDEleve"]["L"])))
    }

    fn week(&self, d: NaiveDate) -> i64 {
        1 + (d - self.monday).num_days().div_euclid(7)
    }

    fn lesson(&self, c: &Value) -> Option<Lesson> {
        let start = pdatetime(c["DateDuCours"]["V"].as_str()?)?;
        let end = c["DateDuCoursFin"]["V"]
            .as_str()
            .and_then(pdatetime)
            .or_else(|| {
                let pos = (c["place"].as_i64()? % self.places + c["duree"].as_i64()? - 1) as usize;
                let (h, m) = self.ends.get(pos)?.split_once('h')?;
                start.date().and_hms_opt(h.trim().parse().ok()?, m.trim().parse().unwrap_or(0), 0)
            })
            .filter(|e| *e > start)
            .unwrap_or(start + Duration::minutes(55));
        let (mut subject, mut teachers, mut rooms) = (String::new(), Vec::new(), Vec::new());
        for x in c["ListeContenus"]["V"].as_array().into_iter().flatten() {
            match x["G"].as_i64() {
                Some(16) => subject = s(&x["L"]),
                Some(3) | Some(34) => teachers.push(s(&x["L"])),
                Some(17) => rooms.push(s(&x["L"])),
                _ => {}
            }
        }
        if subject.is_empty() {
            subject = if truthy(&c["estSortiePedagogique"]) {
                "Sortie pédagogique".into()
            } else if c.get("estRetenue").is_some() {
                "Retenue".into()
            } else {
                let m = s(&c["motif"]);
                if m.is_empty() { "Cours".into() } else { m }
            };
        }
        Some(Lesson {
            id: s(&c["N"]),
            start,
            end,
            subject,
            teacher: teachers.join(", "),
            room: rooms.join(", "),
            status: s(&c["Statut"]),
            canceled: truthy(&c["estAnnule"]),
            color: color(&c["CouleurFond"]),
            ..Default::default()
        })
    }

    fn timetable(&mut self, week: i64) -> Result<Vec<Lesson>, String> {
        let res = json!({"G": self.res["G"], "L": self.res["L"], "N": self.res["N"]});
        let d = json!({
            "estEDTAnnuel": false, "estEDTPermanence": false, "avecAbsencesEleve": false,
            "avecRessourcesLibrePiedHoraire": false, "avecAbsencesRessource": true, "avecInfosPrefsGrille": true,
            "avecConseilDeClasse": true, "avecCoursSortiePeda": true, "avecDisponibilites": true, "avecRetenuesEleve": true,
            "edt": {"G": 16, "L": "Emploi du temps"},
            "ressource": res, "Ressource": res, "numeroSemaine": week, "NumeroSemaine": week
        });
        let r = self.call("PageEmploiDuTemps", self.req(Some(16), d))?;
        let dk = self.dk();
        Ok(r[dk]["ListeCours"].as_array().into_iter().flatten().filter_map(|c| self.lesson(c)).collect())
    }

    fn homework(&mut self, w1: i64, w2: i64) -> Result<Vec<Homework>, String> {
        let dom = if w1 == w2 { format!("[{w1}]") } else { format!("[{w1}..{w2}]") };
        let r = self.call("PageCahierDeTexte", self.req(Some(88), json!({"domaine": {"_T": 8, "V": dom}})))?;
        Ok(r[self.dk()]["ListeTravauxAFaire"]["V"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|h| {
                Some(Homework {
                    id: s(&h["N"]),
                    subject: s(&h["Matiere"]["V"]["L"]),
                    text: strip_html(&s(&h["descriptif"]["V"])),
                    due: pdate(h["PourLe"]["V"].as_str()?)?,
                    done: truthy(&h["TAFFait"]),
                    color: color(&h["CouleurFond"]),
                })
            })
            .collect())
    }

    fn set_done(&mut self, id: &str, done: bool) -> Result<(), String> {
        let d = json!({"listeTAF": [{"E": 2, "TAFFait": done, "N": id}]});
        self.call("SaisieTAFFaitEleve", self.req(Some(88), d)).map(|_| ())
    }

    fn grades(&mut self, sc: &mut School) -> Result<(), String> {
        let today = Local::now().date_naive();
        let tab = self.res["listeOngletsPourPeriodes"]["V"].as_array().into_iter().flatten().find(|t| t["G"].as_i64() == Some(198));
        let def = tab.map(|t| s(&t["periodeParDefaut"]["V"]["N"])).unwrap_or_default();
        let per = self
            .periods
            .iter()
            .find(|p| !def.is_empty() && s(&p["N"]) == def)
            .or_else(|| {
                self.periods.iter().find(|p| {
                    let a = p["dateDebut"]["V"].as_str().and_then(pdate);
                    let b = p["dateFin"]["V"].as_str().and_then(pdate);
                    matches!((a, b), (Some(a), Some(b)) if a <= today && today <= b) && p["G"].as_i64() != Some(3)
                })
            })
            .or_else(|| self.periods.first())
            .cloned()
            .ok_or("Pronote : aucune période de notes.")?;
        let r = self.call("DernieresNotes", self.req(Some(198), json!({"Periode": {"N": per["N"], "G": per["G"], "L": per["L"]}})))?;
        let d = &r[self.dk()];
        sc.period = s(&per["L"]);
        sc.grades = d["listeDevoirs"]["V"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| {
                Some(Grade {
                    id: s(&g["N"]),
                    subject: s(&g["service"]["V"]["L"]),
                    value: s(&g["note"]["V"]),
                    out_of: s(&g["bareme"]["V"]),
                    date: pdate(g["date"]["V"].as_str()?)?,
                    class_avg: s(&g["moyenne"]["V"]),
                    coef: s(&g["coefficient"]),
                })
            })
            .collect();
        sc.grades.sort_by(|a, b| b.date.cmp(&a.date));
        sc.averages = d["listeServices"]["V"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| Avg { subject: s(&a["L"]), student: s(&a["moyEleve"]["V"]), class: s(&a["moyClasse"]["V"]) })
            .collect();
        sc.overall = s(&d["moyGenerale"]["V"]);
        sc.class_overall = s(&d["moyGeneraleClasse"]["V"]);
        Ok(())
    }

    fn news(&mut self) -> Result<Vec<News>, String> {
        let r = self.call("PageActualites", self.req(Some(8), json!({"modesAffActus": {"_T": 26, "V": "[0]"}})))?;
        let mut v: Vec<News> = r[self.dk()]["listeModesAff"][0]["listeActualites"]["V"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|n| truthy(&n["estInformation"]) || truthy(&n["estSondage"]))
            .map(|n| News {
                id: s(&n["N"]),
                title: s(&n["L"]),
                author: s(&n["auteur"]),
                date: n["dateDebut"]["V"].as_str().or(n["dateCreation"]["V"].as_str()).and_then(pdate).unwrap_or_default(),
                text: strip_html(&s(&n["listeQuestions"]["V"][0]["texte"]["V"])),
                read: truthy(&n["lue"]),
            })
            .collect();
        v.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(v)
    }

    fn talks(&mut self) -> Result<Vec<Talk>, String> {
        let d = json!({"avecLu": true, "avecMessage": true, "possessionMessageDiscussionUnique": null});
        let r = self.call("ListeMessagerie", self.req(Some(131), d))?;
        Ok(r[self.dk()]["listeMessagerie"]["V"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|t| truthy(&t["estUneDiscussion"]) && t["profondeur"].as_i64().unwrap_or(0) == 0)
            .map(|t| Talk {
                subject: s(&t["objet"]),
                from: s(&t["initiateur"]),
                when: s(&t["libelleDate"]),
                unread: t["nbNonLus"].as_i64().unwrap_or(0),
            })
            .collect())
    }
}

fn s_l(e: &Value) -> String {
    s(&e["L"])
}

/// Connexion complète (QR code ou jeton). Renvoie la session, le nouveau jeton et l'identifiant.
fn login(base: &str, kind: i64, login: &str, token: &str, uuid: &str, qr: bool, variant: u8) -> Result<(Sess, String, String), String> {
    let mut s = Sess::open(base, kind)?;
    let sav = if !qr && variant == 1 { token } else { "" };
    let id = s.identify(login, uuid, qr, sav)?;
    let lg = if qr { id["login"].as_str().filter(|x| !x.is_empty()).unwrap_or(login).to_string() } else { login.to_string() };
    sys::log(&format!(
        "pronote : identification ok (modeCompLog={}, modeCompMdp={}, login renvoyé={}, variante {variant})",
        id["modeCompLog"], id["modeCompMdp"], id["login"].is_string()
    ));
    let a = s.authenticate(&id, &lg, token, variant)?;
    let next = a["jetonConnexionAppliMobile"].as_str().unwrap_or(token).to_string();
    if a.get("actionsDoubleAuth").is_some_and(truthy) && next != token {
        // Pronote demande de sécuriser le compte : comme l'appli, on se reconnecte avec le nouveau jeton.
        return self::login(base, kind, &lg, &next, uuid, false, variant);
    }
    Ok((s, next, lg))
}

// ---------- Fil Pronote ----------

pub enum PJob {
    /// Actualiser (avec copie dans Google Agenda ?).
    Sync(bool),
    /// QR code (JSON) + code à 4 chiffres.
    Login(String, String),
    Done(String, bool),
    Logout,
}

#[derive(Serialize, Deserialize, Default)]
struct Cache {
    school: School,
    seen: HashSet<String>,
}

fn cache_path() -> std::path::PathBuf {
    crate::config::dir().join("pronote.bin")
}

fn load_cache() -> Cache {
    std::fs::read(cache_path())
        .ok()
        .and_then(|b| sys::unprotect(&b))
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_cache(c: &Cache) {
    if let Some(b) = serde_json::to_vec(c).ok().and_then(|j| sys::protect(&j)) {
        let _ = std::fs::write(cache_path(), b);
    }
}

struct Worker {
    out: Out,
    vault: Arc<Mutex<Secrets>>,
    sess: Option<Sess>,
    cache: Cache,
}

pub fn spawn(out: Out, vault: Arc<Mutex<Secrets>>) -> Sender<PJob> {
    let (tx, rx) = channel::<PJob>();
    let _ = std::thread::Builder::new().name("pronote".into()).spawn(move || {
        let mut w = Worker { out, vault, sess: None, cache: load_cache() };
        let acc = w.vault.lock().unwrap().pronote.clone();
        w.cache.school.connected = acc.is_some();
        if acc.is_none() {
            w.cache = Cache::default();
        }
        w.publish();
        for job in rx {
            match job {
                PJob::Sync(g) => w.sync(g),
                PJob::Login(qr, pin) => w.login_qr(&qr, &pin),
                PJob::Done(id, done) => w.done(&id, done),
                PJob::Logout => {
                    w.sess = None;
                    let mut v = w.vault.lock().unwrap();
                    v.pronote = None;
                    accounts::save(&v);
                    drop(v);
                    w.cache = Cache::default();
                    let _ = std::fs::remove_file(cache_path());
                    w.publish();
                    w.out.send(Msg::Status("Pronote déconnecté.".into()));
                }
            }
        }
    });
    tx
}

impl Worker {
    fn publish(&self) {
        self.out.send(Msg::School(self.cache.school.clone()));
    }

    fn save_acct(&self, a: &PAcct) {
        let mut v = self.vault.lock().unwrap();
        v.pronote = Some(a.clone());
        accounts::save(&v);
    }

    /// Session prête (réutilisée si elle a servi il y a moins de 4 min, sinon reconnexion par jeton).
    fn session(&mut self) -> Result<&mut Sess, String> {
        if self.sess.as_ref().is_some_and(|s| s.used.elapsed() < StdDur::from_secs(240)) {
            return Ok(self.sess.as_mut().unwrap());
        }
        self.sess = None;
        let mut a = self.vault.lock().unwrap().pronote.clone().ok_or("Pronote n'est pas connecté.")?;
        if !a.blocked.is_empty() {
            return Err(a.blocked.clone());
        }
        let order = if a.variant == 1 { [1u8, 0] } else { [0u8, 1] };
        let mut last = String::new();
        for (i, v) in order.into_iter().enumerate() {
            match login(&a.base, a.kind, &a.login, &a.token, &a.uuid, false, v) {
                Ok((mut s, tok, lg)) => {
                    // le jeton change à chaque connexion : on l'enregistre tout de suite
                    a.token = tok;
                    a.login = lg;
                    a.variant = v;
                    self.save_acct(&a);
                    let (name, class) = s.user()?;
                    if name != a.name || class != a.class {
                        a.name = name;
                        a.class = class;
                        self.save_acct(&a);
                    }
                    self.sess = Some(s);
                    return Ok(self.sess.as_mut().unwrap());
                }
                Err(e) if e.starts_with(BAD) && i == 0 => {
                    sys::log(&format!("pronote : variante {v} refusée, essai de l'autre"));
                    last = e;
                }
                Err(e) => return Err(e),
            }
        }
        Err(format!("{last} Reconnecte-toi avec un nouveau QR code (Paramètres › Comptes)."))
    }

    fn login_qr(&mut self, qr: &str, pin: &str) {
        sys::log("QR : essai");
        let r = (|| -> Result<String, String> {
            let q: Value = serde_json::from_str(qr).map_err(|_| "QR code illisible.")?;
            let (base, kind) = split_url(q["url"].as_str().unwrap_or("")).ok_or("Adresse Pronote absente du QR code.")?;
            let dec = |k: &str| {
                aes_dec(q[k].as_str().unwrap_or(""), pin.trim().as_bytes(), &[])
                    .and_then(|b| String::from_utf8(b).ok())
                    .ok_or("Code à 4 chiffres incorrect (ou QR code périmé).".to_string())
            };
            let (lg, tok) = (dec("login")?, dec("jeton")?);
            let uuid = self.vault.lock().unwrap().pronote.as_ref().map(|a| a.uuid.clone()).unwrap_or_else(new_uuid);
            let mut last = String::new();
            for v in [0u8, 1] {
                match login(&base, kind, &lg, &tok, &uuid, true, v) {
                    Ok((mut s, token, login)) => {
                        let (name, class) = s.user()?;
                        let a = PAcct { base: base.clone(), kind, login, token, uuid: uuid.clone(), name: name.clone(), class, variant: v, ..Default::default() };
                        self.save_acct(&a);
                        sys::log(&format!("pronote : connecté (Pronote {}, variante {v})", s.version));
                        self.sess = Some(s);
                        return Ok(name);
                    }
                    Err(e) if e.starts_with(BAD) && v == 0 => {
                        sys::log("pronote : QR — variante 0 refusée, essai de la variante 1");
                        last = e;
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(format!("{last} Vérifie ton code, ou génère un nouveau QR code sur Pronote."))
        })();
        match r {
            Ok(name) => {
                sys::log("QR : réussi");
                self.cache = Cache::default();
                self.out.send(Msg::Status(format!("Pronote connecté : {name}. Chargement…")));
                self.sync(false);
            }
            Err(e) => {
                sys::log(&format!("QR : {e}"));
                let shown = if refusal_msg(&e) == SUSPENDED { SUSPENDED.to_string() } else { e };
                self.out.send(Msg::Status(shown));
            }
        }
    }

    fn fetch(&mut self, sc: &mut School) -> Result<(), String> {
        let today = Local::now().date_naive();
        let s = self.session()?;
        let w = s.week(today);
        let mut lessons = Vec::new();
        for k in 0..3 {
            lessons.extend(s.timetable(w + k)?);
        }
        lessons.retain(|l| l.end.date() >= today - Duration::days(1));
        sc.lessons = lessons;
        let mut errs = Vec::new();
        match s.homework(w, w + 2) {
            Ok(mut h) => {
                h.retain(|x| x.due >= today);
                h.sort_by_key(|x| x.due);
                sc.homework = h;
            }
            Err(e) => errs.push(format!("devoirs : {e}")),
        }
        if let Err(e) = s.grades(sc) {
            errs.push(format!("notes : {e}"));
        }
        match s.news() {
            Ok(n) => sc.news = n,
            Err(e) => errs.push(format!("annonces : {e}")),
        }
        match s.talks() {
            Ok(t) => sc.talks = t,
            Err(e) => errs.push(format!("messagerie : {e}")),
        }
        if !errs.is_empty() {
            sys::log(&format!("pronote : {}", errs.join(" | ")));
        }
        Ok(())
    }

    fn sync(&mut self, google: bool) {
        let Some(acc) = self.vault.lock().unwrap().pronote.clone() else {
            self.cache = Cache::default();
            self.publish();
            return;
        };
        let mut sc = self.cache.school.clone();
        // refus précédent : on n'essaie plus tout seul (sinon Pronote bloque l'adresse IP)
        if !acc.blocked.is_empty() {
            sc.connected = true;
            sc.name = acc.name.clone();
            sc.class = acc.class.clone();
            sc.base = acc.base.clone();
            sc.error = acc.blocked.clone();
            self.cache.school = sc;
            self.publish();
            return;
        }
        let mut r = self.fetch(&mut sc);
        if r.as_ref().is_err_and(|e| e == "EXPIRE") {
            self.sess = None;
            r = self.fetch(&mut sc);
        }
        let acc = self.vault.lock().unwrap().pronote.clone().unwrap_or(acc);
        sc.connected = true;
        sc.name = acc.name.clone();
        sc.class = acc.class.clone();
        sc.base = acc.base.clone();
        match r {
            Ok(()) => {
                sc.error.clear();
                sc.updated = Some(Local::now().naive_local());
                self.notify(&sc);
            }
            Err(e) => {
                let e = if e == "EXPIRE" { "La session Pronote a expiré.".to_string() } else { e };
                sys::log(&format!("pronote : {e}"));
                self.sess = None;
                if is_refusal(&e) {
                    let msg = refusal_msg(&e);
                    // (copie d'abord : le verrou du coffre doit être relâché avant save_acct)
                    let saved = self.vault.lock().unwrap().pronote.clone();
                    if let Some(mut a) = saved {
                        a.blocked = msg.clone();
                        self.save_acct(&a);
                    }
                    sys::log("pronote : plus d'essai automatique jusqu'à une nouvelle connexion par QR code");
                    sc.error = msg;
                } else {
                    sc.error = e;
                }
            }
        }
        self.cache.school = sc;
        save_cache(&self.cache);
        self.publish();
        if google && self.cache.school.error.is_empty() {
            match google_push(&self.vault, &self.cache.school) {
                Ok(0) => {}
                Ok(n) => sys::log(&format!("pronote → google : {n} événements mis à jour")),
                Err(e) => {
                    sys::log(&format!("pronote → google : {e}"));
                    self.out.send(Msg::Status(format!("Copie Pronote → Google : {e}")));
                }
            }
        }
    }

    /// Notifications : nouvelle note, nouveau devoir, cours annulé / prof absent (7 prochains jours).
    fn notify(&mut self, sc: &School) {
        let primed = !self.cache.seen.is_empty();
        let now = Local::now().naive_local();
        let mut new: Vec<(u8, String, String)> = Vec::new();
        for g in &sc.grades {
            if self.cache.seen.insert(format!("n:{}", g.id)) {
                let avg = if g.class_avg.is_empty() { String::new() } else { format!(" (classe : {})", grade_txt(&g.class_avg)) };
                new.push((0, format!("Nouvelle note — {}", g.subject), format!("{}/{}{avg}", grade_txt(&g.value), g.out_of)));
            }
        }
        for h in &sc.homework {
            if self.cache.seen.insert(format!("d:{}", h.id)) && !h.done {
                new.push((1, format!("Nouveau devoir — {}", h.subject), format!("Pour le {} : {}", h.due.format("%d/%m"), h.text)));
            }
        }
        let pai = crate::pai::actuel();
        for l in &sc.lessons {
            // PAI : pas d'alerte « cours annulé » pour un cours où je ne vais pas
            if l.bad() && crate::pai::pourquoi(&pai, l).is_none() && l.start > now && l.start < now + Duration::days(7) && self.cache.seen.insert(format!("c:{}:{}", l.id, l.start)) {
                let what = if l.alert() == Alert::Absent { "Prof absent" } else { "Cours annulé" };
                new.push((2, format!("{what} — {}", l.subject), format!("{} à {}", crate::widgets::day_label(l.start.date(), now.date()), l.start.format("%H:%M"))));
            }
        }
        if primed {
            for (k, t, b) in new.into_iter().take(4) {
                self.out.send(Msg::PNotify(k, t, b.chars().take(180).collect()));
            }
        }
        if self.cache.seen.len() > 3000 {
            self.cache.seen.clear();
        }
    }

    fn done(&mut self, id: &str, done: bool) {
        let mut r = self.session().and_then(|s| s.set_done(id, done));
        if r.as_ref().is_err_and(|e| e == "EXPIRE") {
            self.sess = None;
            r = self.session().and_then(|s| s.set_done(id, done));
        }
        if let Some(h) = self.cache.school.homework.iter_mut().find(|h| h.id == id) {
            h.done = if r.is_ok() { done } else { !done };
        }
        if let Err(e) = r {
            sys::log(&format!("pronote : devoir non coché : {e}"));
            self.out.send(Msg::Status(format!("Devoir non synchronisé avec Pronote : {e}")));
        }
        save_cache(&self.cache);
        self.publish();
    }
}

/// Numéros des devoirs pas encore faits, à rendre ce jour-là (et de cette matière si elle est donnée).
pub fn devoirs_a_cocher(devoirs: &[Homework], jour: NaiveDate, matiere: &str) -> Vec<usize> {
    let m = crate::pai::simple(matiere);
    devoirs
        .iter()
        .enumerate()
        .filter(|(_, h)| h.due == jour && !h.done && (m.is_empty() || crate::pai::simple(&h.subject).contains(&m)))
        .map(|(i, _)| i)
        .collect()
}

/// Tous les devoirs de ce jour sont-ils faits ? (faux s'il n'y en a aucun)
pub fn jour_termine(devoirs: &[Homework], jour: NaiveDate) -> bool {
    let mut du_jour = devoirs.iter().filter(|h| h.due == jour).peekable();
    du_jour.peek().is_some() && du_jour.all(|h| h.done)
}

#[cfg(test)]
mod tests_devoirs {
    use super::*;

    fn hw(id: &str, matiere: &str, jour: u32, fait: bool) -> Homework {
        Homework { id: id.into(), subject: matiere.into(), text: String::new(), due: NaiveDate::from_ymd_opt(2026, 11, jour).unwrap(), done: fait, color: [0; 3] }
    }

    #[test]
    fn devoirs_du_jour() {
        let v = vec![hw("a", "MATHÉMATIQUES", 23, false), hw("b", "FRANCAIS", 23, true), hw("c", "SVT", 24, false), hw("d", "FRANCAIS", 23, false)];
        let j = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        assert_eq!(devoirs_a_cocher(&v, j, ""), vec![0, 3]);
        assert_eq!(devoirs_a_cocher(&v, j, "maths"), Vec::<usize>::new());
        assert_eq!(devoirs_a_cocher(&v, j, "mathématiques"), vec![0]);
        assert!(!jour_termine(&v, j));
        assert!(jour_termine(&v[1..2], j));
        assert!(!jour_termine(&[], j));
    }
}

// ---------- Copie dans Google Agenda (« Pronote (Givre) ») ----------

fn google_push(vault: &Mutex<Secrets>, sc: &School) -> Result<usize, String> {
    let ag = crate::data::agent();
    let tok = accounts::google_token(&ag, vault)?;
    let auth = format!("Bearer {tok}");
    let mut acc = vault.lock().unwrap().pronote.clone().ok_or("Pronote n'est pas connecté.")?;
    let api = "https://www.googleapis.com/calendar/v3";
    // agenda « Pronote (Givre) » : retrouvé ou créé
    let exists = !acc.gcal.is_empty()
        && ag.get(&format!("{api}/calendars/{}", enc(&acc.gcal))).set("Authorization", &auth).call().is_ok();
    if !exists {
        acc.gsync.clear();
        let list: Value = ag
            .get(&format!("{api}/users/me/calendarList?minAccessRole=owner"))
            .set("Authorization", &auth)
            .call()
            .map_err(accounts::http_err)?
            .into_json()
            .map_err(|e| e.to_string())?;
        acc.gcal = list["items"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["summary"] == GCAL)
            .map(|c| s(&c["id"]))
            .unwrap_or_default();
        if acc.gcal.is_empty() {
            let c: Value = ag
                .post(&format!("{api}/calendars"))
                .set("Authorization", &auth)
                .send_json(json!({"summary": GCAL, "timeZone": "Europe/Paris"}))
                .map_err(|e| format!("{} (reconnecte ton compte Google : Givre demande maintenant le droit de créer un agenda)", accounts::http_err(e)))?
                .into_json()
                .map_err(|e| e.to_string())?;
            acc.gcal = s(&c["id"]);
        }
    }
    let cal = enc(&acc.gcal);
    let f = |d: NaiveDateTime| d.format("%Y-%m-%dT%H:%M:%S").to_string();
    let now = Local::now().naive_local();
    let mut want: Vec<(String, Value, String)> = Vec::new();
    let pai = crate::pai::actuel();
    for l in &sc.lessons {
        // PAI : les cours où je ne vais pas ne vont pas dans mon agenda
        if l.end < now - Duration::days(1) || crate::pai::pourquoi(&pai, l).is_some() {
            continue;
        }
        let mut title = l.subject.clone();
        if l.alert() != Alert::None {
            title = format!("{} — {}", l.alert_label(), title);
        }
        let desc = [l.teacher.as_str(), l.status.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join("\n");
        let mut ev = json!({
            "summary": title, "location": l.room, "description": desc, "status": "confirmed",
            "start": {"dateTime": f(l.start), "timeZone": "Europe/Paris"},
            "end": {"dateTime": f(l.end), "timeZone": "Europe/Paris"},
            "reminders": {"useDefault": false}
        });
        if l.bad() {
            ev["colorId"] = json!("11");
        }
        want.push((format!("c{}{}", l.id, l.start), ev, l.start.date().to_string()));
    }
    for h in &sc.homework {
        let title = format!("{}Devoir — {}", if h.done { "✓ " } else { "" }, h.subject);
        let ev = json!({
            "summary": title, "description": h.text, "status": "confirmed", "transparency": "transparent",
            "start": {"date": h.due.to_string()}, "end": {"date": (h.due + Duration::days(1)).to_string()},
            "reminders": {"useDefault": false}, "colorId": "6"
        });
        want.push((format!("d{}", h.id), ev, h.due.to_string()));
    }
    let mut n = 0;
    let mut keep = HashSet::new();
    for (key, mut ev, date) in want {
        let id = format!("givre{}", &hex(&Sha256::digest(key.as_bytes()))[..40]);
        let fp = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&ev.to_string(), &mut h);
            std::hash::Hasher::finish(&h)
        };
        keep.insert(id.clone());
        if acc.gsync.get(&id).is_some_and(|(x, _)| *x == fp) {
            continue;
        }
        let r = ag.put(&format!("{api}/calendars/{cal}/events/{id}")).set("Authorization", &auth).send_json(ev.clone());
        let r = match r {
            Err(ureq::Error::Status(404, _)) => {
                ev["id"] = json!(id);
                ag.post(&format!("{api}/calendars/{cal}/events")).set("Authorization", &auth).send_json(ev)
            }
            r => r,
        };
        r.map_err(accounts::http_err)?;
        acc.gsync.insert(id, (fp, date));
        n += 1;
    }
    // cours / devoirs disparus de Pronote (à venir) : supprimés de l'agenda
    let today = now.date().to_string();
    let gone: Vec<String> = acc.gsync.iter().filter(|(id, (_, d))| !keep.contains(*id) && *d >= today).map(|(id, _)| id.clone()).collect();
    for id in gone {
        let _ = ag.delete(&format!("{api}/calendars/{cal}/events/{id}")).set("Authorization", &auth).call();
        acc.gsync.remove(&id);
        n += 1;
    }
    acc.gsync.retain(|_, (_, d)| *d >= today);
    let mut v = vault.lock().unwrap();
    if let Some(a) = v.pronote.as_mut() {
        a.gcal = acc.gcal;
        a.gsync = acc.gsync;
        accounts::save(&v);
    }
    Ok(n)
}

// ---------- Test sans compte (givre.exe --pronote-test DOSSIER) ----------

pub fn selftest(dir: &str) {
    let base = "https://0000000a.index-education.net/pronote";
    let mut log = String::new();
    match Sess::open(base, 6) {
        Ok(s) => {
            log.push_str(&format!(
                "OK : Pronote {} · session {} · chiffrement {} · compression {} · premier lundi {} · {} créneaux/jour · {} heures de fin · {} périodes\n",
                s.version, s.id, s.crypt, s.zip, s.monday, s.places, s.ends.len(), s.periods.len()
            ));
            log.push_str(&format!("semaine actuelle : {}\n", s.week(Local::now().date_naive())));
            for p in &s.periods {
                log.push_str(&format!("  période {} (G={})\n", s_l(p), p["G"]));
            }
        }
        Err(e) => log.push_str(&format!("ERREUR : {e}\n")),
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(std::path::Path::new(dir).join("pronote_test.txt"), log);
}

/// Données d'exemple pour l'aperçu (--apercu).
pub fn demo() -> School {
    let now = Local::now().naive_local();
    let d0 = now.date();
    let at = |d: i64, h: u32, m: u32| (d0 + Duration::days(d)).and_hms_opt(h, m, 0).unwrap_or(now);
    let first = now.with_second(0).unwrap_or(now) + Duration::minutes(17);
    let lesson = |st: NaiveDateTime, min: i64, sub: &str, t: &str, r: &str, status: &str, c: [u8; 3]| Lesson {
        id: format!("{sub}{st}"),
        start: st,
        end: st + Duration::minutes(min),
        subject: sub.into(),
        teacher: t.into(),
        room: r.into(),
        status: status.into(),
        canceled: status.contains("annul") || status.contains("absent"),
        color: c,
        ..Default::default()
    };
    let mut lessons = vec![
        lesson(first, 55, "Mathématiques", "M. Durand", "B12", "", [90, 200, 250]),
        lesson(first + Duration::minutes(60), 55, "Anglais LV1", "Mme Martin", "C04", "Prof. absent", [255, 159, 10]),
        lesson(first + Duration::minutes(120), 55, "Histoire-Géographie", "Mme Petit", "A21", "Changement de salle", [191, 90, 242]),
    ];
    for (i, (h, sub, c)) in [(8, "Français", [52, 199, 89]), (9, "SVT", [48, 209, 88]), (10, "EPS", [255, 55, 95]), (14, "Physique-Chimie", [10, 132, 255])]
        .into_iter()
        .enumerate()
    {
        lessons.push(lesson(at(1, h, 30), 55, sub, "M. Bernard", &format!("D1{i}"), "", c));
    }
    School {
        connected: true,
        name: "DUPONT Camille".into(),
        class: "3E2".into(),
        base: "https://0000000a.index-education.net/pronote".into(),
        lessons,
        homework: vec![
            Homework { id: "1".into(), subject: "Français".into(), text: "Lire le chapitre 3 et répondre aux questions 1 à 5 p. 42".into(), due: d0 + Duration::days(1), done: false, color: [52, 199, 89] },
            Homework { id: "2".into(), subject: "Mathématiques".into(), text: "Exercices 12, 13 et 14 page 87".into(), due: d0 + Duration::days(1), done: true, color: [90, 200, 250] },
            Homework { id: "3".into(), subject: "Anglais LV1".into(), text: "Apprendre le vocabulaire de l'unité 4".into(), due: d0 + Duration::days(3), done: false, color: [255, 159, 10] },
            Homework { id: "4".into(), subject: "SVT".into(), text: "Finir le schéma de la cellule".into(), due: d0 + Duration::days(4), done: false, color: [48, 209, 88] },
        ],
        grades: vec![
            Grade { id: "a".into(), subject: "Mathématiques".into(), value: "16,5".into(), out_of: "20".into(), date: d0 - Duration::days(1), class_avg: "12,8".into(), coef: "1".into() },
            Grade { id: "b".into(), subject: "Français".into(), value: "13".into(), out_of: "20".into(), date: d0 - Duration::days(3), class_avg: "11,9".into(), coef: "2".into() },
            Grade { id: "c".into(), subject: "Anglais LV1".into(), value: "8,5".into(), out_of: "20".into(), date: d0 - Duration::days(5), class_avg: "10,2".into(), coef: "1".into() },
            Grade { id: "d".into(), subject: "SVT".into(), value: "|1".into(), out_of: "20".into(), date: d0 - Duration::days(6), class_avg: "13,1".into(), coef: "1".into() },
            Grade { id: "e".into(), subject: "Histoire-Géographie".into(), value: "17".into(), out_of: "20".into(), date: d0 - Duration::days(8), class_avg: "12".into(), coef: "1".into() },
        ],
        averages: Vec::new(),
        overall: "14,2".into(),
        class_overall: "12,6".into(),
        period: "Trimestre 1".into(),
        news: vec![
            News { id: "x".into(), title: "Sortie au musée d'Aquitaine".into(), author: "M. le Principal".into(), date: d0 - Duration::days(1), text: "Autorisation à rendre avant vendredi.".into(), read: false },
            News { id: "y".into(), title: "Menu de la cantine".into(), author: "Vie scolaire".into(), date: d0 - Duration::days(4), text: String::new(), read: true },
        ],
        talks: vec![Talk { subject: "Rappel : brevet blanc".into(), from: "Mme Martin".into(), when: "hier 17h02".into(), unread: 1 }],
        updated: Some(now),
        error: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_capture_et_code() {
        let pin = b"1234";
        let txt = json!({
            "jeton": aes_enc(b"JETON-SECRET-42", pin, &[]),
            "login": aes_enc(b"c.dupont", pin, &[]),
            "url": "https://0000000a.index-education.net/pronote/mobile.eleve.html?login=true"
        })
        .to_string();
        let code = qrcode::QrCode::new(txt.as_bytes()).unwrap();
        let (n, px, q) = (code.width(), 3usize, 4usize);
        let side = (n + 2 * q) * px;
        // DIB 32 bits « de bas en haut », comme une capture Win+Maj+S
        let mut dib = Vec::new();
        for v in [40u32, side as u32, side as u32] {
            dib.extend(v.to_le_bytes());
        }
        dib.extend(1u16.to_le_bytes());
        dib.extend(32u16.to_le_bytes());
        dib.extend([0u8; 24]);
        let colors = code.to_colors();
        for y in (0..side).rev() {
            for x in 0..side {
                let (mx, my) = ((x / px) as i64 - q as i64, (y / px) as i64 - q as i64);
                let dark = mx >= 0 && my >= 0 && (mx as usize) < n && (my as usize) < n && colors[my as usize * n + mx as usize] == qrcode::Color::Dark;
                let c = if dark { 0u8 } else { 255 };
                dib.extend([c, c, c, 255]);
            }
        }
        let (w, h, luma) = sys::parse_dib(&dib).expect("DIB");
        let got = read_qr(w, h, &luma).expect("QR");
        assert_eq!(got, txt);
        let v: Value = serde_json::from_str(&got).unwrap();
        assert_eq!(aes_dec(v["jeton"].as_str().unwrap(), pin, &[]).unwrap(), b"JETON-SECRET-42");
        assert!(aes_dec(v["jeton"].as_str().unwrap(), b"9999", &[]).is_none_or(|b| b != b"JETON-SECRET-42"));
        assert_eq!(split_url(v["url"].as_str().unwrap()), Some(("https://0000000a.index-education.net/pronote".into(), 6)));
    }

    #[test]
    fn page_et_cours() {
        let html = "<script>try{Start ({h:'123',a:6, CrA:true});}catch(e){}</script><title>PRONOTE - X</title><a>PRONOTE 2026.2.8 gestion</a>";
        assert_eq!(parse_start(html).unwrap()["h"], "123");
        assert_eq!(parse_version(&html.replace("<a>", "<a >").replace("<a >", "<a>")), Some(vec![2026, 2, 8]));
        let now = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap().and_hms_opt(10, 0, 0).unwrap();
        let mk = |h: u32, st: &str, c: bool| Lesson {
            start: now.date().and_hms_opt(h, 0, 0).unwrap(),
            end: now.date().and_hms_opt(h, 55, 0).unwrap(),
            subject: format!("{h}"),
            status: st.into(),
            canceled: c,
            ..Default::default()
        };
        let sc = School { lessons: vec![mk(11, "Cours annulé", true), mk(11, "", false), mk(9, "", false), mk(14, "Prof. absent", true)], ..Default::default() };
        let (a, b) = sc.next(now);
        assert!(a.is_some_and(|l| l.start.hour() == 11 && !l.canceled));
        assert!(b.is_some_and(|l| l.alert() == Alert::Absent));
        assert_eq!(grade_txt("|1"), "Abs");
        assert_eq!(strip_html("<p>Lire&nbsp;p.&#160;42</p><br/>et   finir"), "Lire p. 42 et finir");
    }
}

#[cfg(test)]
mod tests_refus {
    use super::*;

    #[test]
    fn refus_et_blocage() {
        assert!(is_refusal("Pronote a refusé la connexion (identifiant ou jeton incorrect)"));
        assert!(is_refusal("Pronote : erreur du serveur"));
        assert!(!is_refusal("connexion impossible : délai dépassé"));
        assert_eq!(refusal_msg("Pronote a bloqué ton adresse IP pour un moment (trop d'essais)."), SUSPENDED);
        assert_eq!(refusal_msg("Pronote a refusé la connexion"), REDO);
    }
}
