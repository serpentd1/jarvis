//! Liaison avec le téléphone Android (appli « Jarvis Téléphone », dossier appli-telephone), en Wi-Fi local.
//!
//! - Le PC écoute sur le port 47803, mais n'accepte QUE les adresses du réseau local (192.168.x, 10.x,
//!   172.16-31.x…) : rien n'est ouvert sur Internet. Le pare-feu de Windows demande l'accord une fois.
//! - Appairage : « Relier un téléphone… » dans le menu de l'icône donne un code à 6 chiffres (valable 5 min,
//!   5 essais). Le téléphone l'envoie une fois et reçoit un JETON secret (256 bits) ; le PC n'en garde que
//!   l'empreinte SHA-256 (%APPDATA%\Jarvis\barre\telephone.json). Chaque requête porte ce jeton.
//! - Le téléphone envoie ses notifications (POST /tel/notif) et ses questions (POST /tel/demander) ;
//!   il attend les ordres du PC en « long polling » (GET /tel/attendre) : alarme, minuteur, message, sonner.
//! - Limite honnête : le trafic est en HTTP simple (pas chiffré) sur ton Wi-Fi ; le jeton empêche un autre
//!   appareil de piloter Jarvis, mais quelqu'un qui espionne ton Wi-Fi pourrait lire les messages.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::data::{Msg, Out};

pub const PORT: u16 = 47803;
const CODE_VALIDE: Duration = Duration::from_secs(300);
const ATTENTE_MAX: Duration = Duration::from_secs(25);

#[derive(Serialize, Deserialize, Clone, Default)]
struct Appareil {
    nom: String,
    /// SHA-256 du jeton, en hexadécimal.
    empreinte: String,
    relie_le: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Fichier {
    appareils: Vec<Appareil>,
}

#[derive(Default)]
struct Etat {
    charge: bool,
    appareils: Vec<Appareil>,
    code: Option<(String, Instant, u32)>,
    commandes: Vec<(u64, Value)>,
    suivant: u64,
    vu: Option<Instant>,
    /// Dernières notifications du téléphone (appli Android et « Mobile connecté »), les plus récentes à la fin.
    historique: Vec<crate::data::PhoneItem>,
    /// Dernier état envoyé par le téléphone : batterie, en charge, sonnerie, position…
    statut: Value,
    statut_recu: Option<chrono::NaiveDateTime>,
    /// Personnes importantes (réglage « people ») et mots-clés.
    importants: Vec<String>,
}

const MOTS_IMPORTANTS: [&str; 22] = [
    "urgent", "important", "vite", "rappelle", "appelle moi", "appelle-moi", "code", "verification", "vérification",
    "mot de passe", "maman", "papa", "mere", "mère", "pere", "père", "college", "collège", "prof", "école", "ecole", "rendez-vous",
];

/// Une notification est-elle importante ? (appel, personne importante, mot-clé)
pub fn est_important(p: &crate::data::PhoneItem, importants: &[String]) -> bool {
    let tout = crate::pouvoirs::sans_accents(&format!("{} {} {}", p.from, p.text, p.app)).to_lowercase();
    let de = crate::pouvoirs::sans_accents(&p.from).to_lowercase();
    p.kind == "appel"
        || importants.iter().map(|x| crate::pouvoirs::sans_accents(x.trim()).to_lowercase()).any(|x| !x.is_empty() && de.contains(&x))
        || MOTS_IMPORTANTS.iter().any(|m| tout.contains(&crate::pouvoirs::sans_accents(m)))
}

/// Garde une notification du téléphone (pour « est-ce que j'ai eu un message important ? »).
pub fn memoriser(p: &crate::data::PhoneItem) {
    avec(|e| {
        e.historique.push(p.clone());
        let trop = e.historique.len().saturating_sub(100);
        e.historique.drain(..trop);
    });
}

pub fn regler_importants(liste: &str) {
    let v: Vec<String> = liste.split([',', ';', '\n']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    avec(|e| e.importants = v);
}

/// Résumé oral des messages reçus : `filtre` = importants, sms ou tous.
pub fn resume_messages(filtre: &str) -> String {
    let f = crate::pouvoirs::sans_accents(filtre).to_lowercase();
    avec(|e| {
        let depuis = chrono::Local::now().naive_local() - chrono::Duration::hours(24);
        let recents: Vec<&crate::data::PhoneItem> = e.historique.iter().filter(|p| p.time >= depuis).collect();
        let dire = |p: &crate::data::PhoneItem| {
            let quoi = match p.kind.as_str() {
                "sms" => "SMS".to_string(),
                "appel" => "appel".to_string(),
                _ if !p.app.is_empty() => p.app.clone(),
                _ => "notification".to_string(),
            };
            let t: String = p.text.chars().take(120).collect();
            format!("{} ({quoi}, {}) : « {t} »", p.from, p.time.format("%H:%M"))
        };
        if recents.is_empty() {
            return "Je n'ai reçu aucune notification de ton téléphone depuis hier. Vérifie que l'appli Jarvis Téléphone (ou Mobile connecté) est bien reliée.".into();
        }
        if f.contains("sms") || f.contains("message") && !f.contains("important") {
            let sms: Vec<String> = recents.iter().rev().filter(|p| p.kind == "sms").take(5).map(|p| dire(p)).collect();
            return if sms.is_empty() { "Aucun SMS depuis hier.".into() } else { format!("Tes derniers SMS : {}.", sms.join(" ; ")) };
        }
        if f.contains("tous") || f.contains("tout") {
            let v: Vec<String> = recents.iter().rev().take(6).map(|p| dire(p)).collect();
            return format!("{} notifications depuis hier. Les dernières : {}.", recents.len(), v.join(" ; "));
        }
        let imp: Vec<String> = recents.iter().rev().filter(|p| est_important(p, &e.importants)).take(5).map(|p| dire(p)).collect();
        if imp.is_empty() {
            let mut par_appli: Vec<(String, usize)> = Vec::new();
            for p in &recents {
                let k = if p.kind == "sms" { "SMS".to_string() } else if p.app.is_empty() { p.kind.clone() } else { p.app.clone() };
                match par_appli.iter_mut().find(|(a, _)| *a == k) {
                    Some(x) => x.1 += 1,
                    None => par_appli.push((k, 1)),
                }
            }
            let detail: Vec<String> = par_appli.iter().map(|(a, n)| format!("{n} {a}")).collect();
            format!("Rien d'important. Tu as {} notifications depuis hier : {}.", recents.len(), detail.join(", "))
        } else {
            format!("Oui, {} message{} important{} : {}.", imp.len(), if imp.len() > 1 { "s" } else { "" }, if imp.len() > 1 { "s" } else { "" }, imp.join(" ; "))
        }
    })
}

/// « Quelle batterie ? », « où est mon téléphone ? » : d'après le dernier état envoyé par le téléphone.
pub fn resume_statut(question: &str) -> String {
    avec(|e| {
        let Some(quand) = e.statut_recu else {
            return if e.appareils.is_empty() {
                "Aucun téléphone n'est relié. Menu de l'icône → « Relier un téléphone… ».".into()
            } else {
                "Ton téléphone ne m'a encore rien dit : ouvre l'appli Jarvis Téléphone.".into()
            };
        };
        let s = &e.statut;
        let il_y_a = (chrono::Local::now().naive_local() - quand).num_minutes();
        let quand_txt = if il_y_a < 2 { "à l'instant".to_string() } else { format!("il y a {il_y_a} minutes") };
        let bat = s["batterie"].as_i64().map(|b| format!("{b} %{}", if s["en_charge"].as_bool() == Some(true) { ", en charge" } else { "" }));
        if question == "batterie" {
            return match bat {
                Some(b) => format!("La batterie de ton téléphone est à {b} (nouvelles {quand_txt})."),
                None => "Ton téléphone ne m'a pas donné sa batterie.".into(),
            };
        }
        let mut t = if e.vu.is_some_and(|v| v.elapsed() < Duration::from_secs(90)) {
            "Ton téléphone est connecté au même Wi-Fi que le PC, donc il est sûrement à la maison.".to_string()
        } else {
            format!("Ton téléphone ne répond plus depuis {quand_txt} : il n'est peut-être plus sur le Wi-Fi.")
        };
        if let (Some(la), Some(lo)) = (s["lat"].as_f64(), s["lon"].as_f64()) {
            t += &format!(" Dernière position connue : {la:.5}, {lo:.5}.");
        }
        if let Some(b) = bat {
            t += &format!(" Batterie : {b}.");
        }
        t + " Dis « fais sonner mon téléphone » pour le retrouver."
    })
}

fn statut_recu(j: &Value) {
    avec(|e| {
        if let Some(o) = j.as_object() {
            if !e.statut.is_object() {
                e.statut = json!({});
            }
            for (k, v) in o {
                e.statut[k] = v.clone();
            }
        }
        e.statut_recu = Some(chrono::Local::now().naive_local());
    });
}

static ETAT: Mutex<Option<Etat>> = Mutex::new(None);
static REVEIL: Condvar = Condvar::new();
static LANCE: AtomicBool = AtomicBool::new(false);

fn avec<R>(f: impl FnOnce(&mut Etat) -> R) -> R {
    let mut g = ETAT.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.get_or_insert_with(Etat::default);
    if !e.charge {
        e.charge = true;
        e.appareils = std::fs::read_to_string(fichier()).ok().and_then(|s| serde_json::from_str::<Fichier>(&s).ok()).unwrap_or_default().appareils;
    }
    f(e)
}

fn fichier() -> PathBuf {
    crate::config::dir().join("telephone.json")
}

fn enregistrer(e: &Etat) {
    if let Ok(s) = serde_json::to_string_pretty(&Fichier { appareils: e.appareils.clone() }) {
        let _ = std::fs::write(fichier(), s);
    }
}

fn hasard(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    unsafe {
        windows_sys::Win32::Security::Cryptography::BCryptGenRandom(std::ptr::null_mut(), v.as_mut_ptr(), n as u32, 2);
    }
    v
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn empreinte(jeton: &str) -> String {
    hex(&Sha256::digest(jeton.trim().as_bytes()))
}

/// Adresse du réseau local seulement (jamais Internet).
pub fn adresse_locale(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local(),
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return adresse_locale(IpAddr::V4(v4));
            }
            v.is_loopback() || (v.segments()[0] & 0xffc0) == 0xfe80 || (v.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

/// Adresse IP du PC sur le Wi-Fi (sans rien envoyer : on demande juste au système quelle carte servirait).
pub fn ip_du_pc() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.168.1.1:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then(|| ip.to_string())
}

/// Ouvre l'appairage : nouveau code à 6 chiffres, valable 5 minutes.
pub fn nouveau_code() -> String {
    let b = hasard(4);
    let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) % 1_000_000;
    let code = format!("{n:06}");
    avec(|e| e.code = Some((code.clone(), Instant::now(), 0)));
    code
}

pub fn nb_appareils() -> usize {
    avec(|e| e.appareils.len())
}

pub fn delier_tout() {
    avec(|e| {
        e.appareils.clear();
        enregistrer(e);
    });
    REVEIL.notify_all();
}

/// Envoie un ordre au téléphone (alarme, minuteur, message, sonner). Err = aucun téléphone relié.
pub fn envoyer(cmd: Value) -> Result<bool, String> {
    let r = avec(|e| {
        if e.appareils.is_empty() {
            return Err("Aucun téléphone n'est relié. Menu de l'icône → « Relier un téléphone… ».".to_string());
        }
        e.suivant += 1;
        let id = e.suivant;
        e.commandes.push((id, cmd));
        let garde = e.commandes.len().saturating_sub(50);
        e.commandes.drain(..garde);
        Ok(e.vu.is_some_and(|t| t.elapsed() < Duration::from_secs(60)))
    });
    REVEIL.notify_all();
    r
}

fn jeton_valide(jeton: &str) -> bool {
    if jeton.trim().len() < 32 {
        return false;
    }
    let h = empreinte(jeton);
    avec(|e| {
        let ok = e.appareils.iter().any(|a| a.empreinte == h);
        if ok {
            e.vu = Some(Instant::now());
        }
        ok
    })
}

/// Code d'appairage -> jeton (ou erreur). 5 essais ratés : le code est annulé.
pub fn appairer(code: &str, nom: &str) -> Result<String, String> {
    avec(|e| {
        let Some((bon, depuis, essais)) = e.code.clone() else { return Err("Pas d'appairage en cours : ouvre « Relier un téléphone… » sur le PC.".into()) };
        if depuis.elapsed() > CODE_VALIDE {
            e.code = None;
            return Err("Code expiré : redemande un code sur le PC.".into());
        }
        if code.trim() != bon {
            e.code = if essais + 1 >= 5 { None } else { Some((bon, depuis, essais + 1)) };
            return Err("Mauvais code.".into());
        }
        e.code = None;
        let jeton = hex(&hasard(32));
        let nom = if nom.trim().is_empty() { "Téléphone".to_string() } else { nom.trim().chars().take(40).collect() };
        e.appareils.push(Appareil { nom, empreinte: empreinte(&jeton), relie_le: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string() });
        e.vu = Some(Instant::now());
        enregistrer(e);
        Ok(jeton)
    })
}

/// Ordres en attente, après `depuis` ; attend jusqu'à 25 s s'il n'y en a pas.
fn attendre(depuis: u64, max: Duration) -> (Vec<Value>, u64) {
    let fin = Instant::now() + max;
    let mut g = ETAT.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let e = g.get_or_insert_with(Etat::default);
        let dernier = e.suivant;
        // le PC a redémarré (numéros repartis de zéro) : le téléphone repart avec nous
        let depuis = if depuis > dernier { 0 } else { depuis };
        let v: Vec<Value> = e.commandes.iter().filter(|(i, _)| *i > depuis).map(|(i, c)| {
            let mut c = c.clone();
            c["id"] = json!(i);
            c
        }).collect();
        let reste = fin.saturating_duration_since(Instant::now());
        if !v.is_empty() || reste.is_zero() {
            return (v, dernier);
        }
        g = REVEIL.wait_timeout(g, reste).unwrap_or_else(|e| e.into_inner()).0;
    }
}

/// Notification Android -> élément « téléphone » de l'île (même chemin que « Mobile connecté »).
pub fn notif_vers_item(j: &Value) -> Option<crate::data::PhoneItem> {
    let s = |k: &str| j[k].as_str().unwrap_or("").trim().to_string();
    let (titre, texte, appli, paquet, cat) = (s("titre"), s("texte"), s("app"), s("paquet").to_lowercase(), s("categorie"));
    if titre.is_empty() && texte.is_empty() {
        return None;
    }
    let sms = paquet.contains("messaging") || paquet.contains(".mms") || paquet.contains("android.messages");
    let appel = cat == "call" || paquet.contains("dialer") || paquet.contains("incallui") || paquet.contains("telecom");
    let (kind, text, app) = if appel {
        ("appel", if texte.is_empty() { "Appel".into() } else { texte }, String::new())
    } else if sms {
        ("sms", texte, String::new())
    } else {
        ("appli", texte, appli)
    };
    Some(crate::data::PhoneItem { kind: kind.into(), from: titre, text, app, time: chrono::Local::now().naive_local() })
}

/// Lance le serveur du téléphone (une seule fois).
pub fn demarrer(out: Out) {
    demarrer_sur(out, PORT);
}

fn demarrer_sur(out: Out, port: u16) {
    if LANCE.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("telephone".into()).spawn(move || {
        let l = match TcpListener::bind(("0.0.0.0", port)) {
            Ok(l) => l,
            Err(e) => {
                crate::sys::log(&format!("téléphone : port {PORT} indisponible ({e})"));
                LANCE.store(false, Ordering::SeqCst);
                return;
            }
        };
        crate::sys::log(&format!("téléphone : en écoute sur le port {PORT} (réseau local seulement)"));
        for s in l.incoming().flatten() {
            let local = s.peer_addr().map(|a| adresse_locale(a.ip())).unwrap_or(false);
            if !local {
                continue; // une adresse d'Internet : on raccroche sans répondre
            }
            let o = out.clone();
            std::thread::spawn(move || traiter(s, &o));
        }
    });
}

fn repondre(mut s: TcpStream, code: u16, corps: &Value) {
    let corps = corps.to_string();
    let raison = match code {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    };
    let _ = write!(s, "HTTP/1.1 {code} {raison}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corps}", corps.len());
    let _ = s.flush();
}

fn traiter(s: TcpStream, out: &Out) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let Ok(rs) = s.try_clone() else { return };
    let mut r = BufReader::new(rs);
    let mut ligne = String::new();
    if r.read_line(&mut ligne).is_err() {
        return;
    }
    let mut p = ligne.split_whitespace();
    let (methode, chemin) = (p.next().unwrap_or("").to_string(), p.next().unwrap_or("").to_string());
    let (mut longueur, mut jeton) = (0usize, String::new());
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        let (k, v) = h.split_once(':').unwrap_or((&h, ""));
        match k.trim().to_lowercase().as_str() {
            "content-length" => longueur = v.trim().parse().unwrap_or(0),
            "x-jarvis-jeton" => jeton = v.trim().to_string(),
            _ => {}
        }
    }
    let mut corps = vec![0u8; longueur.min(64 * 1024)];
    if longueur > 0 && r.read_exact(&mut corps).is_err() {
        return repondre(s, 400, &json!({"ok": false, "erreur": "requête incomplète"}));
    }
    let j: Value = serde_json::from_slice(&corps).unwrap_or(json!({}));
    let (chemin, requete) = chemin.split_once('?').unwrap_or((&chemin, ""));
    let txt = |k: &str| j[k].as_str().unwrap_or("").trim().to_string();
    if (methode.as_str(), chemin) == ("POST", "/tel/appairer") {
        return match appairer(&txt("code"), &txt("nom")) {
            Ok(jeton) => {
                crate::sys::log(&format!("téléphone relié : {}", txt("nom")));
                out.send(Msg::Phone(crate::data::PhoneItem { kind: "appli".into(), from: "Téléphone relié".into(), text: format!("{} est relié à Jarvis.", txt("nom")), app: "Jarvis".into(), time: chrono::Local::now().naive_local() }));
                repondre(s, 200, &json!({"ok": true, "jeton": jeton, "pc": std::env::var("COMPUTERNAME").unwrap_or_default()}))
            }
            Err(e) => repondre(s, 401, &json!({"ok": false, "erreur": e})),
        };
    }
    if !jeton_valide(&jeton) {
        return repondre(s, 401, &json!({"ok": false, "erreur": "téléphone pas relié (jeton inconnu)"}));
    }
    match (methode.as_str(), chemin) {
        ("GET", "/tel/etat") => repondre(s, 200, &json!({"ok": true, "pc": std::env::var("COMPUTERNAME").unwrap_or_default(), "jarvis": crate::jarvis_link::status().is_some()})),
        ("GET", "/tel/attendre") => {
            let depuis = requete.split('&').find_map(|kv| kv.strip_prefix("depuis=")).and_then(|v| v.parse().ok()).unwrap_or(0);
            let (commandes, dernier) = attendre(depuis, ATTENTE_MAX);
            repondre(s, 200, &json!({"ok": true, "commandes": commandes, "dernier": dernier}))
        }
        ("POST", "/tel/notif") => {
            if let Some(item) = notif_vers_item(&j) {
                out.send(Msg::Phone(item));
            }
            repondre(s, 200, &json!({"ok": true}))
        }
        ("POST", "/tel/statut") => {
            statut_recu(&j);
            repondre(s, 200, &json!({"ok": true}))
        }
        ("POST", "/tel/resultat") => {
            // résultat d'un ordre (SMS envoyé, appel lancé…) : affiché dans l'île
            let texte = txt("texte");
            crate::sys::log(&format!("téléphone : résultat {} {}", j["id"], texte));
            if !texte.is_empty() {
                out.send(Msg::Phone(crate::data::PhoneItem { kind: "appli".into(), from: "Téléphone".into(), text: texte, app: "Jarvis".into(), time: chrono::Local::now().naive_local() }));
            }
            repondre(s, 200, &json!({"ok": true}))
        }
        ("POST", "/tel/demander") => {
            let texte = txt("texte");
            if texte.is_empty() {
                return repondre(s, 400, &json!({"ok": false, "erreur": "texte vide"}));
            }
            // la voix de Jarvis (même cerveau, même mémoire) si elle tourne, sinon le cerveau de l'île
            let rep = if crate::jarvis_link::status().is_some() {
                crate::jarvis_link::post("/demander", json!({"texte": texte, "source": "telephone"}), 150)
                    .map(|v| v["reponse"].as_str().unwrap_or("").to_string())
            } else {
                let (tx, rx) = channel();
                out.send(Msg::Api(crate::api::ApiReq::Demander { texte, source: "telephone".into() }, tx));
                rx.recv_timeout(Duration::from_secs(150)).map_err(|_| "pas de réponse".to_string()).and_then(|(_, b)| {
                    let v: Value = serde_json::from_str(&b).unwrap_or_default();
                    v["reponse"].as_str().map(|x| x.to_string()).ok_or_else(|| v["erreur"].as_str().unwrap_or("erreur").to_string())
                })
            };
            match rep {
                Ok(t) => repondre(s, 200, &json!({"ok": true, "reponse": t})),
                Err(e) => repondre(s, 200, &json!({"ok": false, "reponse": format!("Désolé : {e}")})),
            }
        }
        _ => repondre(s, 404, &json!({"ok": false, "erreur": "route inconnue"})),
    }
}

/// Durée dite en français → secondes : « 5 minutes », « 1 h 30 », « 90 secondes », « dix minutes ».
pub fn duree(texte: &str) -> Option<u32> {
    let t = crate::pouvoirs::sans_accents(texte).to_lowercase().replace(['-', ','], " ");
    let mots = ["zero", "une", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix", "onze", "douze",
        "treize", "quatorze", "quinze", "seize"];
    let mut total = 0f64;
    let mut nombre: Option<f64> = None;
    let mut sauter_heure = false;
    let t = t.replace("et demie", "30 minutes").replace("et demi", "30 minutes").replace("d'heure", "heure");
    // « 1h30 » -> « 1 heure 30 »
    let mut jetons: Vec<String> = Vec::new();
    for m in t.split_whitespace() {
        match m.split_once('h') {
            Some((a, b)) if !a.is_empty() && a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()) => {
                jetons.push(a.into());
                jetons.push("heure".into());
                if !b.is_empty() {
                    jetons.push(b.into());
                }
            }
            _ if m == "h" => jetons.push("heure".into()),
            _ => jetons.push(m.into()),
        }
    }
    for m in jetons.iter().map(|s| s.as_str()) {
        if let Ok(n) = m.replace(',', ".").parse::<f64>() {
            nombre = Some(n);
        } else if let Some(i) = mots.iter().position(|x| *x == m) {
            nombre = Some(i as f64);
        } else if m == "un" {
            nombre = Some(1.0);
        } else if m == "vingt" {
            nombre = Some(20.0);
        } else if m == "trente" {
            nombre = Some(30.0);
        } else if m == "quarante" {
            nombre = Some(40.0);
        } else if m == "cinquante" {
            nombre = Some(50.0);
        } else if m == "quart" {
            total += 15.0 * 60.0;
            nombre = None;
            sauter_heure = true; // « un quart d'heure »
        } else if matches!(m, "heure" | "heures") {
            if !std::mem::take(&mut sauter_heure) {
                total += nombre.take().unwrap_or(1.0) * 3600.0;
            }
        } else if matches!(m, "minute" | "minutes" | "min" | "mn") {
            total += nombre.take().unwrap_or(1.0) * 60.0;
        } else if matches!(m, "seconde" | "secondes" | "sec" | "s") {
            total += nombre.take().unwrap_or(1.0);
        }
    }
    if let Some(n) = nombre {
        total += n * 60.0; // « 1 h 30 » ou « 5 » tout court : des minutes
    }
    (total >= 1.0 && total <= 86_400.0).then_some(total.round() as u32)
}

/// Heure dite dans une phrase : « … à 7h30 … », « 7 h 30 », « 07:30 », « 18 heures », « midi », « 7h et demie ».
pub fn heure_dans(texte: &str) -> Option<(u32, u32)> {
    let t = crate::pouvoirs::sans_accents(texte).to_lowercase().replace("midi", "12h").replace("minuit", "0h")
        .replace("heures", "h").replace("heure", "h");
    let c: Vec<char> = t.chars().collect();
    let mut i = c.iter().position(|x| x.is_ascii_digit())?;
    let lire = |i: &mut usize| {
        let mut n = String::new();
        while *i < c.len() && c[*i].is_ascii_digit() && n.len() < 2 {
            n.push(c[*i]);
            *i += 1;
        }
        n.parse::<u32>().ok()
    };
    let h = lire(&mut i)?;
    while i < c.len() && c[i] == ' ' {
        i += 1;
    }
    let mut m = 0;
    if i < c.len() && (c[i] == 'h' || c[i] == ':') {
        i += 1;
        while i < c.len() && c[i] == ' ' {
            i += 1;
        }
        m = lire(&mut i).unwrap_or(0);
        let reste: String = c[i..].iter().collect();
        if m == 0 && reste.trim_start().starts_with("et demi") {
            m = 30;
        } else if m == 0 && reste.trim_start().starts_with("et quart") {
            m = 15;
        }
    } else if i < c.len() && c[i].is_ascii_digit() {
        m = lire(&mut i)?;
    }
    (h < 24 && m < 60).then_some((h, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reseau_local_seulement() {
        for ip in ["192.168.1.20", "10.0.0.3", "172.16.4.1", "127.0.0.1", "169.254.1.1", "fe80::1"] {
            assert!(adresse_locale(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "172.32.0.1", "2a01:cb00::1", "1.1.1.1"] {
            assert!(!adresse_locale(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn durees() {
        assert_eq!(duree("5 minutes"), Some(300));
        assert_eq!(duree("dix minutes"), Some(600));
        assert_eq!(duree("1 h 30"), Some(5400));
        assert_eq!(duree("une heure et demie"), Some(5400));
        assert_eq!(duree("90 secondes"), Some(90));
        assert_eq!(duree("un quart d'heure"), Some(900));
        assert_eq!(duree("rien"), None);
    }

    #[test]
    fn heures() {
        assert_eq!(heure_dans("mets une alarme a 7h30 sur mon telephone"), Some((7, 30)));
        assert_eq!(heure_dans("alarme 07:05"), Some((7, 5)));
        assert_eq!(heure_dans("reveille moi a 18 heures sur mon tel"), Some((18, 0)));
        assert_eq!(heure_dans("alarme a 6 h et demie"), Some((6, 30)));
        assert_eq!(heure_dans("alarme a midi"), Some((12, 0)));
        assert_eq!(heure_dans("mets une alarme 7 45"), Some((7, 45)));
        assert_eq!(heure_dans("mets une alarme"), None);
        assert_eq!(duree("mets un minuteur de 5 minutes sur mon telephone"), Some(300));
    }

    #[test]
    fn messages_importants() {
        let maintenant = chrono::Local::now().naive_local();
        let it = |kind: &str, from: &str, text: &str, app: &str| crate::data::PhoneItem { kind: kind.into(), from: from.into(), text: text.into(), app: app.into(), time: maintenant };
        assert!(est_important(&it("sms", "Maman", "À table", ""), &[]));
        assert!(est_important(&it("appli", "Google", "Ton code de vérification est 1234", "Gmail"), &[]));
        assert!(est_important(&it("appli", "Tom", "yo", "Discord"), &["tom".into()]));
        assert!(!est_important(&it("appli", "Tom", "yo", "Discord"), &[]));
        assert!(est_important(&it("appel", "Inconnu", "Appel manqué", ""), &[]));
    }

    #[test]
    fn notifs_classees() {
        let sms = notif_vers_item(&json!({"titre": "Maman", "texte": "Tu rentres ?", "paquet": "com.google.android.apps.messaging"})).unwrap();
        assert_eq!((sms.kind.as_str(), sms.from.as_str()), ("sms", "Maman"));
        let app = notif_vers_item(&json!({"titre": "Lucas", "texte": "gg", "app": "Discord", "paquet": "com.discord"})).unwrap();
        assert_eq!((app.kind.as_str(), app.app.as_str()), ("appli", "Discord"));
        assert_eq!(notif_vers_item(&json!({"titre": "Léa", "categorie": "call", "paquet": "com.google.android.dialer"})).unwrap().kind, "appel");
        assert!(notif_vers_item(&json!({})).is_none());
    }

    /// Faux téléphone : appairage par code, jeton, notification, ordre « alarme » reçu en long polling.
    #[test]
    fn faux_telephone_de_bout_en_bout() {
        let dossier = std::env::temp_dir().join(format!("jarvis-tel-{}", std::process::id()));
        unsafe { std::env::set_var("GIVRE_PROFIL", &dossier) };
        let (out, rx) = crate::data::test_out();
        demarrer_sur(out, 47813); // pas le vrai port : Jarvis tourne peut-être à côté
        std::thread::sleep(Duration::from_millis(300));
        let base = "http://127.0.0.1:47813".to_string();
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(40)).build();
        // sans jeton : refusé
        assert!(agent.get(&format!("{base}/tel/etat")).call().is_err());
        // mauvais code puis bon code
        let code = nouveau_code();
        assert!(agent.post(&format!("{base}/tel/appairer")).send_json(json!({"code": "000000x", "nom": "Pirate"})).is_err());
        let v: Value = agent.post(&format!("{base}/tel/appairer")).send_json(json!({"code": code, "nom": "Pixel"})).unwrap().into_json().unwrap();
        let jeton = v["jeton"].as_str().unwrap().to_string();
        assert_eq!(jeton.len(), 64);
        // le code ne sert qu'une fois
        assert!(agent.post(&format!("{base}/tel/appairer")).send_json(json!({"code": code})).is_err());
        assert!(std::fs::read_to_string(fichier()).unwrap().contains(&empreinte(&jeton)));
        assert!(!std::fs::read_to_string(fichier()).unwrap().contains(&jeton));
        // notification du téléphone -> l'île
        let ok: Value = agent.post(&format!("{base}/tel/notif")).set("X-Jarvis-Jeton", &jeton)
            .send_json(json!({"titre": "Maman", "texte": "À table !", "paquet": "com.google.android.apps.messaging"})).unwrap().into_json().unwrap();
        assert_eq!(ok["ok"], true);
        let mut recu = false;
        while let Ok(m) = rx.recv_timeout(Duration::from_secs(2)) {
            if let Msg::Phone(p) = m {
                recu |= p.from == "Maman" && p.text == "À table !";
            }
        }
        assert!(recu);
        // état du téléphone (batterie) puis question « quelle batterie ? »
        agent.post(&format!("{base}/tel/statut")).set("X-Jarvis-Jeton", &jeton).send_json(json!({"batterie": 42, "en_charge": true})).unwrap();
        assert!(resume_statut("batterie").contains("42 %, en charge"));
        // messages importants (ce qui passe dans l'île est mémorisé par main.rs ; ici directement)
        memoriser(&notif_vers_item(&json!({"titre": "Maman", "texte": "Rappelle-moi vite", "paquet": "com.google.android.apps.messaging"})).unwrap());
        memoriser(&notif_vers_item(&json!({"titre": "Pub", "texte": "Promo -50%", "app": "Shein", "paquet": "com.shein"})).unwrap());
        let r = resume_messages("importants");
        assert!(r.starts_with("Oui, 1 message important") && r.contains("Maman"), "{r}");
        assert!(resume_messages("sms").contains("Rappelle-moi vite"));
        // ordre du PC : le téléphone le reçoit pendant qu'il attend
        let j2 = jeton.clone();
        let b2 = base.clone();
        let attente = std::thread::spawn(move || {
            let v: Value = ureq::get(&format!("{b2}/tel/attendre?depuis=0")).set("X-Jarvis-Jeton", &j2).call().unwrap().into_json().unwrap();
            v
        });
        std::thread::sleep(Duration::from_millis(300));
        assert!(envoyer(json!({"type": "alarme", "heure": 7, "minute": 30, "nom": "collège"})).is_ok());
        let v = attente.join().unwrap();
        assert_eq!(v["commandes"][0]["type"], "alarme");
        assert_eq!(v["commandes"][0]["minute"], 30);
        // délier : le jeton ne marche plus
        delier_tout();
        assert!(agent.get(&format!("{base}/tel/etat")).set("X-Jarvis-Jeton", &jeton).call().is_err());
        assert!(envoyer(json!({"type": "message"})).is_err());
        let _ = std::fs::remove_dir_all(&dossier);
    }
}
