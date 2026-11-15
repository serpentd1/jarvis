//! API locale pour Jarvis : petit serveur HTTP sur 127.0.0.1 seulement (port 47800 par défaut).
//!   GET  /etat      → {"ok":true, "heure":…, "meteo":{…}, "pronote":{…}, "mails":[…], "minuteurs":[…]}
//!   POST /demander  {"texte":"…","source":"jarvis"} → {"reponse":"…"}
//!   POST /jarvis    {"etat":"repos|ecoute|reflechit|parle","texte":"…"} → {"ok":true}
//!   POST /notif     {"titre":"…","texte":"…","source":"…"} → {"ok":true}
//!   POST /minuteur {"secondes":300,"nom":"pâtes"} → {"ok":true,"id":N} ; POST /minuteur/annuler {"id":N} ou {"tous":true}
//!   POST /alarme {"heure":"07:30","active":true} ; POST /chrono {"commande":"demarrer|pause|zero"} → {"ok":true,"secondes":S}
//!   POST /ouvrir {"quoi":"pronote|mails|mail|devoirs","n":0} ; POST /meteo/actualiser {}
//!   GET  /notifs?depuis=ID → {"ok":true,"dernier":ID,"notifs":[…]}
//! Les requêtes venant d'une page web (en-tête Origin) sont refusées.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::channel;
use std::time::Duration;

use serde_json::{Value, json};

use crate::data::{Msg, Out};

pub enum ApiReq {
    Etat,
    Demander { texte: String, source: String },
    Jarvis { etat: String, texte: String, question: String, reponse: String },
    Notif { titre: String, texte: String, source: String },
    Timer { secs: f64, nom: String },
    TimerCancel { id: Option<u64>, tous: bool },
    Alarm { heure: String, active: bool },
    Chrono { commande: String },
    Open { quoi: String, n: usize },
    WeatherRefresh,
    Notifs { depuis: u64 },
    Mail { n: usize },
}

/// Lance le serveur. `port` = 0 : arrêté ; changer la valeur le redémarre sur le nouveau port.
pub fn spawn(out: Out, port: Arc<AtomicU32>) {
    let _ = std::thread::Builder::new().name("api".into()).spawn(move || {
        let mut cur = 0u32;
        let mut l: Option<TcpListener> = None;
        loop {
            let want = port.load(Ordering::Relaxed);
            if want != cur {
                l = None;
                cur = want;
                if want != 0 {
                    match TcpListener::bind(("127.0.0.1", want as u16)) {
                        Ok(x) => {
                            let _ = x.set_nonblocking(true);
                            crate::sys::log(&format!("API locale : http://127.0.0.1:{want}"));
                            l = Some(x);
                        }
                        Err(e) => crate::sys::log(&format!("API locale : port {want} indisponible ({e})")),
                    }
                }
            }
            match l.as_ref().map(|x| x.accept()) {
                Some(Ok((s, _))) => {
                    let o = out.clone();
                    std::thread::spawn(move || {
                        let _ = s.set_nonblocking(false);
                        handle(s, &o);
                    });
                }
                _ => std::thread::sleep(Duration::from_millis(if l.is_some() { 40 } else { 500 })),
            }
        }
    });
}

fn reply(mut s: TcpStream, code: u16, body: &str) {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body.as_bytes());
    let _ = s.flush();
}

fn err(msg: &str) -> String {
    json!({"ok": false, "erreur": msg}).to_string()
}

fn handle(s: TcpStream, out: &Out) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(rs) = s.try_clone() else { return };
    let mut r = BufReader::new(rs);
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
    let (mut len, mut origin, mut host_ok) = (0usize, false, true);
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        let (k, v) = h.split_once(':').unwrap_or((&h, ""));
        match k.trim().to_lowercase().as_str() {
            "content-length" => len = v.trim().parse().unwrap_or(0),
            "origin" => origin = true,
            "host" => {
                let host = v.trim().rsplit_once(':').map(|x| x.0).unwrap_or(v.trim()).to_lowercase();
                host_ok = matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]");
            }
            _ => {}
        }
    }
    // une page web ne doit pas pouvoir piloter Givre (protection contre les sites malveillants)
    if origin || !host_ok {
        return reply(s, 403, &err("requêtes venant d'un navigateur refusées"));
    }
    let mut body = vec![0u8; len.min(64 * 1024)];
    if len > 0 && r.read_exact(&mut body).is_err() {
        return reply(s, 400, &err("corps de requête incomplet"));
    }
    let j: Value = if body.is_empty() { json!({}) } else { serde_json::from_slice(&body).unwrap_or(Value::Null) };
    let txt = |k: &str| j[k].as_str().unwrap_or("").trim().to_string();
    let (path, query) = path.split_once('?').unwrap_or((&path, ""));
    let path = path.trim_end_matches('/').to_string();
    let depuis: u64 = query.split('&').find_map(|kv| kv.strip_prefix("depuis=")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let req = match (method.as_str(), path.as_str()) {
        ("GET", "/etat") => ApiReq::Etat,
        ("GET", "/notifs") => ApiReq::Notifs { depuis },
        ("GET", "/mail") => match query.split('&').find_map(|kv| kv.strip_prefix("n=")).and_then(|v| v.parse().ok()) {
            Some(n) => ApiReq::Mail { n },
            None => return reply(s, 400, &err("paramètre n manquant (/mail?n=0)")),
        },
        ("GET", "") => {
            return reply(s, 200, &json!({"ok": true, "nom": "Givre", "routes": ["GET /etat", "GET /notifs?depuis=ID", "GET /mail?n=N", "POST /demander", "POST /jarvis", "POST /notif", "POST /minuteur", "POST /minuteur/annuler", "POST /alarme", "POST /chrono", "POST /ouvrir", "POST /meteo/actualiser"]}).to_string());
        }
        ("POST", _) if j.is_null() => return reply(s, 400, &err("JSON invalide")),
        ("POST", "/demander") => {
            if txt("texte").is_empty() {
                return reply(s, 400, &err("« texte » manquant"));
            }
            ApiReq::Demander { texte: txt("texte"), source: txt("source") }
        }
        ("POST", "/jarvis") => ApiReq::Jarvis { etat: txt("etat"), texte: txt("texte"), question: txt("question"), reponse: txt("reponse") },
        ("POST", "/minuteur") => match j["secondes"].as_f64().or_else(|| j["minutes"].as_f64().map(|m| m * 60.0)) {
            Some(x) if x > 0.0 && x <= 86400.0 => ApiReq::Timer { secs: x, nom: txt("nom") },
            _ => return reply(s, 400, &err("« secondes » manquant ou invalide")),
        },
        ("POST", "/minuteur/annuler") => ApiReq::TimerCancel { id: j["id"].as_u64(), tous: j["tous"].as_bool().unwrap_or(false) },
        ("POST", "/alarme") => ApiReq::Alarm { heure: txt("heure"), active: j["active"].as_bool().unwrap_or(true) },
        ("POST", "/chrono") => ApiReq::Chrono { commande: txt("commande") },
        ("POST", "/ouvrir") => ApiReq::Open { quoi: txt("quoi").to_lowercase(), n: j["n"].as_u64().unwrap_or(0) as usize },
        ("POST", "/meteo/actualiser") => ApiReq::WeatherRefresh,
        ("POST", "/notif") => ApiReq::Notif { titre: txt("titre"), texte: txt("texte"), source: txt("source") },
        _ => return reply(s, 404, &err("route inconnue")),
    };
    let wait = match req {
        ApiReq::Demander { .. } => 150,
        ApiReq::Mail { .. } => 40,
        _ => 10,
    };
    let (tx, rx) = channel();
    out.send(Msg::Api(req, tx));
    match rx.recv_timeout(Duration::from_secs(wait)) {
        Ok((code, b)) => reply(s, code, &b),
        Err(_) => reply(s, 500, &err("Givre n'a pas répondu à temps")),
    }
}
