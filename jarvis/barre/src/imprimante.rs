//! Imprimante 3D Elegoo (Centauri Carbon et les autres « SDCP v3 ») :
//! - état et commandes par WebSocket (ws://IP:3030/websocket, l'état arrive toutes les ~3 s) ;
//! - recherche sur le Wi-Fi : message UDP « M99999 » sur le port 3000, sinon on essaie les adresses une par une ;
//! - caméra MJPEG (http://IP:3031/video), ouverte seulement quand la page Imprimante est affichée
//!   (l'imprimante n'accepte que 4 flux vidéo à la fois).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::data::{Msg, Out, Printer};

/// Ordres envoyés au fil de l'imprimante.
pub enum PCmd {
    Pause,
    Reprendre,
    Arreter,
    Lumiere(bool),
    /// Chercher l'imprimante sur le Wi-Fi (bouton « Chercher » ou adresse qui ne répond plus).
    Chercher,
}

/// Ce que la fenêtre garde pour parler au fil de l'imprimante.
pub struct Lien {
    pub tx: Sender<PCmd>,
    /// Vrai = la page Imprimante est ouverte : la caméra tourne.
    pub cam: Arc<AtomicBool>,
}

// Codes repris de l'interface web de l'imprimante :
// 0 attente, 1/15/16/18/19/20/21 préparation, 5 mise en pause, 6 pause,
// 8/14 arrêtée, 9 terminée, 10 vérification, 12 reprise, 13 impression.
pub fn status_label(s: i64) -> &'static str {
    match s {
        0 => "En attente",
        5 => "Mise en pause…",
        6 => "En pause",
        7 => "Arrêt…",
        8 | 14 => "Arrêtée",
        9 => "Terminée",
        10 => "Vérification",
        12 => "Reprise",
        13 => "Impression",
        1 | 15 | 16 | 18 | 19 | 20 | 21 => "Préparation",
        _ => "Occupée",
    }
}

/// Une impression est en cours (même en pause ou en préparation).
pub fn is_active(s: i64) -> bool {
    matches!(s, 1 | 5 | 6 | 7 | 10 | 12 | 13 | 15 | 16 | 18 | 19 | 20 | 21)
}

pub fn en_pause(s: i64) -> bool {
    matches!(s, 5 | 6)
}

/// « 1 h 05 », « 12 min ».
pub fn duree(min: i64) -> String {
    if min >= 60 { format!("{} h {:02}", min / 60, min % 60) } else { format!("{} min", min.max(0)) }
}

/// Nom du fichier sans le chemin ni l'extension (« support_casque_v3 »).
pub fn nom_fichier(f: &str) -> String {
    let f = f.rsplit(['/', '\\']).next().unwrap_or(f);
    let f = f.strip_suffix(".gcode").or_else(|| f.strip_suffix(".gcode.3mf")).or_else(|| f.strip_suffix(".3mf")).unwrap_or(f);
    f.replace('_', " ")
}

// ---------- Fin d'impression ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fin {
    Terminee,
    Arretee,
    /// L'imprimante s'est mise en pause toute seule ou à la main (fin de filament…).
    Pause,
}

/// Suit les changements d'état pour ne sonner qu'une fois par impression.
#[derive(Default)]
pub struct Suivi {
    arme: bool,
    avant: Option<i64>,
    progres: i64,
    couche: i64,
    couches: i64,
    sonne_pour: String,
    pub fichier: String,
}

impl Suivi {
    pub fn update(&mut self, p: &Printer) -> Option<Fin> {
        if !p.online {
            return None;
        }
        let s = p.status;
        let avant = self.avant.replace(s);
        if is_active(s) {
            self.arme = true;
            self.progres = p.progress;
            self.couche = p.layer;
            self.couches = p.total;
            if !p.file.is_empty() {
                self.fichier = p.file.clone();
            }
            let entre_en_pause = s == 6 && avant.is_some_and(|x| x != 6 && x != 5);
            return entre_en_pause.then_some(Fin::Pause);
        }
        if !self.arme {
            return None;
        }
        self.arme = false;
        if !p.task.is_empty() && p.task == self.sonne_pour {
            return None;
        }
        self.sonne_pour = p.task.clone();
        let fini = s == 9 || (s == 0 && (self.progres >= 99 || (self.couches > 0 && self.couche >= self.couches)));
        Some(if fini { Fin::Terminee } else { Fin::Arretee })
    }
}

// ---------- Fil de l'imprimante ----------

pub fn spawn(out: Out, ip: Arc<Mutex<String>>, cherche: Arc<AtomicBool>) -> Lien {
    let (tx, rx) = channel::<PCmd>();
    let cam = Arc::new(AtomicBool::new(false));
    {
        let (out, ip) = (out.clone(), ip.clone());
        let _ = std::thread::Builder::new().name("imprimante".into()).spawn(move || boucle(out, ip, cherche, rx));
    }
    {
        let (out, ip, cam) = (out, ip, cam.clone());
        let _ = std::thread::Builder::new().name("camera".into()).spawn(move || camera(out, ip, cam));
    }
    Lien { tx, cam }
}

fn boucle(out: Out, ip: Arc<Mutex<String>>, cherche: Arc<AtomicBool>, rx: Receiver<PCmd>) {
    let mut last = Printer::default();
    let mut derniere_recherche: Option<Instant> = None;
    let mut echecs = 0u32;
    let mut demande = false;
    loop {
        let addr = ip.lock().map(|i| i.trim().to_string()).unwrap_or_default();
        // pas d'adresse (ou une adresse qui ne répond plus depuis longtemps) : on cherche l'imprimante
        let vieille = derniere_recherche.is_none_or(|t| t.elapsed() > Duration::from_secs(600));
        let auto = cherche.load(Ordering::Relaxed) && vieille && (addr.is_empty() || echecs >= 3);
        if demande || auto {
            derniere_recherche = Some(Instant::now());
            match chercher() {
                Some((ip_trouvee, nom)) => {
                    echecs = 0;
                    out.send(Msg::PrinterFound(ip_trouvee, nom));
                    std::thread::sleep(Duration::from_millis(300)); // la fenêtre enregistre l'adresse
                }
                None if demande => out.send(Msg::PrinterFound(String::new(), String::new())),
                None => {}
            }
            demande = false;
            continue;
        }
        if !addr.is_empty() {
            match session(&addr, &out, &mut last, &rx) {
                Ok(true) => {
                    demande = true;
                    continue;
                }
                Ok(false) => echecs = 0,
                Err(_) => echecs += 1,
            }
            if last.online {
                last.online = false;
                out.send(Msg::Printer(last.clone()));
            }
        }
        // attente avant de réessayer (un ordre « Chercher » la coupe)
        match rx.recv_timeout(Duration::from_secs(if addr.is_empty() { 5 } else { 15 })) {
            Ok(PCmd::Chercher) => demande = true,
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn requete(cmd: u32, data: Value, carte: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    json!({
        "Id": "",
        "Data": {"Cmd": cmd, "Data": data, "RequestID": format!("{:032x}", now.as_nanos()), "MainboardID": carte, "TimeStamp": now.as_secs(), "From": 1},
        "Topic": format!("sdcp/request/{carte}"),
    })
    .to_string()
}

/// Une connexion à l'imprimante. Ok(true) = on a demandé de chercher l'imprimante.
fn session(ip: &str, out: &Out, last: &mut Printer, rx: &Receiver<PCmd>) -> Result<bool, Box<dyn std::error::Error>> {
    use tungstenite::{Error as WsError, Message};
    let addr = (ip, 3030).to_socket_addrs()?.next().ok_or("adresse invalide")?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(4))?;
    tcp.set_read_timeout(Some(Duration::from_millis(800)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(4)))?;
    let (mut ws, _) = tungstenite::client(format!("ws://{ip}:3030/websocket").as_str(), tcp).map_err(|e| e.to_string())?;
    let mut carte = String::new();
    ws.send(Message::text(requete(0, json!({}), &carte)))?;
    ws.send(Message::text(requete(1, json!({}), &carte)))?;
    let (mut ping, mut recu, mut envoye) = (Instant::now(), Instant::now(), Instant::now() - Duration::from_secs(60));
    let mut p = Printer { ip: ip.to_string(), ..last.clone() };
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                recu = Instant::now();
                let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                if carte.is_empty() {
                    if let Some(m) = v["MainboardID"].as_str().or_else(|| v["Data"]["MainboardID"].as_str()) {
                        carte = m.to_string();
                    }
                }
                if let Some(a) = v["Attributes"].as_object() {
                    if let Some(n) = a.get("Name").and_then(Value::as_str).or_else(|| a.get("MachineName").and_then(Value::as_str)) {
                        p.name = n.to_string();
                    }
                }
                let st = &v["Status"];
                if st.is_null() {
                    continue;
                }
                lire_etat(st, &mut p);
                p.online = true;
                // n'envoie que les vrais changements (le temps restant au plus une fois par minute)
                let mut cmp = p.clone();
                cmp.left_min = last.left_min;
                if cmp != *last || (p.left_min != last.left_min && envoye.elapsed() > Duration::from_secs(55)) {
                    *last = p.clone();
                    envoye = Instant::now();
                    out.send(Msg::Printer(p.clone()));
                }
            }
            Ok(Message::Close(_)) => return Ok(false),
            Ok(_) => recu = Instant::now(),
            Err(WsError::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.into()),
        }
        while let Ok(c) = rx.try_recv() {
            let (cmd, data) = match c {
                PCmd::Pause => (129, json!({})),
                PCmd::Reprendre => (131, json!({})),
                PCmd::Arreter => (130, json!({})),
                PCmd::Lumiere(on) => (403, json!({"LightStatus": {"SecondLight": on, "RgbLight": [0, 0, 0]}})),
                PCmd::Chercher => {
                    // déjà connecté : on redit juste où elle est
                    out.send(Msg::PrinterFound(ip.to_string(), p.name.clone()));
                    continue;
                }
            };
            crate::sys::log(&format!("imprimante : commande {cmd}"));
            ws.send(Message::text(requete(cmd, data, &carte)))?;
            ws.send(Message::text(requete(0, json!({}), &carte)))?;
        }
        if ping.elapsed() >= Duration::from_secs(25) {
            ws.send(Message::text("ping"))?;
            ws.send(Message::text(requete(0, json!({}), &carte)))?;
            ping = Instant::now();
        }
        if recu.elapsed() >= Duration::from_secs(70) {
            return Err("plus de réponse".into());
        }
    }
}

/// Recopie l'état reçu (l'imprimante peut n'envoyer qu'une partie des champs).
fn lire_etat(st: &Value, p: &mut Printer) {
    let i = |v: &Value, d: &mut i64| {
        if let Some(x) = v.as_f64() {
            *d = x.round() as i64;
        }
    };
    let pi = &st["PrintInfo"];
    i(&pi["Status"], &mut p.status);
    i(&pi["Progress"], &mut p.progress);
    i(&pi["CurrentLayer"], &mut p.layer);
    i(&pi["TotalLayer"], &mut p.total);
    if let Some(f) = pi["Filename"].as_str() {
        p.file = f.to_string();
    }
    if let Some(t) = pi["TaskId"].as_str() {
        p.task = t.to_string();
    }
    let (cur, tot) = (pi["CurrentTicks"].as_f64(), pi["TotalTicks"].as_f64());
    if let (Some(mut cur), Some(mut tot)) = (cur, tot) {
        // certains firmwares comptent en millisecondes
        if tot > 864_000.0 {
            cur /= 1000.0;
            tot /= 1000.0;
        }
        p.left_min = ((tot - cur).max(0.0) / 60.0).round() as i64;
        p.elapsed_min = (cur.max(0.0) / 60.0).round() as i64;
        // la Centauri Carbon laisse parfois « Progress » à 0 pendant l'impression : on prend le temps écoulé
        if p.progress == 0 && tot > 0.0 && cur > 0.0 && is_active(p.status) {
            p.progress = ((cur / tot) * 100.0).round().clamp(0.0, 99.0) as i64;
        }
    }
    i(&st["TempOfNozzle"], &mut p.nozzle);
    i(&st["TempTargetNozzle"], &mut p.nozzle_target);
    i(&st["TempOfHotbed"], &mut p.bed);
    i(&st["TempTargetHotbed"], &mut p.bed_target);
    i(&st["TempOfBox"], &mut p.box_temp);
    let l = &st["LightStatus"]["SecondLight"];
    if !l.is_null() {
        p.light = l.as_bool().unwrap_or(false) || l.as_i64().unwrap_or(0) != 0;
    }
}

/// `JarvisBarre.exe --imprimante-test` : cherche l'imprimante, lit son état et prend une image de la caméra.
pub fn selftest() {
    use tungstenite::Message;
    let t0 = Instant::now();
    let r = chercher();
    println!("recherche ({} ms) : {r:?}", t0.elapsed().as_millis());
    println!("balayage seul : {:?}", par_balayage());
    let Some((ip, _)) = r else { return };
    let essai = || -> Result<Printer, Box<dyn std::error::Error>> {
        let tcp = TcpStream::connect_timeout(&(ip.as_str(), 3030).to_socket_addrs()?.next().ok_or("adresse")?, Duration::from_secs(4))?;
        tcp.set_read_timeout(Some(Duration::from_secs(6)))?;
        let (mut ws, _) = tungstenite::client(format!("ws://{ip}:3030/websocket").as_str(), tcp).map_err(|e| e.to_string())?;
        ws.send(Message::text(requete(0, json!({}), "")))?;
        let mut p = Printer::default();
        for _ in 0..10 {
            if let Message::Text(t) = ws.read()? {
                let v: Value = serde_json::from_str(t.as_str())?;
                if !v["Status"].is_null() {
                    lire_etat(&v["Status"], &mut p);
                    return Ok(p);
                }
            }
        }
        Err("pas d'état reçu".into())
    };
    match essai() {
        Ok(p) => println!(
            "état : {} ({}), {} %, couche {}/{}, reste {} min, buse {}/{} °C, plateau {}/{} °C, caisson {} °C, lumière {}, fichier « {} »",
            status_label(p.status), p.status, p.progress, p.layer, p.total, p.left_min, p.nozzle, p.nozzle_target, p.bed, p.bed_target, p.box_temp, p.light, p.file
        ),
        Err(e) => println!("état : erreur {e}"),
    }
    let cam = AtomicBool::new(true);
    let image = || -> io::Result<(u32, u32)> {
        let mut tcp = TcpStream::connect_timeout(&(ip.as_str(), 3031).to_socket_addrs()?.next().ok_or_else(|| io::Error::other("adresse"))?, Duration::from_secs(4))?;
        tcp.set_read_timeout(Some(Duration::from_secs(8)))?;
        write!(tcp, "GET /video HTTP/1.1\r\nHost: {ip}:3031\r\nConnection: close\r\n\r\n")?;
        let mut r = BufReader::new(tcp);
        let mut l = Vec::new();
        lire_ligne(&mut r, &mut l)?;
        loop {
            lire_ligne(&mut r, &mut l)?;
            if l.trim_ascii().is_empty() {
                break;
            }
        }
        let mut jpeg = Vec::new();
        lire_partie(&mut r, &mut l, &mut jpeg)?;
        let pm = decoder(&jpeg).ok_or_else(|| io::Error::other("image illisible"))?;
        let _ = cam.load(Ordering::Relaxed);
        Ok((pm.width(), pm.height()))
    };
    println!("caméra : {:?}", image());
}

// ---------- Recherche sur le Wi-Fi ----------

/// Cherche une imprimante Elegoo sur le réseau : (adresse, nom).
pub fn chercher() -> Option<(String, String)> {
    crate::sys::log("imprimante : je cherche sur le Wi-Fi");
    // diffusion sur le réseau de ce PC (x.y.z.255) : 255.255.255.255 part parfois sur une autre carte réseau
    let reseau = mon_adresse().map(|a| {
        let [a, b, c, _] = a.octets();
        Ipv4Addr::new(a, b, c, 255)
    });
    if let Some(r) = reseau.and_then(|n| par_annonce(Some(n))).or_else(|| par_annonce(None)) {
        crate::sys::log(&format!("imprimante trouvée : {} ({})", r.0, r.1));
        return Some(r);
    }
    let r = par_balayage();
    match &r {
        Some((ip, nom)) => crate::sys::log(&format!("imprimante trouvée en essayant les adresses : {ip} ({nom})")),
        None => crate::sys::log("imprimante : rien trouvé"),
    }
    r
}

/// « M99999 » en diffusion (à tout le monde, à un réseau x.y.z.255 ou à une seule adresse) :
/// les imprimantes SDCP répondent par leur fiche.
fn par_annonce(cible: Option<Ipv4Addr>) -> Option<(String, String)> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.set_broadcast(true).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(400))).ok()?;
    let dest = SocketAddr::from((cible.unwrap_or(Ipv4Addr::BROADCAST), 3000));
    for _ in 0..3 {
        let _ = s.send_to(b"M99999", dest);
        let fin = Instant::now() + Duration::from_millis(1200);
        let mut buf = [0u8; 4096];
        while Instant::now() < fin {
            let Ok((n, de)) = s.recv_from(&mut buf) else { continue };
            let Ok(v) = serde_json::from_slice::<Value>(&buf[..n]) else { continue };
            let d = &v["Data"];
            let ip = d["MainboardIP"].as_str().map(str::to_string).unwrap_or_else(|| de.ip().to_string());
            let nom = d["Name"].as_str().or_else(|| d["MachineName"].as_str()).unwrap_or("Imprimante 3D").to_string();
            return Some((ip, nom));
        }
    }
    None
}

/// Adresse de ce PC sur le réseau local (aucun paquet n'est envoyé).
fn mon_adresse() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    // « connect » d'UDP n'envoie rien : il choisit juste la carte réseau de la route par défaut
    s.connect("8.8.8.8:53").or_else(|_| s.connect("192.168.255.255:9")).ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(a) if !a.is_loopback() && !a.is_unspecified() => Some(a),
        _ => None,
    }
}

/// Repli : on essaie le port 3030 des 254 adresses du réseau (16 fils, ~3 s).
fn par_balayage() -> Option<(String, String)> {
    let moi = mon_adresse()?;
    let [a, b, c, _] = moi.octets();
    let trouve: Arc<Mutex<Option<Ipv4Addr>>> = Arc::new(Mutex::new(None));
    let fils: Vec<_> = (0..16u8)
        .map(|k| {
            let trouve = trouve.clone();
            std::thread::spawn(move || {
                for d in (1..=254u8).filter(|d| d % 16 == k) {
                    if trouve.lock().map(|t| t.is_some()).unwrap_or(true) {
                        return;
                    }
                    let ip = Ipv4Addr::new(a, b, c, d);
                    if ip == moi {
                        continue;
                    }
                    if TcpStream::connect_timeout(&SocketAddr::from((ip, 3030)), Duration::from_millis(180)).is_ok() {
                        if let Ok(mut t) = trouve.lock() {
                            t.get_or_insert(ip);
                        }
                        return;
                    }
                }
            })
        })
        .collect();
    for f in fils {
        let _ = f.join();
    }
    let ip = (*trouve.lock().ok()?)?;
    let nom = par_annonce(Some(ip)).map(|r| r.1).unwrap_or_else(|| "Imprimante 3D".into());
    Some((ip.to_string(), nom))
}

// ---------- Caméra ----------

const MAX_JPEG: usize = 8 * 1024 * 1024;

fn camera(out: Out, ip: Arc<Mutex<String>>, cam: Arc<AtomicBool>) {
    loop {
        let addr = ip.lock().map(|i| i.trim().to_string()).unwrap_or_default();
        if !cam.load(Ordering::Relaxed) || addr.is_empty() {
            std::thread::sleep(Duration::from_millis(300));
            continue;
        }
        let r = flux(&addr, &out, &cam);
        if let Err(e) = r {
            crate::sys::log(&format!("caméra de l'imprimante : {e}"));
            out.send(Msg::PrinterCam(None));
            // nouvel essai dans 4 s (plus tôt si la page se ferme puis se rouvre)
            for _ in 0..40 {
                if !cam.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn flux(ip: &str, out: &Out, cam: &AtomicBool) -> io::Result<()> {
    let addr = (ip, 3031).to_socket_addrs()?.next().ok_or_else(|| io::Error::other("adresse invalide"))?;
    let mut tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(4))?;
    tcp.set_read_timeout(Some(Duration::from_secs(8)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(4)))?;
    write!(tcp, "GET /video HTTP/1.1\r\nHost: {ip}:3031\r\nUser-Agent: Jarvis\r\nConnection: close\r\n\r\n")?;
    let mut r = BufReader::with_capacity(64 * 1024, tcp);
    let mut ligne = Vec::with_capacity(256);
    lire_ligne(&mut r, &mut ligne)?;
    if !ligne.starts_with(b"HTTP/") || !ligne.windows(4).any(|w| w == b" 200") {
        return Err(io::Error::other("réponse inattendue (caméra occupée ?)"));
    }
    loop {
        lire_ligne(&mut r, &mut ligne)?;
        if ligne.trim_ascii().is_empty() {
            break;
        }
    }
    let mut jpeg = Vec::with_capacity(128 * 1024);
    let mut derniere: Option<Instant> = None;
    while cam.load(Ordering::Relaxed) {
        lire_partie(&mut r, &mut ligne, &mut jpeg)?;
        // 4 images par seconde suffisent (et ça ne coûte presque rien)
        if derniere.is_some_and(|t| t.elapsed() < Duration::from_millis(240)) {
            continue;
        }
        if let Some(pm) = decoder(&jpeg) {
            derniere = Some(Instant::now());
            out.send(Msg::PrinterCam(Some(pm)));
        }
    }
    Ok(())
}

fn lire_ligne(r: &mut impl BufRead, ligne: &mut Vec<u8>) -> io::Result<()> {
    ligne.clear();
    if r.by_ref().take(8192).read_until(b'\n', ligne)? == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(())
}

fn longueur(t: &[u8]) -> Option<usize> {
    let deux_points = t.iter().position(|&c| c == b':')?;
    if !t[..deux_points].trim_ascii().eq_ignore_ascii_case(b"content-length") {
        return None;
    }
    std::str::from_utf8(t[deux_points + 1..].trim_ascii()).ok()?.parse().ok()
}

/// Une partie du flux multipart : séparateur, en-têtes, puis l'image JPEG.
fn lire_partie(r: &mut impl BufRead, ligne: &mut Vec<u8>, jpeg: &mut Vec<u8>) -> io::Result<()> {
    let mut sautees = 0;
    loop {
        lire_ligne(r, ligne)?;
        if ligne.trim_ascii().starts_with(b"--") {
            break;
        }
        sautees += 1;
        if sautees > 64 {
            return Err(io::Error::other("flux vidéo illisible"));
        }
    }
    let mut n = None;
    loop {
        lire_ligne(r, ligne)?;
        let t = ligne.trim_ascii();
        if t.is_empty() {
            break;
        }
        if let Some(x) = longueur(t) {
            n = Some(x);
        }
    }
    match n {
        Some(n) if n > 0 && n <= MAX_JPEG => {
            jpeg.resize(n, 0);
            r.read_exact(jpeg)
        }
        _ => jusqu_a_la_fin(r, jpeg),
    }
}

/// Sans « Content-Length » : on lit jusqu'au marqueur de fin JPEG (FF D9).
fn jusqu_a_la_fin(r: &mut impl BufRead, jpeg: &mut Vec<u8>) -> io::Result<()> {
    jpeg.clear();
    let mut avant = 0u8;
    loop {
        let buf = r.fill_buf()?;
        if buf.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let mut pris = buf.len();
        let mut fini = false;
        for (i, &b) in buf.iter().enumerate() {
            if avant == 0xFF && b == 0xD9 {
                pris = i + 1;
                fini = true;
                break;
            }
            avant = b;
        }
        jpeg.extend_from_slice(&buf[..pris]);
        r.consume(pris);
        if fini {
            return Ok(());
        }
        if jpeg.len() > MAX_JPEG {
            return Err(io::Error::other("image trop grande"));
        }
    }
}

fn decoder(jpeg: &[u8]) -> Option<tiny_skia::Pixmap> {
    let img = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg).ok()?;
    let img = if img.width() > 800 { img.resize(800, 600, image::imageops::FilterType::Triangle) } else { img };
    let img = img.to_rgba8();
    let (w, h) = img.dimensions();
    tiny_skia::Pixmap::from_vec(img.into_raw(), tiny_skia::IntSize::from_wh(w, h)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(status: i64, progress: i64, task: &str) -> Printer {
        Printer { online: true, status, progress, task: task.into(), file: "cube.gcode".into(), ..Default::default() }
    }

    #[test]
    fn une_seule_alarme_par_impression() {
        let mut s = Suivi::default();
        assert_eq!(s.update(&p(0, 0, "")), None);
        assert_eq!(s.update(&p(13, 40, "a")), None);
        assert_eq!(s.update(&p(13, 99, "a")), None);
        assert_eq!(s.update(&p(9, 100, "a")), Some(Fin::Terminee));
        assert_eq!(s.update(&p(9, 100, "a")), None);
        assert_eq!(s.update(&p(0, 100, "a")), None);
    }

    #[test]
    fn arret_et_pause() {
        let mut s = Suivi::default();
        assert_eq!(s.update(&p(13, 20, "b")), None);
        assert_eq!(s.update(&p(6, 20, "b")), Some(Fin::Pause));
        assert_eq!(s.update(&p(6, 20, "b")), None);
        assert_eq!(s.update(&p(13, 20, "b")), None);
        assert_eq!(s.update(&p(8, 20, "b")), Some(Fin::Arretee));
    }

    #[test]
    fn etat_partiel_et_millisecondes() {
        let mut pr = Printer::default();
        let v: Value = serde_json::from_str(
            r#"{"TempOfNozzle":219.6,"TempTargetNozzle":220,"TempOfHotbed":59.8,"LightStatus":{"SecondLight":1},
            "PrintInfo":{"Status":13,"Progress":42.4,"CurrentLayer":126,"TotalLayer":300,"CurrentTicks":3720000,"TotalTicks":8820000,"Filename":"support_casque_v3.gcode"}}"#,
        )
        .unwrap();
        lire_etat(&v, &mut pr);
        assert_eq!((pr.status, pr.progress, pr.nozzle, pr.bed, pr.light), (13, 42, 220, 60, true));
        assert_eq!(pr.left_min, 85);
        // un message partiel ne remet pas le reste à zéro
        lire_etat(&serde_json::json!({"TempOfNozzle": 100}), &mut pr);
        assert_eq!((pr.status, pr.nozzle, pr.total), (13, 100, 300));
        assert_eq!(nom_fichier("/local/support_casque_v3.gcode"), "support casque v3");
        assert_eq!(duree(85), "1 h 25");
    }
}
