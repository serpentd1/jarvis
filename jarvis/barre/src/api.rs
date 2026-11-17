//! API locale pour Jarvis : petit serveur HTTP sur 127.0.0.1 seulement (port 47802 par défaut).
//!   GET  /etat      → {"ok":true, "heure":…, "meteo":{…}, "pronote":{…}, "mails":[…], "minuteurs":[…]}
//!   POST /demander  {"texte":"…","source":"jarvis"} → {"reponse":"…"}
//!   POST /jarvis    {"etat":"repos|ecoute|reflechit|parle","texte":"…","duree":2.5} → {"ok":true}
//!                   (duree = secondes que dure la phrase dite : l'île l'affiche mot par mot)
//!   POST /notif     {"titre":"…","texte":"…","source":"…"} → {"ok":true}
//!   POST /minuteur {"secondes":300,"nom":"pâtes"} → {"ok":true,"id":N} ; POST /minuteur/annuler {"id":N} ou {"tous":true}
//!   POST /alarme {"heure":"07:30","active":true} ; POST /chrono {"commande":"demarrer|pause|zero"} → {"ok":true,"secondes":S}
//!   POST /ouvrir {"quoi":"pronote|mails|mail|devoirs","n":0} ; POST /meteo/actualiser {}
//!   GET  /notifs?depuis=ID → {"ok":true,"dernier":ID,"notifs":[…]}
//!   POST /cerveau {"texte":"…","source":"voix","coupe":"…","reglages":{…}} → une ligne JSON par phrase prête
//!        {"phrase":"…"} puis {"fin":true,"reponse":"…","reste":"…","suivi":false} ; POST /cerveau/oublier {}
//!   POST /ia/discuter {"messages":[…],"temperature":0.4,"max_mots":300,"json":false,"outils":[]} → {"ok":true,"texte":"…","appels":[[nom,args]],"fournisseur":"LM Studio · modèle"} ou {"ok":false,"erreur":"…"}
//!        (les messages peuvent avoir "images":[base64])
//!   GET  /ia/etat → les services (sans les clés) ; POST /ia/cle {"cle":"…","adresse":""} → {"ok":true|false,"message":"…"}
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
    /// `duree` : secondes que met Jarvis à dire `texte` (affiché mot par mot dans l'île), 0 = inconnu.
    Jarvis { etat: String, texte: String, question: String, reponse: String, duree: f64 },
    Notif { titre: String, texte: String, source: String },
    Timer { secs: f64, nom: String },
    TimerCancel { id: Option<u64>, tous: bool },
    Alarm { heure: String, active: bool },
    Chrono { commande: String },
    Open { quoi: String, n: usize, section: String },
    WeatherRefresh,
    Notifs { depuis: u64 },
    Mail { n: usize },
    /// Jarvis s'éteint (« éteins-toi », menu Quitter) : l'appli entière s'arrête avec lui.
    Quit,
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

/// Le cerveau répond en direct : une ligne JSON par phrase, puis la ligne de fin.
fn cerveau_flux(mut s: TcpStream, j: &Value) {
    let texte = j["texte"].as_str().unwrap_or("").trim().to_string();
    if texte.is_empty() {
        return reply(s, 400, &err("« texte » manquant"));
    }
    let _ = s.write_all(b"HTTP/1.1 200 OK
Content-Type: application/x-ndjson; charset=utf-8
Connection: close

");
    let d = crate::cerveau::Demande {
        texte,
        source: j["source"].as_str().unwrap_or("voix").to_string(),
        coupe: j["coupe"].as_str().filter(|c| !c.is_empty()).map(str::to_string),
        python: j["python"].as_bool().unwrap_or(true),
        contexte: None,
        reglages: crate::cerveau::Reglages::depuis_json(&j["reglages"]),
    };
    let r = crate::cerveau::repondre(d, &mut |p| {
        let _ = s.write_all(format!("{}
", json!({"phrase": p})).as_bytes());
        let _ = s.flush();
    });
    let _ = s.write_all(format!("{}
", json!({"fin": true, "reponse": r.texte, "reste": r.reste, "suivi": r.suivi})).as_bytes());
    let _ = s.flush();
}

/// POST /ia/discuter : une question à l'IA (le service qui répond le premier, le gratuit d'abord).
fn ia_discuter(j: &Value) -> Value {
    let messages: Vec<Value> = j["messages"].as_array().cloned().unwrap_or_default();
    if messages.is_empty() {
        return json!({"ok": false, "erreur": "« messages » manquant"});
    }
    let req = crate::ia::Requete {
        messages,
        outils: j["outils"].as_array().cloned().unwrap_or_default(),
        max_mots: j["max_mots"].as_u64().unwrap_or(300) as u32,
        temperature: j["temperature"].as_f64().unwrap_or(0.4) as f32,
        json: j["json"].as_bool().unwrap_or(false),
        rapide: false,
        contexte: 8192,
        garder: "30m".into(),
    };
    match crate::ia::discuter(&req, None) {
        Ok(x) => {
            let appels: Vec<Value> = x.appels.iter().map(|(nom, args)| json!([nom, args])).collect();
            json!({"ok": true, "texte": x.contenu, "appels": appels, "fournisseur": crate::ia::actif().unwrap_or_default()})
        }
        Err(crate::ia::Echec::Injoignable) => json!({"ok": false, "erreur": "Je n'arrive à joindre aucune IA."}),
        Err(crate::ia::Echec::Autre(t)) => json!({"ok": false, "erreur": t}),
    }
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
    // une page web ne doit pas pouvoir piloter l'application (protection contre les sites malveillants)
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
            return reply(s, 200, &json!({"ok": true, "nom": "Jarvis", "routes": ["GET /etat", "GET /notifs?depuis=ID", "GET /mail?n=N", "POST /demander", "POST /jarvis", "POST /notif", "POST /minuteur", "POST /minuteur/annuler", "POST /alarme", "POST /chrono", "POST /ouvrir", "POST /meteo/actualiser", "POST /quitter", "POST /cerveau", "POST /cerveau/oublier", "POST /ia/discuter", "GET /ia/etat", "POST /ia/cle", "GET /outils", "POST /outil", "POST /outil/confirmer"]}).to_string());
        }
        ("POST", _) if j.is_null() => return reply(s, 400, &err("JSON invalide")),
        ("POST", "/demander") => {
            if txt("texte").is_empty() {
                return reply(s, 400, &err("« texte » manquant"));
            }
            ApiReq::Demander { texte: txt("texte"), source: txt("source") }
        }
        ("POST", "/jarvis") => ApiReq::Jarvis { etat: txt("etat"), texte: txt("texte"), question: txt("question"), reponse: txt("reponse"), duree: j["duree"].as_f64().unwrap_or(0.0) },
        ("POST", "/minuteur") => match j["secondes"].as_f64().or_else(|| j["minutes"].as_f64().map(|m| m * 60.0)) {
            Some(x) if x > 0.0 && x <= 86400.0 => ApiReq::Timer { secs: x, nom: txt("nom") },
            _ => return reply(s, 400, &err("« secondes » manquant ou invalide")),
        },
        ("POST", "/minuteur/annuler") => ApiReq::TimerCancel { id: j["id"].as_u64(), tous: j["tous"].as_bool().unwrap_or(false) },
        ("POST", "/alarme") => ApiReq::Alarm { heure: txt("heure"), active: j["active"].as_bool().unwrap_or(true) },
        ("POST", "/chrono") => ApiReq::Chrono { commande: txt("commande") },
        ("POST", "/ouvrir") => ApiReq::Open { quoi: txt("quoi").to_lowercase(), n: j["n"].as_u64().unwrap_or(0) as usize, section: txt("section") },
        ("POST", "/meteo/actualiser") => ApiReq::WeatherRefresh,
        ("POST", "/notif") => ApiReq::Notif { titre: txt("titre"), texte: txt("texte"), source: txt("source") },
        ("POST", "/quitter") => ApiReq::Quit,
        // le cerveau (cerveau.rs) : traité dans le fil de la requête, la réponse part phrase par phrase
        ("POST", "/cerveau") => return cerveau_flux(s, &j),
        // les fournisseurs d'IA (ia.rs) : discussion, état, ajout d'une clé
        ("POST", "/ia/discuter") => return reply(s, 200, &ia_discuter(&j).to_string()),
        ("GET", "/ia/etat") => return reply(s, 200, &crate::ia::etat_json().to_string()),
        ("POST", "/ia/cle") => {
            let (ok, message) = match crate::ia::ajouter_cle(&txt("cle"), &txt("adresse")) {
                Ok(m) => (true, m),
                Err(m) => (false, m),
            };
            return reply(s, 200, &json!({"ok": ok, "message": message}).to_string());
        }
        ("POST", "/cerveau/oublier") => {
            crate::cerveau::oublier();
            return reply(s, 200, &json!({"ok": true}).to_string());
        }
        // pouvoirs sur le PC (fichiers, PowerShell, programmes…) : traités ici, dans le fil de la requête,
        // pour ne pas bloquer l'île pendant une commande longue
        ("GET", "/outils") => return reply(s, 200, &json!({"ok": true, "outils": crate::pouvoirs::outils(), "regles": crate::pouvoirs::regles()}).to_string()),
        ("POST", "/outil") => return reply(s, 200, &crate::pouvoirs::appeler(&txt("nom"), &j["args"]).json().to_string()),
        ("POST", "/outil/confirmer") => {
            let Some(id) = j["id"].as_u64() else { return reply(s, 400, &err("« id » manquant")) };
            return reply(s, 200, &crate::pouvoirs::confirmer(id, j["oui"].as_bool().unwrap_or(false)).json().to_string());
        }
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
        Err(_) => reply(s, 500, &err("Jarvis n'a pas répondu à temps")),
    }
}
