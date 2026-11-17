//! Comptes : coffre chiffré, Google (Agenda + Gmail), Microsoft (Hotmail), IA locale (Ollama).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration as StdDur, Instant};

use base64::Engine;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::data::{AccountsInfo, Cat, Event, Mail, MailBox, Msg, Out, enc};
use crate::sys;

// ---------- Coffre chiffré (DPAPI) ----------

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Secrets {
    pub google: Vec<Acct>,
    pub ms: Option<Acct>,
    pub discord: Option<Acct>,
    #[serde(default)]
    pub pronote: Option<crate::pronote::PAcct>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Acct {
    pub email: String,
    pub refresh: String,
    #[serde(default)]
    pub access: String,
    #[serde(default)]
    pub expires: i64,
}

fn vault_path() -> PathBuf {
    crate::config::dir().join("comptes.bin")
}

pub fn load() -> Secrets {
    std::fs::read(vault_path())
        .ok()
        .and_then(|b| sys::unprotect(&b))
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(s: &Secrets) {
    if let Ok(j) = serde_json::to_vec(s) {
        if let Some(b) = sys::protect(&j) {
            let _ = std::fs::write(vault_path(), b);
        }
    }
}

pub fn now_ts() -> i64 {
    Utc::now().timestamp()
}

// ---------- HTTP ----------

pub fn http_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(c, r) => {
            let b = r.into_string().unwrap_or_default();
            format!("erreur {c} : {}", b.chars().take(220).collect::<String>())
        }
        e => e.to_string(),
    }
}

pub fn post_form(ag: &ureq::Agent, url: &str, f: &[(&str, &str)]) -> Result<Value, String> {
    ag.post(url).send_form(f).map_err(http_err)?.into_json().map_err(|e| e.to_string())
}

fn get_json(ag: &ureq::Agent, url: &str, tok: &str) -> Result<Value, String> {
    ag.get(url)
        .set("Authorization", &format!("Bearer {tok}"))
        .call()
        .map_err(http_err)?
        .into_json()
        .map_err(|e| e.to_string())
}

fn b64url(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn html_unescape(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(j) = tail.find(';').filter(|j| *j < 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let ent = &tail[1..j];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=j]),
        }
        rest = &tail[j + 1..];
    }
    out.push_str(rest);
    out
}

fn sender_name(from: &str) -> String {
    let name = from.split('<').next().unwrap_or("").trim().trim_matches('"').trim();
    if name.is_empty() {
        from.trim_matches(|c| c == '<' || c == '>').to_string()
    } else {
        name.to_string()
    }
}

// ---------- Google ----------

const G_SCOPES: &str = "openid email https://www.googleapis.com/auth/calendar.events \
https://www.googleapis.com/auth/calendar.readonly https://www.googleapis.com/auth/calendar.app.created https://www.googleapis.com/auth/gmail.readonly";

pub struct GClient {
    id: String,
    secret: String,
}

fn parse_gclient(path: &std::path::Path) -> Option<GClient> {
    parse_gclient_str(&std::fs::read_to_string(path).ok()?)
}

fn parse_gclient_str(txt: &str) -> Option<GClient> {
    let v: Value = serde_json::from_str(txt.trim().trim_start_matches('\u{feff}')).ok()?;
    let o = v.get("installed").or_else(|| v.get("web"))?;
    Some(GClient { id: o["client_id"].as_str()?.into(), secret: o["client_secret"].as_str().unwrap_or("").into() })
}

/// Client Google intégré à la compilation (variable JARVIS_GOOGLE_CLIENT remplie par construire.ps1).
/// Jamais écrit dans le dépôt.
const EMBEDDED_GOOGLE: Option<&str> = option_env!("JARVIS_GOOGLE_CLIENT");

/// Message quand aucun client Google n'est disponible.
pub const NO_GOOGLE: &str = "Connexion Google pas disponible dans cette version";

/// Choix du client : le fichier google.json (prioritaire), sinon le client intégré.
fn pick_gclient(file: &std::path::Path, embedded: Option<&str>) -> Option<GClient> {
    parse_gclient(file).or_else(|| embedded.and_then(parse_gclient_str))
}

/// Identifiant d'appli Microsoft : celui des réglages, sinon celui intégré à la compilation (JARVIS_MS_CLIENT).
pub fn ms_client_id(cfg: &str) -> String {
    let c = cfg.trim();
    if !c.is_empty() {
        return c.to_string();
    }
    option_env!("JARVIS_MS_CLIENT").unwrap_or("").trim().to_string()
}

/// google.json dans le dossier des réglages ; sinon la clé téléchargée (client_secret_*.json)
/// dans Téléchargements ou à côté de Givre.exe, recopiée dans le dossier des réglages.
pub fn google_client() -> Option<GClient> {
    let main = crate::config::dir().join("google.json");
    if let Some(c) = pick_gclient(&main, EMBEDDED_GOOGLE) {
        return Some(c);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(up) = std::env::var_os("USERPROFILE") {
        dirs.push(PathBuf::from(up).join("Downloads"));
    }
    if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) {
        if let Some(pp) = d.parent() {
            dirs.push(pp.to_path_buf());
        }
        dirs.push(d);
    }
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if (n == "google.json" || n.starts_with("client_secret")) && n.ends_with(".json") {
                if let Some(t) = e.metadata().ok().and_then(|m| m.modified().ok()) {
                    found.push((t, e.path()));
                }
            }
        }
    }
    found.sort();
    for (_, p) in found.into_iter().rev() {
        if let Some(c) = parse_gclient(&p) {
            if std::fs::copy(&p, &main).is_ok() {
                sys::log(&format!("google.json repris depuis {}", p.display()));
            }
            return Some(c);
        }
    }
    None
}

/// Attend que le navigateur revienne sur http://127.0.0.1:PORT/?code=…
fn wait_code(l: &TcpListener, secs: u64) -> Result<String, String> {
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    let end = Instant::now() + StdDur::from_secs(secs);
    while Instant::now() < end {
        match l.accept() {
            Ok((mut s, _)) => {
                let _ = s.set_nonblocking(false);
                let _ = s.set_read_timeout(Some(StdDur::from_secs(5)));
                let mut buf = [0u8; 8192];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
                let q = path.split_once('?').map(|x| x.1).unwrap_or("").to_string();
                let get = |k: &str| {
                    q.split('&')
                        .find_map(|kv| kv.split_once('=').filter(|(a, _)| *a == k).map(|(_, b)| pct_decode(b)))
                };
                let (code, msg) = match (get("code"), get("error")) {
                    (Some(c), _) => (Some(c), "Jarvis est connecté. Tu peux fermer cet onglet."),
                    (_, Some(_)) => (None, "Connexion annulée. Tu peux fermer cet onglet."),
                    _ => {
                        let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        continue;
                    }
                };
                let body = format!(
                    "<!doctype html><meta charset=utf-8><title>Jarvis</title><body style=\"font-family:Segoe UI,sans-serif;\
                     background:#1e1e20;color:#eee;display:grid;place-items:center;height:90vh\"><h2>{msg}</h2></body>"
                );
                let _ = s.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                );
                return code.ok_or_else(|| "Connexion annulée.".into());
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(StdDur::from_millis(150)),
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Connexion : délai dépassé (5 min).".into())
}

pub fn google_add(ag: &ureq::Agent, vault: &Mutex<Secrets>) -> Result<String, String> {
    let c = google_client().ok_or(NO_GOOGLE)?;
    let l = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let redirect = format!("http://127.0.0.1:{}", l.local_addr().map_err(|e| e.to_string())?.port());
    let verifier = b64url(&sys::random(48));
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &code_challenge={challenge}&code_challenge_method=S256&access_type=offline&prompt=consent%20select_account",
        enc(&c.id),
        enc(&redirect),
        enc(G_SCOPES)
    );
    sys::open_url(&url);
    let code = wait_code(&l, 300)?;
    let v = post_form(
        ag,
        "https://oauth2.googleapis.com/token",
        &[
            ("code", &code),
            ("client_id", &c.id),
            ("client_secret", &c.secret),
            ("redirect_uri", &redirect),
            ("grant_type", "authorization_code"),
            ("code_verifier", &verifier),
        ],
    )?;
    let refresh = v["refresh_token"].as_str().ok_or("Google n'a pas renvoyé de jeton.")?.to_string();
    let access = v["access_token"].as_str().unwrap_or("").to_string();
    let info = get_json(ag, "https://openidconnect.googleapis.com/v1/userinfo", &access)?;
    let email = info["email"].as_str().unwrap_or("compte Google").to_string();
    let mut s = vault.lock().unwrap();
    s.google.retain(|a| a.email != email);
    s.google.push(Acct {
        email: email.clone(),
        refresh,
        access,
        expires: now_ts() + v["expires_in"].as_i64().unwrap_or(3000) - 60,
    });
    save(&s);
    Ok(email)
}

fn g_token(ag: &ureq::Agent, c: &GClient, a: &mut Acct) -> Result<String, String> {
    if !a.access.is_empty() && a.expires > now_ts() {
        return Ok(a.access.clone());
    }
    let v = post_form(
        ag,
        "https://oauth2.googleapis.com/token",
        &[("client_id", &c.id), ("client_secret", &c.secret), ("refresh_token", &a.refresh), ("grant_type", "refresh_token")],
    )?;
    a.access = v["access_token"].as_str().ok_or("jeton Google refusé")?.into();
    a.expires = now_ts() + v["expires_in"].as_i64().unwrap_or(3000) - 60;
    Ok(a.access.clone())
}

/// Jeton d'accès du premier compte Google (pour la copie Pronote → Google Agenda).
pub fn google_token(ag: &ureq::Agent, vault: &Mutex<Secrets>) -> Result<String, String> {
    let c = google_client().ok_or(NO_GOOGLE)?;
    let mut s = vault.lock().unwrap();
    let a = s.google.first_mut().ok_or("connecte d'abord un compte Google (Paramètres › Comptes)")?;
    let t = g_token(ag, &c, a)?;
    save(&s);
    Ok(t)
}

fn hex_color(s: &str) -> [u8; 3] {
    let h = s.trim_start_matches('#');
    let p = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok()).unwrap_or(128);
    if h.len() >= 6 { [p(0), p(2), p(4)] } else { [10, 132, 255] }
}

fn g_when(v: &Value) -> Option<(NaiveDateTime, bool)> {
    if let Some(dt) = v["dateTime"].as_str() {
        return DateTime::parse_from_rfc3339(dt).ok().map(|d| (d.with_timezone(&Local).naive_local(), false));
    }
    let d = NaiveDate::parse_from_str(v["date"].as_str()?, "%Y-%m-%d").ok()?;
    Some((d.and_hms_opt(0, 0, 0)?, true))
}

fn g_events(ag: &ureq::Agent, tok: &str) -> Result<Vec<Event>, String> {
    let cals = get_json(ag, "https://www.googleapis.com/calendar/v3/users/me/calendarList?minAccessRole=reader&maxResults=20", tok)?;
    let min = (Utc::now() - Duration::days(40)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let max = (Utc::now() + Duration::days(70)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut out = Vec::new();
    for cal in cals["items"].as_array().into_iter().flatten().take(10) {
        if cal["selected"].as_bool() != Some(true) && cal["primary"].as_bool() != Some(true) {
            continue;
        }
        let id = cal["id"].as_str().unwrap_or("primary");
        let color = hex_color(cal["backgroundColor"].as_str().unwrap_or("#0a84ff"));
        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events?singleEvents=true&orderBy=startTime&maxResults=250&timeMin={}&timeMax={}",
            enc(id),
            enc(&min),
            enc(&max)
        );
        let Ok(v) = get_json(ag, &url, tok) else { continue };
        for e in v["items"].as_array().into_iter().flatten() {
            if e["status"] == "cancelled" {
                continue;
            }
            let (Some((s, all_day)), Some((en, _))) = (g_when(&e["start"]), g_when(&e["end"])) else { continue };
            out.push(Event {
                start: s,
                end: en,
                all_day,
                title: e["summary"].as_str().unwrap_or("(sans titre)").into(),
                color,
            });
        }
    }
    Ok(out)
}

fn gmail(ag: &ureq::Agent, tok: &str, user: usize, cache: &mut HashMap<String, Mail>) -> Result<Vec<Mail>, String> {
    let v = get_json(
        ag,
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?q=is%3Aunread%20in%3Ainbox%20newer_than%3A14d&maxResults=15",
        tok,
    )?;
    let mut out = Vec::new();
    for m in v["messages"].as_array().into_iter().flatten() {
        let Some(id) = m["id"].as_str() else { continue };
        if let Some(c) = cache.get(id) {
            out.push(c.clone());
            continue;
        }
        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{id}?format=metadata&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=To&metadataHeaders=Cc"
        );
        let Ok(d) = get_json(ag, &url, tok) else { continue };
        let hdr = |n: &str| {
            d["payload"]["headers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|h| h["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n)))
                .and_then(|h| h["value"].as_str())
                .unwrap_or("")
                .to_string()
        };
        let ms = d["internalDate"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let (from, subject) = (sender_name(&hdr("From")), hdr("Subject"));
        let link = format!("https://mail.google.com/mail/u/{user}/#all/{id}");
        // mail Hotmail transféré vers Gmail : on garde de quoi le retrouver aussi dans Outlook.com
        let find = if to_microsoft(&format!("{} {}", hdr("To"), hdr("Cc"))) {
            if subject.trim().is_empty() { from.clone() } else { subject.trim().to_string() }
        } else {
            String::new()
        };
        let mail = Mail {
            id: id.into(),
            from,
            subject,
            snippet: html_unescape(d["snippet"].as_str().unwrap_or("")),
            date: DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&Local).naive_local()).unwrap_or_default(),
            important: d["labelIds"].as_array().is_some_and(|l| l.iter().any(|x| x == "IMPORTANT")),
            urgent: false,
            cat: Cat::None,
            link,
            find,
        };
        cache.insert(id.into(), mail.clone());
        out.push(mail);
    }
    Ok(out)
}

/// HTML → texte brut lisible (balises, styles et scripts retirés, entités décodées).
pub fn html_to_text(h: &str) -> String {
    let mut s = h.to_string();
    for tag in ["style", "script", "head"] {
        loop {
            let low = s.to_lowercase();
            let Some(a) = low.find(&format!("<{tag}")) else { break };
            let end = low[a..].find(&format!("</{tag}>")).map(|e| a + e + tag.len() + 3).unwrap_or(s.len());
            s.replace_range(a..end, " ");
        }
    }
    let mut out = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in s.chars() {
        match (in_tag, ch) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let t = tag.trim_start_matches('/').to_lowercase();
                if ["br", "p", "div", "tr", "li", "h1", "h2", "h3", "table"].iter().any(|x| t.starts_with(x)) {
                    out.push('\n');
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    clean_text(&html_unescape(&out))
}

/// Espaces et lignes vides en trop retirés.
fn clean_text(s: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for l in s.replace('\r', "").lines() {
        let l = l.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() && lines.last().is_none_or(|p| p.is_empty()) {
            continue;
        }
        lines.push(l);
    }
    lines.join("\n").trim().to_string()
}

fn b64url_decode(s: &str) -> Option<String> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace() && *c != '=').collect();
    let b = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(clean).ok()?;
    Some(String::from_utf8_lossy(&b).into_owned())
}

/// Cherche la première partie du type voulu dans un message Gmail (format=full).
fn gmail_part(p: &Value, mime: &str) -> Option<String> {
    if p["mimeType"].as_str().is_some_and(|m| m.eq_ignore_ascii_case(mime)) {
        if let Some(d) = p["body"]["data"].as_str() {
            return b64url_decode(d);
        }
    }
    p["parts"].as_array().into_iter().flatten().find_map(|x| gmail_part(x, mime))
}

/// Corps complet d'un mail en texte brut (8000 caractères au plus).
pub fn mail_body(ag: &ureq::Agent, vault: &Mutex<Secrets>, ms_client: &str, m: &Mail) -> Result<String, String> {
    let text = if m.link.contains("mail.google.com") {
        // n° du compte Gmail = /mail/u/N/ dans le lien
        let user: usize = m.link.split("/u/").nth(1).and_then(|r| r.split('/').next()).and_then(|n| n.parse().ok()).unwrap_or(0);
        let c = google_client().ok_or(NO_GOOGLE)?;
        let tok = {
            let mut s = vault.lock().unwrap();
            let a = s.google.get_mut(user).ok_or("compte Gmail introuvable")?;
            let t = g_token(ag, &c, a)?;
            save(&s);
            t
        };
        let d = get_json(ag, &format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full", m.id), &tok)?;
        match gmail_part(&d["payload"], "text/plain") {
            Some(t) if !t.trim().is_empty() => clean_text(&t),
            _ => gmail_part(&d["payload"], "text/html").map(|h| html_to_text(&h)).unwrap_or_else(|| html_unescape(d["snippet"].as_str().unwrap_or(""))),
        }
    } else {
        let tok = {
            let mut s = vault.lock().unwrap();
            let a = s.ms.as_mut().ok_or("compte Hotmail introuvable")?;
            let t = ms_token(ag, ms_client, a)?;
            save(&s);
            t
        };
        let v: Value = ag
            .get(&format!("https://graph.microsoft.com/v1.0/me/messages/{}?$select=body", m.id))
            .set("Authorization", &format!("Bearer {tok}"))
            .set("Prefer", "outlook.body-content-type=\"text\"")
            .call()
            .map_err(http_err)?
            .into_json()
            .map_err(|e| e.to_string())?;
        let body = v["body"]["content"].as_str().unwrap_or("");
        if v["body"]["contentType"].as_str() == Some("html") { html_to_text(body) } else { clean_text(body) }
    };
    Ok(text.chars().take(8000).collect())
}

#[cfg(test)]
mod tests_google {
    use super::*;

    #[test]
    fn client_integre_si_pas_de_fichier() {
        let json = r#"{"installed":{"client_id":"123-abc.apps.googleusercontent.com","client_secret":"secret-test"}}"#;
        let absent = std::path::Path::new("Z:/n/existe/pas/google.json");
        let c = pick_gclient(absent, Some(json)).expect("client intégré");
        assert_eq!(c.id, "123-abc.apps.googleusercontent.com");
        assert!(pick_gclient(absent, None).is_none());
        // le fichier reste prioritaire
        let dir = std::env::temp_dir().join(format!("jarvis_barre_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("google.json");
        std::fs::write(&f, r#"{"installed":{"client_id":"fichier","client_secret":"x"}}"#).unwrap();
        assert_eq!(pick_gclient(&f, Some(json)).unwrap().id, "fichier");
        let _ = std::fs::remove_dir_all(&dir);
        // compilé avec JARVIS_GOOGLE_CLIENT : le client intégré doit être lisible
        if let Some(e) = EMBEDDED_GOOGLE {
            assert!(parse_gclient_str(e).is_some(), "JARVIS_GOOGLE_CLIENT illisible");
        }
    }
}

#[cfg(test)]
mod tests_mail {
    #[test]
    fn html_en_texte() {
        let t = super::html_to_text("<html><head><style>p{}</style></head><body><p>Bonjour&nbsp;Lucas,</p><p>À <b>lundi</b> !</p><script>x()</script></body></html>");
        assert_eq!(t, "Bonjour Lucas,\n\nÀ lundi !");
        assert_eq!(super::b64url_decode("SGVsbG8gIQ").as_deref(), Some("Hello !"));
    }
}

pub fn google_create(
    ag: &ureq::Agent,
    vault: &Mutex<Secrets>,
    title: &str,
    date: NaiveDate,
    time: Option<NaiveTime>,
    dur: i64,
) -> Result<(), String> {
    let c = google_client().ok_or(NO_GOOGLE)?;
    let tok = {
        let mut s = vault.lock().unwrap();
        let a = s.google.first_mut().ok_or("Connecte d'abord un compte Google (Paramètres › Comptes).")?;
        let t = g_token(ag, &c, a)?;
        save(&s);
        t
    };
    let body = match time {
        Some(t) => {
            let st = date.and_time(t);
            let en = st + Duration::minutes(dur.max(5));
            let f = |d: NaiveDateTime| d.format("%Y-%m-%dT%H:%M:%S").to_string();
            json!({"summary": title, "start": {"dateTime": f(st), "timeZone": "Europe/Paris"},
                   "end": {"dateTime": f(en), "timeZone": "Europe/Paris"}})
        }
        None => json!({"summary": title, "start": {"date": date.to_string()},
                       "end": {"date": (date + Duration::days(1)).to_string()}}),
    };
    ag.post("https://www.googleapis.com/calendar/v3/calendars/primary/events")
        .set("Authorization", &format!("Bearer {tok}"))
        .send_json(body)
        .map_err(http_err)?;
    Ok(())
}

// ---------- Microsoft (Hotmail / Outlook) ----------

const MS_TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MS_SCOPES: &str = "offline_access User.Read Mail.Read";

pub fn ms_add(ag: &ureq::Agent, client: &str, vault: &Mutex<Secrets>, out: &Out) -> Result<String, String> {
    if client.trim().is_empty() {
        return Err("Entre d'abord l'ID d'application Microsoft (voir LISEZMOI).".into());
    }
    let v = post_form(
        ag,
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode",
        &[("client_id", client), ("scope", MS_SCOPES)],
    )?;
    let code = v["user_code"].as_str().ok_or("Microsoft n'a pas donné de code.")?;
    let uri = v["verification_uri"].as_str().unwrap_or("https://microsoft.com/link");
    let dev = v["device_code"].as_str().unwrap_or("");
    let mut iv = v["interval"].as_u64().unwrap_or(5);
    out.send(Msg::Status(format!("Hotmail : sur la page qui s'ouvre, tape le code {code}")));
    sys::open_url(uri);
    let end = Instant::now() + StdDur::from_secs(v["expires_in"].as_u64().unwrap_or(900));
    while Instant::now() < end {
        std::thread::sleep(StdDur::from_secs(iv));
        let r = ag.post(MS_TOKEN).send_form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client),
            ("device_code", dev),
        ]);
        match r {
            Ok(r) => {
                let t: Value = r.into_json().map_err(|e| e.to_string())?;
                let access = t["access_token"].as_str().unwrap_or("").to_string();
                let me = get_json(ag, "https://graph.microsoft.com/v1.0/me", &access)?;
                let email = me["mail"].as_str().or(me["userPrincipalName"].as_str()).unwrap_or("Hotmail").to_string();
                let mut s = vault.lock().unwrap();
                s.ms = Some(Acct {
                    email: email.clone(),
                    refresh: t["refresh_token"].as_str().unwrap_or("").into(),
                    access,
                    expires: now_ts() + t["expires_in"].as_i64().unwrap_or(3000) - 60,
                });
                save(&s);
                return Ok(email);
            }
            Err(ureq::Error::Status(_, r)) => {
                let e: Value = r.into_json().unwrap_or_default();
                match e["error"].as_str() {
                    Some("authorization_pending") => {}
                    Some("slow_down") => iv += 5,
                    other => {
                        return Err(format!(
                            "Hotmail : {}",
                            e["error_description"].as_str().unwrap_or(other.unwrap_or("erreur")).lines().next().unwrap_or("")
                        ));
                    }
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Hotmail : délai dépassé.".into())
}

fn ms_token(ag: &ureq::Agent, client: &str, a: &mut Acct) -> Result<String, String> {
    if !a.access.is_empty() && a.expires > now_ts() {
        return Ok(a.access.clone());
    }
    let v = post_form(
        ag,
        MS_TOKEN,
        &[("client_id", client), ("grant_type", "refresh_token"), ("refresh_token", &a.refresh), ("scope", MS_SCOPES)],
    )?;
    a.access = v["access_token"].as_str().ok_or("jeton Microsoft refusé")?.into();
    if let Some(r) = v["refresh_token"].as_str() {
        a.refresh = r.into();
    }
    a.expires = now_ts() + v["expires_in"].as_i64().unwrap_or(3000) - 60;
    Ok(a.access.clone())
}

pub const OUTLOOK: &str = "https://outlook.live.com/mail/0/";

/// Vrai si le mail était adressé à une boîte Microsoft (Hotmail, Outlook, Live, MSN).
fn to_microsoft(dest: &str) -> bool {
    let d = dest.to_lowercase();
    ["@hotmail.", "@outlook.", "@live.", "@msn."].iter().any(|x| d.contains(x))
}

fn outlook(ag: &ureq::Agent, tok: &str) -> Result<Vec<Mail>, String> {
    let v = get_json(
        ag,
        "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages?$filter=isRead%20eq%20false&$top=15\
         &$select=id,subject,from,receivedDateTime,bodyPreview,importance,webLink",
        tok,
    )?;
    Ok(v["value"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| Mail {
            id: m["id"].as_str().unwrap_or("").into(),
            from: m["from"]["emailAddress"]["name"]
                .as_str()
                .or(m["from"]["emailAddress"]["address"].as_str())
                .unwrap_or("?")
                .into(),
            subject: m["subject"].as_str().unwrap_or("").into(),
            snippet: m["bodyPreview"].as_str().unwrap_or("").chars().take(200).collect(),
            date: m["receivedDateTime"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Local).naive_local())
                .unwrap_or_default(),
            important: m["importance"] == "high",
            urgent: false,
            cat: Cat::None,
            link: m["webLink"].as_str().unwrap_or(OUTLOOK).into(),
            find: String::new(),
        })
        .collect())
}

// ---------- Tri et résumé ----------

/// Tri sans IA : règles simples sur l'objet, l'extrait et l'expéditeur.
fn rule_cat(m: &Mail) -> Cat {
    let t = format!("{} {} {}", m.subject, m.snippet, m.from).to_lowercase();
    const PROMO: [&str; 7] = ["newsletter", "promo", "offre", "réduction", "désabonner", "unsubscribe", "soldes"];
    const KW: [&str; 14] = [
        "urgent", "important", "rappel", "dernier délai", "date limite", "avant demain", "aujourd'hui", "convocation",
        "annulé", "annulation", "pronote", "collège", "absence", "alerte de sécurité",
    ];
    const AUTO: [&str; 4] = ["noreply", "no-reply", "ne-pas-repondre", "notification"];
    if PROMO.iter().any(|k| t.contains(k)) {
        Cat::Promo
    } else if KW.iter().any(|k| t.contains(k)) || m.important && t.contains('?') {
        Cat::Important
    } else if AUTO.iter().any(|k| t.contains(k)) {
        Cat::Notif
    } else {
        Cat::None
    }
}

/// Adresse d'Ollama : vide = ce PC ; on accepte « localhost:11434 » sans « http:// » et on retire le « / » final.
fn ollama_base(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    if u.is_empty() {
        "http://127.0.0.1:11434".into()
    } else if u.starts_with("http://") || u.starts_with("https://") {
        u.into()
    } else {
        format!("http://{u}")
    }
}

/// État de l'IA locale (affiché dans les paramètres).
/// Question posée à Ollama par l'île ou par Jarvis : historique court + consigne avec les données du moment.
/// Si le modèle demandé n'est pas installé, on prend le modèle de secours (celui des réglages mails).
pub fn ollama_ask(r: &crate::brain::AskReq) -> Result<String, String> {
    let base = ollama_base(&r.url);
    let installed: Vec<String> = ureq::AgentBuilder::new()
        .timeout(StdDur::from_secs(3))
        .build()
        .get(&format!("{base}/api/tags"))
        .call()
        .ok()
        .and_then(|x| x.into_json::<Value>().ok())
        .map(|v| v["models"].as_array().into_iter().flatten().filter_map(|m| m["name"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    let has = |m: &str| !m.is_empty() && installed.iter().any(|n| n == m || (!m.contains(':') && n.split(':').next() == Some(m)));
    let model = if has(&r.model) || installed.is_empty() {
        r.model.clone()
    } else if has(&r.fallback) {
        r.fallback.clone()
    } else {
        installed.first().cloned().unwrap_or_else(|| r.model.clone())
    };
    let mut msgs = vec![json!({"role": "system", "content": r.system})];
    for (q, a) in &r.history {
        msgs.push(json!({"role": "user", "content": q}));
        msgs.push(json!({"role": "assistant", "content": a}));
    }
    msgs.push(json!({"role": "user", "content": r.question}));
    let mut body = json!({
        "model": model, "stream": false, "keep_alive": "10m", "think": false,
        "options": {"temperature": r.temp, "num_ctx": 8192},
        "messages": msgs
    });
    if r.json {
        body["format"] = json!("json");
    }
    let ag = ureq::AgentBuilder::new().timeout_connect(StdDur::from_secs(4)).timeout(StdDur::from_secs(120)).build();
    let v: Value = ag
        .post(&format!("{base}/api/chat"))
        .send_json(body)
        .map_err(|e| ollama_err(e, &model, &base).0)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let a = crate::jarvis_link::strip_think(v["message"]["content"].as_str().unwrap_or(""));
    if a.is_empty() { Err("Ollama n'a rien répondu.".into()) } else { Ok(a) }
}

fn ollama_status(model: &str, url: &str) -> String {
    let base_url = ollama_base(url);
    let ag = ureq::AgentBuilder::new().timeout(StdDur::from_secs(3)).build();
    let Some(v) = ag.get(&format!("{base_url}/api/tags")).call().ok().and_then(|r| r.into_json::<Value>().ok()) else {
        return format!("Ollama ne répond pas à {base_url} (pas installé ou pas lancé : ollama.com). En attendant, Jarvis trie les mails avec des règles simples.");
    };
    let base = model.split(':').next().unwrap_or(model);
    let has = v["models"].as_array().into_iter().flatten().any(|m| {
        m["name"].as_str().is_some_and(|n| n == model || (!model.contains(':') && n.split(':').next() == Some(base)))
    });
    if has {
        format!("Ollama est prêt avec le modèle {model}.")
    } else {
        format!("Ollama est lancé, mais il manque le modèle : dans un terminal, tape « ollama pull {model} ».")
    }
}

/// Lit une catégorie écrite par le modèle (tolère majuscules, accents et petites variantes).
fn parse_cat(s: &str) -> Cat {
    let s = s.trim().to_lowercase();
    if s.contains("import") || s.contains("urgent") {
        Cat::Important
    } else if s.contains("répond") || s.contains("repond") || s.contains("reply") {
        Cat::Reply
    } else if s.contains("notif") {
        Cat::Notif
    } else if s.contains("promo") || s.contains("pub") || s.contains("news") {
        Cat::Promo
    } else if s.contains("spam") || s.contains("arnaque") || s.contains("phishing") {
        Cat::Spam
    } else {
        Cat::None
    }
}

/// Lit la réponse JSON du modèle : (résumé, catégorie par numéro de mail). Accepte l'ancien format {"urgents": [..]}.
fn parse_reply(content: &str, n: usize) -> Result<(String, HashMap<usize, Cat>), String> {
    let j: Value = serde_json::from_str(content).map_err(|e| format!("réponse de l'IA illisible ({e})"))?;
    let resume = j["resume"].as_str().unwrap_or("").trim().to_string();
    let mut cats = HashMap::new();
    for x in j["mails"].as_array().into_iter().flatten() {
        let idx = x["n"].as_u64().or_else(|| x["n"].as_str().and_then(|s| s.trim().parse().ok()));
        if let (Some(i), Some(c)) = (idx, x["cat"].as_str()) {
            if (i as usize) < n {
                cats.insert(i as usize, parse_cat(c));
            }
        }
    }
    for x in j["urgents"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()) {
        if (x as usize) < n {
            cats.entry(x as usize).or_insert(Cat::Important);
        }
    }
    if resume.is_empty() && cats.is_empty() {
        return Err("l'IA n'a rien répondu d'utile".into());
    }
    Ok((resume, cats))
}

/// Erreur d'Ollama expliquée simplement. `true` = Ollama injoignable (on réessaiera à la prochaine synchro).
fn ollama_err(e: ureq::Error, model: &str, base: &str) -> (String, bool) {
    match e {
        ureq::Error::Transport(_) => (format!("Ollama ne répond pas ({base}). Lance-le pour activer le tri par l'IA."), true),
        ureq::Error::Status(404, _) => (format!("Le modèle {model} n'est pas installé : « ollama pull {model} »."), false),
        e => (http_err(e), false),
    }
}

/// Demande à l'IA locale un résumé + une catégorie par mail. Le modèle est déchargé juste après.
/// En cas d'échec : (message simple, Ollama injoignable ?).
fn summarize(model: &str, url: &str, mails: &[Mail]) -> Result<(String, HashMap<usize, Cat>), (String, bool)> {
    let base = ollama_base(url);
    let list = mails
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let flag = if m.important { " | marqué important par la messagerie" } else { "" };
            let subject: String = m.subject.chars().take(120).collect();
            let from: String = m.from.chars().take(60).collect();
            format!("[{i}] De : {from} | Objet : {subject}{flag} | Extrait : {}", m.snippet.chars().take(200).collect::<String>())
        })
        .collect::<Vec<_>>()
        .join("\n");
    let system = "Tu aides un collégien français à trier ses mails non lus. Réponds uniquement en JSON, sans autre texte :\n\
{\"resume\": \"2 ou 3 phrases courtes et simples en français\", \"mails\": [{\"n\": 0, \"cat\": \"important\"}, ...]}\n\
Mets exactement un élément dans « mails » pour chaque mail, avec son numéro « n » et une « cat » parmi :\n\
- \"important\" : demande une action rapide, vient de l'école (Pronote, collège, professeurs), de la famille ou d'un proche, ou annonce un changement ou un danger (compte piraté, alerte de sécurité).\n\
- \"repondre\" : une vraie personne attend une réponse, mais ce n'est pas pressé.\n\
- \"notification\" : message automatique utile (confirmation, reçu, expédition, réseau social, jeu, application).\n\
- \"promo\" : publicité, newsletter, soldes, offre commerciale.\n\
- \"spam\" : arnaque, hameçonnage, expéditeur louche, gain ou cadeau inattendu.\n\
En cas de doute entre deux catégories, choisis la moins alarmante. Le résumé parle seulement des mails \"important\" et \"repondre\" ; \
s'il n'y en a aucun, écris que rien n'est urgent.";
    let body = json!({
        "model": model, "stream": false, "format": "json", "keep_alive": 0,
        "options": {"temperature": 0.1},
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": list}
        ]
    });
    let ag = ureq::AgentBuilder::new().timeout_connect(StdDur::from_secs(4)).timeout(StdDur::from_secs(240)).build();
    let r: Value = ag
        .post(&format!("{base}/api/chat"))
        .send_json(body)
        .map_err(|e| ollama_err(e, model, &base))?
        .into_json()
        .map_err(|e| (e.to_string(), false))?;
    let content = r["message"]["content"].as_str().unwrap_or("{}");
    parse_reply(content, mails.len()).map_err(|e| (e, false))
}

pub struct SyncCfg {
    pub ms_client: String,
    pub model: String,
    pub ollama_url: String,
    pub ai: bool,
}

#[derive(Default)]
pub struct State {
    cache: HashMap<String, Mail>,
    last_key: String,
    last_summary: String,
    last_cats: HashMap<String, Cat>,
    last_error: String,
    notified: HashSet<String>,
    primed: bool,
}

pub fn info(vault: &Mutex<Secrets>, model: &str, url: &str) -> AccountsInfo {
    let s = vault.lock().unwrap().clone();
    AccountsInfo {
        google: s.google.iter().map(|a| a.email.clone()).collect(),
        ms: s.ms.map(|a| a.email),
        google_json: google_client().is_some(),
        ai: ollama_status(model, url),
    }
}

/// Récupère agenda + mails de tous les comptes, trie, résume, et prévient pour les nouveaux mails urgents.
pub fn sync(ag: &ureq::Agent, vault: &Mutex<Secrets>, st: &mut State, cfg: &SyncCfg, out: &Out) {
    let gc = google_client();
    let mut s = vault.lock().unwrap().clone();
    let (mut events, mut mails, mut errs) = (Vec::new(), Vec::new(), Vec::new());
    // n° du compte = ordre d'ajout ; Gmail l'utilise dans ses adresses (/mail/u/0/, /mail/u/1/…)
    for (user, a) in s.google.iter_mut().enumerate() {
        let Some(c) = &gc else {
            errs.push(NO_GOOGLE.to_string());
            break;
        };
        match g_token(ag, c, a) {
            Ok(tok) => {
                match g_events(ag, &tok) {
                    Ok(e) => events.extend(e),
                    Err(e) => errs.push(format!("agenda {} : {e}", a.email)),
                }
                match gmail(ag, &tok, user, &mut st.cache) {
                    Ok(m) => mails.extend(m),
                    Err(e) => errs.push(format!("gmail {} : {e}", a.email)),
                }
            }
            Err(e) => errs.push(format!("google {} : {e}", a.email)),
        }
    }
    if let Some(a) = s.ms.as_mut() {
        match ms_token(ag, &cfg.ms_client, a) {
            Ok(tok) => match outlook(ag, &tok) {
                Ok(m) => mails.extend(m),
                Err(e) => errs.push(format!("hotmail : {e}")),
            },
            Err(e) => errs.push(format!("hotmail : {e}")),
        }
    }
    {
        // on remet les jetons rafraîchis dans le coffre (si les comptes n'ont pas changé entre-temps)
        let mut v = vault.lock().unwrap();
        let same = v.google.iter().map(|a| &a.email).eq(s.google.iter().map(|a| &a.email))
            && v.ms.as_ref().map(|a| &a.email) == s.ms.as_ref().map(|a| &a.email);
        if same {
            v.google = s.google.clone();
            v.ms = s.ms.clone();
            save(&v);
        }
    }
    if !errs.is_empty() {
        sys::log(&errs.join(" | "));
    }
    let accounts = s.google.len() + s.ms.is_some() as usize;
    if !s.google.is_empty() {
        out.send(Msg::Events(events));
    }
    mails.sort_by(|a, b| b.date.cmp(&a.date));
    for m in &mut mails {
        m.cat = rule_cat(m);
        m.urgent = m.cat == Cat::Important;
    }
    let mut summary = String::new();
    let mut ai = false;
    if cfg.ai && !mails.is_empty() {
        let key = mails.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(",");
        if key != st.last_key {
            out.send(Msg::Mail(MailBox { mails: mails.clone(), summary: "Tri par l'IA en cours…".into(), ai: true, accounts }));
            st.last_key = key;
            st.last_summary.clear();
            st.last_cats.clear();
            st.last_error.clear();
            // les mails au-delà du 20e ne sont pas envoyés à l'IA : ils gardent le tri par règles
            let n = mails.len().min(20);
            match summarize(&cfg.model, &cfg.ollama_url, &mails[..n]) {
                Ok((sum, cats)) => {
                    st.last_summary = sum;
                    st.last_cats = cats.iter().filter_map(|(i, c)| mails.get(*i).map(|m| (m.id.clone(), *c))).collect();
                }
                Err((e, down)) => {
                    sys::log(&format!("ollama : {e}"));
                    st.last_error = e;
                    if down {
                        st.last_key.clear(); // Ollama éteint : on réessaiera à la prochaine synchro
                    }
                }
            }
        }
        if !st.last_cats.is_empty() || !st.last_summary.is_empty() {
            summary = st.last_summary.clone();
            ai = true;
            for m in &mut mails {
                if let Some(c) = st.last_cats.get(&m.id) {
                    m.cat = *c;
                    m.urgent = *c == Cat::Important;
                }
            }
        } else if !st.last_error.is_empty() {
            summary = format!("{} Tri par règles simples en attendant.", st.last_error);
        }
    }
    for m in mails.iter().filter(|m| m.urgent) {
        if st.notified.insert(m.id.clone()) && st.primed {
            out.send(Msg::Notify(format!("Mail urgent — {}", m.from), m.subject.clone(), m.link.clone(), m.find.clone()));
        }
    }
    st.primed = true;
    out.send(Msg::Mail(MailBox { mails, summary, ai, accounts }));
}

#[cfg(test)]
mod tests_ia {
    use super::*;

    #[test]
    fn reponse_ia_nouveau_et_ancien_format() {
        let (r, c) = parse_reply(r#"{"resume":"ok","mails":[{"n":0,"cat":"Important"},{"n":"1","cat":"promo"},{"n":9,"cat":"spam"}]}"#, 3).unwrap();
        assert_eq!(r, "ok");
        assert_eq!(c.get(&0), Some(&Cat::Important));
        assert_eq!(c.get(&1), Some(&Cat::Promo));
        assert_eq!(c.get(&9), None);
        let (_, c) = parse_reply(r#"{"resume":"x","urgents":[2]}"#, 3).unwrap();
        assert_eq!(c.get(&2), Some(&Cat::Important));
        assert!(parse_reply("pas du json", 3).is_err());
        assert!(parse_reply("{}", 3).is_err());
    }

    #[test]
    fn adresse_ollama() {
        assert_eq!(ollama_base(""), "http://127.0.0.1:11434");
        assert_eq!(ollama_base(" localhost:11434/ "), "http://localhost:11434");
        assert_eq!(ollama_base("https://x.fr"), "https://x.fr");
    }
}
