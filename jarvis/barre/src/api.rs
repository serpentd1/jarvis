//! API locale pour Jarvis : petit serveur HTTP sur 127.0.0.1 seulement (port 47802 par défaut).
//!   GET  /etat      → {"ok":true, "heure":…, "meteo":{…}, "pronote":{…}, "mails":[…], "minuteurs":[…]}
//!   POST /demander  {"texte":"…","source":"jarvis"} → {"reponse":"…"}
//!   POST /jarvis    {"etat":"repos|ecoute|parole|reflechit|parle|parle_ecoute","texte":"…","duree":2.5,"toi":"…"} → {"ok":true}
//!                   (toi = la phrase de l'utilisateur, affichée au-dessus de la réponse de Jarvis)
//!                   (duree = secondes que dure la phrase dite : l'île l'affiche mot par mot)
//!   POST /notif     {"titre":"…","texte":"…","source":"…"} → {"ok":true}
//!   POST /minuteur {"secondes":300,"nom":"pâtes"} → {"ok":true,"id":N} ; POST /minuteur/annuler {"id":N} ou {"tous":true}
//!   POST /alarme {"heure":"07:30","active":true} ; POST /chrono {"commande":"demarrer|pause|zero"} → {"ok":true,"secondes":S}
//!   POST /ouvrir {"quoi":"pronote|mails|mail|devoirs","n":0} ; POST /meteo/actualiser {}
//!   GET  /notifs?depuis=ID → {"ok":true,"dernier":ID,"notifs":[…]}
//!   POST /cerveau {"texte":"…","source":"voix","coupe":"…","pas_dit":"…","reglages":{…}} → une ligne JSON par phrase prête
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
    Jarvis { etat: String, texte: String, question: String, reponse: String, duree: f64, toi: String },
    Notif { titre: String, texte: String, source: String },
    Timer { secs: f64, nom: String },
    TimerCancel { id: Option<u64>, tous: bool },
    Alarm { heure: String, active: bool },
    Chrono { commande: String },
    Open { quoi: String, n: usize, section: String },
    WeatherRefresh,
    /// PAI : « je ne vais pas en maths demain » (sauter) ou « finalement j'y vais ».
    Pai { jour: String, matiere: String, heure: String, sauter: bool },
    /// « J'ai fini mes devoirs pour demain » : coche les devoirs Pronote de ce jour (et de cette matière).
    DevoirsFaits { jour: String, matiere: String },
    /// « Renomme le minuteur en pâtes » : renomme le dernier minuteur lancé.
    TimerRename { nom: String },
    /// « Passe sur le plus rapide » : change le modèle de l'IA (voir profils::mode_depuis).
    IaModele { choix: String },
    /// Ne pas déranger de l'île (mode concentration de Jarvis) : `on` = None renvoie juste l'état.
    Dnd { on: Option<bool> },
    Notifs { depuis: u64 },
    Mail { n: usize },
    /// Imprimante 3D : « etat », « pause », « reprendre », « arreter », « lumiere » (allumer / éteindre).
    Imprimante { action: String },
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
    // « chunked » : chaque phrase part tout de suite (sinon le client attend un bloc plein avant de lire)
    let _ = s.set_nodelay(true);
    let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson; charset=utf-8\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
    let d = crate::cerveau::Demande {
        texte,
        source: j["source"].as_str().unwrap_or("voix").to_string(),
        coupe: j["coupe"].as_str().filter(|c| !c.is_empty()).map(str::to_string),
        pas_dit: j["pas_dit"].as_str().filter(|c| !c.trim().is_empty()).map(str::to_string),
        python: j["python"].as_bool().unwrap_or(true),
        contexte: None,
        reglages: crate::cerveau::Reglages::depuis_json(&j["reglages"]),
    };
    // false = la voix a fermé la connexion (on lui a coupé la parole) : le cerveau s'arrête.
    let r = crate::cerveau::repondre(d, &mut |p| ligne(&mut s, &json!({"phrase": p})));
    ligne(&mut s, &json!({"fin": true, "reponse": r.texte, "reste": r.reste, "suivi": r.suivi}));
    let _ = s.write_all(b"0\r\n\r\n");
    let _ = s.flush();
}

/// Une ligne JSON, envoyée comme un morceau (« chunk ») HTTP. false = le client a fermé la connexion.
fn ligne(s: &mut TcpStream, v: &Value) -> bool {
    let l = format!("{v}\n");
    s.write_all(format!("{:x}\r\n{l}\r\n", l.len()).as_bytes()).is_ok() && s.flush().is_ok()
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
    let modele = j["modele"].as_str().unwrap_or("").to_string();
    match crate::ia::avec_modele(&modele, || crate::ia::discuter(&req, None)) {
        Ok(x) => {
            let appels: Vec<Value> = x.appels.iter().map(|(nom, args)| json!([nom, args])).collect();
            json!({"ok": true, "texte": x.contenu, "appels": appels, "fournisseur": crate::ia::actif().unwrap_or_default()})
        }
        Err(crate::ia::Echec::Injoignable) => json!({"ok": false, "erreur": "Je n'arrive à joindre aucune IA."}),
        Err(crate::ia::Echec::Autre(t)) => json!({"ok": false, "erreur": t}),
    }
}

/// Taille maximale du corps d'une requête. /ia/discuter reçoit des images en base64 (capture de l'écran : souvent
/// plus d'un Mo) ; le reste n'envoie que du texte court.
fn taille_max(chemin: &str) -> usize {
    let chemin = chemin.split('?').next().unwrap_or("").trim_end_matches('/');
    if chemin == "/ia/discuter" { 32 * 1024 * 1024 } else { 64 * 1024 }
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
    // Avant, le corps était coupé à 64 Ko sans rien dire : une capture d'écran arrivait tronquée (« JSON invalide »).
    if len > taille_max(&path) {
        return reply(s, 413, &err("requête trop grosse"));
    }
    let mut body = vec![0u8; len];
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
            return reply(s, 200, &json!({"ok": true, "nom": "Jarvis", "routes": ["GET /etat", "GET /notifs?depuis=ID", "GET /mail?n=N", "POST /demander", "POST /jarvis", "POST /notif", "POST /minuteur", "POST /minuteur/annuler", "POST /alarme", "POST /chrono", "POST /ouvrir", "POST /meteo/actualiser", "POST /ne-pas-deranger", "POST /quitter", "POST /cerveau", "POST /cerveau/oublier", "POST /ia/discuter", "GET /ia/etat", "POST /ia/cle", "GET /outils", "POST /outil", "POST /outil/confirmer"]}).to_string());
        }
        ("POST", _) if j.is_null() => return reply(s, 400, &err("JSON invalide")),
        ("POST", "/demander") => {
            if txt("texte").is_empty() {
                return reply(s, 400, &err("« texte » manquant"));
            }
            ApiReq::Demander { texte: txt("texte"), source: txt("source") }
        }
        ("POST", "/jarvis") => ApiReq::Jarvis { etat: txt("etat"), texte: txt("texte"), question: txt("question"), reponse: txt("reponse"), duree: j["duree"].as_f64().unwrap_or(0.0), toi: txt("toi") },
        ("POST", "/minuteur") => match j["secondes"].as_f64().or_else(|| j["minutes"].as_f64().map(|m| m * 60.0)) {
            Some(x) if x > 0.0 && x <= 86400.0 => ApiReq::Timer { secs: x, nom: txt("nom") },
            _ => return reply(s, 400, &err("« secondes » manquant ou invalide")),
        },
        ("POST", "/minuteur/annuler") => ApiReq::TimerCancel { id: j["id"].as_u64(), tous: j["tous"].as_bool().unwrap_or(false) },
        ("POST", "/alarme") => ApiReq::Alarm { heure: txt("heure"), active: j["active"].as_bool().unwrap_or(true) },
        ("POST", "/chrono") => ApiReq::Chrono { commande: txt("commande") },
        ("POST", "/ouvrir") => ApiReq::Open { quoi: txt("quoi").to_lowercase(), n: j["n"].as_u64().unwrap_or(0) as usize, section: txt("section") },
        ("POST", "/meteo/actualiser") => ApiReq::WeatherRefresh,
        ("POST", "/ia/modele") => ApiReq::IaModele { choix: txt("choix") },
        ("POST", "/pronote/devoirs-faits") => ApiReq::DevoirsFaits { jour: txt("jour"), matiere: txt("matiere") },
        ("POST", "/minuteur/renommer") => ApiReq::TimerRename { nom: txt("nom") },
        ("POST", "/pronote/pai") => ApiReq::Pai { jour: txt("jour"), matiere: txt("matiere"), heure: txt("heure"), sauter: j["y_va"].as_bool() != Some(true) },
        ("POST", "/ne-pas-deranger") => ApiReq::Dnd { on: j["actif"].as_bool() },
        ("POST", "/notif") => ApiReq::Notif { titre: txt("titre"), texte: txt("texte"), source: txt("source") },
        ("POST", "/imprimante") => ApiReq::Imprimante { action: txt("action") },
        ("GET", "/imprimante") => ApiReq::Imprimante { action: "etat".into() },
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

#[cfg(test)]
mod tests {
    use super::taille_max;

    #[test]
    fn une_capture_d_ecran_passe_par_ia_discuter() {
        // une capture d'écran 1280 px en PNG base64 pèse souvent 1 à 3 Mo
        assert!(taille_max("/ia/discuter") >= 8 * 1024 * 1024);
        assert!(taille_max("/ia/discuter/") >= 8 * 1024 * 1024);
        assert_eq!(taille_max("/notif"), 64 * 1024);
        assert_eq!(taille_max("/cerveau"), 64 * 1024);
    }
}
