//! Fils en arrière-plan : musique en cours (Windows), notifications Discord, imprimante 3D.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Local;
use serde_json::{Value, json};

use crate::accounts::{self, Acct, Secrets, now_ts, post_form};
use crate::data::{Media, Msg, Notif, Out, Printer};

// ---------- Musique (contrôles multimédias de Windows : Spotify, YouTube, Deezer…) ----------

pub enum MCmd {
    Refresh,
    Toggle,
    Next,
    Prev,
}

pub fn spawn_media(out: Out) -> Sender<MCmd> {
    let (tx, rx) = channel::<MCmd>();
    let t = tx.clone();
    let _ = std::thread::Builder::new().name("musique".into()).spawn(move || {
        if let Err(e) = media_loop(&out, rx, t) {
            crate::sys::log(&format!("musique : {e}"));
        }
    });
    tx
}

fn app_name(id: &str) -> String {
    let base = id.split('!').next().unwrap_or(id);
    let base = base.rsplit('\\').next().unwrap_or(base).trim_end_matches(".exe").trim_end_matches(".EXE");
    match base.to_lowercase().as_str() {
        s if s.contains("spotify") => "Spotify".into(),
        s if s.contains("chrome") => "Chrome".into(),
        s if s.contains("msedge") => "Edge".into(),
        s if s.contains("firefox") || s == "308046b0af4a39cb" => "Firefox".into(),
        s if s.contains("deezer") => "Deezer".into(),
        s if s.contains("zunemusic") || s.contains("media") => "Lecteur multimédia".into(),
        _ => base.split('.').next_back().unwrap_or(base).to_string(),
    }
}

fn thumb(p: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<tiny_skia::Pixmap> {
    use windows::Storage::Streams::{DataReader, IInputStream};
    use windows::core::Interface;
    let st = p.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
    let size = st.Size().ok()? as u32;
    if size == 0 || size > 8_000_000 {
        return None;
    }
    let input: IInputStream = st.cast().ok()?;
    let r = DataReader::CreateDataReader(&input).ok()?;
    r.LoadAsync(size).ok()?.get().ok()?;
    let mut buf = vec![0u8; size as usize];
    r.ReadBytes(&mut buf).ok()?;
    let img = image::load_from_memory(&buf).ok()?.thumbnail(160, 160).to_rgba8();
    let (w, h) = img.dimensions();
    let mut px = img.into_raw();
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = ((p[c] as u32 * a + 127) / 255) as u8;
        }
    }
    tiny_skia::Pixmap::from_vec(px, tiny_skia::IntSize::from_wh(w, h)?)
}

fn media_loop(out: &Out, rx: std::sync::mpsc::Receiver<MCmd>, tx: Sender<MCmd>) -> windows::core::Result<()> {
    use windows::Foundation::TypedEventHandler;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Mgr,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    let mgr = Mgr::RequestAsync()?.get()?;
    {
        let t = tx.clone();
        mgr.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(MCmd::Refresh);
            Ok(())
        }))?;
    }
    let mut hooked: Option<Session> = None;
    let mut art_key = String::new();
    let mut art: Option<tiny_skia::Pixmap> = None;
    let _ = tx.send(MCmd::Refresh);
    while let Ok(cmd) = rx.recv() {
        // regroupe les rafales d'événements
        std::thread::sleep(Duration::from_millis(120));
        while rx.try_recv().is_ok() {}
        let s = mgr.GetCurrentSession().ok();
        if let Some(s) = &s {
            let r = match cmd {
                MCmd::Toggle => s.TryTogglePlayPauseAsync().and_then(|o| o.get()).map(|_| ()),
                MCmd::Next => s.TrySkipNextAsync().and_then(|o| o.get()).map(|_| ()),
                MCmd::Prev => s.TrySkipPreviousAsync().and_then(|o| o.get()).map(|_| ()),
                MCmd::Refresh => Ok(()),
            };
            let _ = r;
            if hooked.as_ref() != Some(s) {
                let t = tx.clone();
                let _ = s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = t.send(MCmd::Refresh);
                    Ok(())
                }));
                let t = tx.clone();
                let _ = s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = t.send(MCmd::Refresh);
                    Ok(())
                }));
                hooked = Some(s.clone());
            }
        }
        let media = s.as_ref().and_then(|s| {
            let p = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
            let title = p.Title().ok()?.to_string();
            if title.is_empty() {
                return None;
            }
            let artist = p.Artist().map(|a| a.to_string()).unwrap_or_default();
            let key = format!("{title}|{artist}");
            if key != art_key {
                art_key = key;
                art = thumb(&p);
            }
            let playing = s.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? == Status::Playing;
            let app = s.SourceAppUserModelId().map(|a| app_name(&a.to_string())).unwrap_or_default();
            Some(Media { title, artist, app, playing, art: art.clone() })
        });
        out.send(Msg::Media(media));
    }
    Ok(())
}

// ---------- Discord (canal local de l'application Discord, autorisé par l'utilisateur) ----------

pub enum DCmd {
    Connect(String, String),
}

pub fn spawn_discord(out: Out, vault: Arc<Mutex<Secrets>>, id: String, secret: String) -> Sender<DCmd> {
    let (tx, rx) = channel::<DCmd>();
    let _ = std::thread::Builder::new().name("discord".into()).spawn(move || {
        let (mut id, mut secret) = (id, secret);
        let mut interactive = false;
        let mut last = String::new();
        let mut state = |s: String, out: &Out| {
            if s != last {
                last = s.clone();
                out.send(Msg::DiscordState(s));
            }
        };
        loop {
            if id.trim().is_empty() {
                state("à configurer".into(), &out);
            } else {
                match discord_session(&out, &vault, id.trim(), secret.trim(), interactive) {
                    Ok(()) => state("Discord fermé".into(), &out),
                    Err(e) => state(e, &out),
                }
            }
            interactive = false;
            match rx.recv_timeout(Duration::from_secs(30)) {
                Ok(DCmd::Connect(i, s)) => {
                    id = i;
                    secret = s;
                    interactive = true;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(_) => return,
            }
        }
    });
    tx
}

fn frame_send(p: &mut File, op: u32, v: &Value) -> Result<(), String> {
    let b = v.to_string().into_bytes();
    let mut f = Vec::with_capacity(b.len() + 8);
    f.extend_from_slice(&op.to_le_bytes());
    f.extend_from_slice(&(b.len() as u32).to_le_bytes());
    f.extend_from_slice(&b);
    p.write_all(&f).map_err(|_| "Discord fermé".to_string())
}

fn frame_recv(p: &mut File) -> Result<(u32, Value), String> {
    let mut h = [0u8; 8];
    p.read_exact(&mut h).map_err(|_| "Discord fermé".to_string())?;
    let op = u32::from_le_bytes([h[0], h[1], h[2], h[3]]);
    let len = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
    if len > 4 << 20 {
        return Err("réponse Discord invalide".into());
    }
    let mut b = vec![0u8; len];
    p.read_exact(&mut b).map_err(|_| "Discord fermé".to_string())?;
    Ok((op, serde_json::from_slice(&b).unwrap_or_default()))
}

fn rpc(p: &mut File, cmd: &str, evt: Option<&str>, args: Value) -> Result<Value, String> {
    let nonce = format!("{:x}", now_ts() as u64 ^ u64::from_le_bytes(crate::sys::random(8).try_into().unwrap_or([0; 8])));
    let mut msg = json!({"cmd": cmd, "args": args, "nonce": nonce});
    if let Some(e) = evt {
        msg["evt"] = json!(e);
    }
    frame_send(p, 1, &msg)?;
    loop {
        let (op, v) = frame_recv(p)?;
        if op == 2 {
            return Err("Discord a fermé la connexion".into());
        }
        crate::sys::log(&format!("discord {cmd} : reçu {}", v.to_string().chars().take(300).collect::<String>()));
        if v["nonce"] == nonce.as_str() {
            return Ok(v);
        }
        // Une erreur envoyée sans notre numéro : sans ça, Givre attendrait pour toujours.
        if v["evt"] == "ERROR" {
            return Err(format!("Discord : {}", v["data"]["message"].as_str().unwrap_or("erreur inconnue")));
        }
    }
}

fn discord_token(ag: &ureq::Agent, id: &str, secret: &str, form: &[(&str, &str)]) -> Result<Acct, String> {
    let mut f = vec![("client_id", id), ("client_secret", secret)];
    f.extend_from_slice(form);
    let v = post_form(ag, "https://discord.com/api/oauth2/token", &f)?;
    Ok(Acct {
        email: "discord".into(),
        refresh: v["refresh_token"].as_str().unwrap_or("").into(),
        access: v["access_token"].as_str().ok_or("Discord a refusé l'autorisation.")?.into(),
        expires: now_ts() + v["expires_in"].as_i64().unwrap_or(600_000) - 120,
    })
}

fn discord_session(out: &Out, vault: &Mutex<Secrets>, id: &str, secret: &str, interactive: bool) -> Result<(), String> {
    let mut p = (0..10)
        .find_map(|i| OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{i}")).ok())
        .ok_or("Discord fermé")?;
    frame_send(&mut p, 0, &json!({"v": 1, "client_id": id}))?;
    let (_, ready) = frame_recv(&mut p)?;
    if ready["evt"] != "READY" {
        return Err(format!("Discord : {}", ready["data"]["message"].as_str().unwrap_or("ID d'application refusé")));
    }
    let ag = crate::data::agent();
    let mut tok = vault.lock().unwrap().discord.clone();
    if let Some(t) = &tok {
        if t.expires < now_ts() {
            tok = discord_token(&ag, id, secret, &[("grant_type", "refresh_token"), ("refresh_token", &t.refresh)]).ok();
        }
    }
    let tok = match tok {
        Some(t) => t,
        None => {
            if !interactive {
                return Err("à autoriser (Paramètres › Comptes)".into());
            }
            out.send(Msg::DiscordState("accepte la demande dans Discord…".into()));
            let r = rpc(&mut p, "AUTHORIZE", None, json!({"client_id": id, "scopes": ["rpc", "rpc.notifications.read"]}))?;
            let code = r["data"]["code"]
                .as_str()
                .ok_or_else(|| format!("Discord : {}", r["data"]["message"].as_str().unwrap_or("autorisation refusée")))?;
            out.send(Msg::DiscordState("autorisé, connexion…".into()));
            discord_token(
                &ag,
                id,
                secret,
                &[("grant_type", "authorization_code"), ("code", code), ("redirect_uri", "http://localhost")],
            )?
        }
    };
    {
        let mut s = vault.lock().unwrap();
        s.discord = Some(tok.clone());
        accounts::save(&s);
    }
    let r = rpc(&mut p, "AUTHENTICATE", None, json!({"access_token": tok.access}))?;
    if r["evt"] == "ERROR" {
        let mut s = vault.lock().unwrap();
        s.discord = None;
        accounts::save(&s);
        return Err("autorisation expirée : reconnecte Discord".into());
    }
    let user = r["data"]["user"]["global_name"].as_str().or(r["data"]["user"]["username"].as_str()).unwrap_or("").to_string();
    let sub = rpc(&mut p, "SUBSCRIBE", Some("NOTIFICATION_CREATE"), json!({}))?;
    if sub["evt"] == "ERROR" {
        return Err(format!("Discord : {}", sub["data"]["message"].as_str().unwrap_or("notifications refusées")));
    }
    out.send(Msg::DiscordState(if user.is_empty() { "connecté".into() } else { format!("connecté ({user})") }));
    loop {
        let (op, v) = frame_recv(&mut p)?;
        match op {
            2 => return Ok(()),
            3 => frame_send(&mut p, 4, &v)?,
            _ => {
                if v["evt"] == "NOTIFICATION_CREATE" {
                    let d = &v["data"];
                    out.send(Msg::Discord(Notif {
                        title: d["title"].as_str().unwrap_or("Discord").into(),
                        body: d["body"].as_str().unwrap_or("").into(),
                        time: Local::now().naive_local(),
                    }));
                }
            }
        }
    }
}

// ---------- Discord via les notifications Windows ----------
//
// Lit seulement les notifications que Discord a déjà affichées dans Windows (centre de
// notifications), avec l'autorisation « Accès aux notifications » de Windows. Aucun jeton Discord,
// aucune lecture des fichiers ou cookies de Discord.

/// Fil de lecture des notifications Windows. `on` = réglage activé ; envoyer () réveille le fil.
pub fn spawn_winnotif(out: Out, on: Arc<std::sync::atomic::AtomicBool>) -> Sender<()> {
    let (tx, rx) = channel::<()>();
    let wake = tx.clone();
    let _ = std::thread::Builder::new().name("notifs-windows".into()).spawn(move || {
        let mut seen = std::collections::HashSet::<u32>::new();
        let mut last = String::new();
        let mut hooked = false;
        loop {
            let state = if on.load(std::sync::atomic::Ordering::Relaxed) {
                match winnotif_poll(&out, &mut seen, &mut hooked, &wake) {
                    Ok(s) => s,
                    Err(e) => format!("erreur ({e})"),
                }
            } else {
                String::new()
            };
            if state != last {
                crate::sys::log(&format!("notifications Windows : {state}"));
                out.send(Msg::WinNotifState(state.clone()));
                last = state;
            }
            // contrôle léger toutes les 30 s (ou tout de suite si Windows signale un changement)
            match rx.recv_timeout(Duration::from_secs(30)) {
                Err(RecvTimeoutError::Disconnected) => return,
                _ => {
                    std::thread::sleep(Duration::from_millis(300));
                    while rx.try_recv().is_ok() {}
                }
            }
        }
    });
    tx
}

/// Notification de Discord : appli Discord, ou Discord ouvert dans un navigateur (Chrome, Edge…),
/// dont la notification porte la mention « discord.com ».
fn is_discord(n: &windows::UI::Notifications::UserNotification) -> bool {
    let Ok(app) = n.AppInfo() else { return false };
    let id = app.AppUserModelId().map(|s| s.to_string()).unwrap_or_default().to_lowercase();
    let name = app.DisplayInfo().and_then(|d| d.DisplayName()).map(|s| s.to_string()).unwrap_or_default().to_lowercase();
    if id.contains("discord") || name.contains("discord") {
        return true;
    }
    let browser = ["chrome", "msedge", "edge", "brave", "opera", "firefox"].iter().any(|b| id.contains(b) || name.contains(b));
    browser && all_texts(n).iter().any(|t| t.to_lowercase().contains("discord"))
}

fn all_texts(n: &windows::UI::Notifications::UserNotification) -> Vec<String> {
    use windows::UI::Notifications::KnownNotificationBindings;
    (|| -> Option<Vec<String>> {
        let b = n.Notification().ok()?.Visual().ok()?.GetBinding(&KnownNotificationBindings::ToastGeneric().ok()?).ok()?;
        Some(b.GetTextElements().ok()?.into_iter().filter_map(|t| t.Text().ok()).map(|t| t.to_string()).collect())
    })()
    .unwrap_or_default()
}

fn notif_texts(n: &windows::UI::Notifications::UserNotification) -> Option<(String, String)> {
    // la mention « discord.com » ajoutée par le navigateur n'est pas un vrai texte
    let texts: Vec<String> = all_texts(n).into_iter().filter(|t| !t.trim().eq_ignore_ascii_case("discord.com")).collect();
    let title = crate::sys::clean(texts.first()?);
    let body = crate::sys::clean(&texts[1..].join(" "));
    Some((title, body))
}

fn winnotif_poll(
    out: &Out,
    seen: &mut std::collections::HashSet<u32>,
    hooked: &mut bool,
    wake: &Sender<()>,
) -> windows::core::Result<String> {
    use windows::UI::Notifications::Management::{UserNotificationListener as L, UserNotificationListenerAccessStatus as A};
    use windows::UI::Notifications::NotificationKinds;
    let l = L::Current()?;
    let mut st = l.GetAccessStatus()?;
    if st == A::Unspecified {
        st = l.RequestAccessAsync()?.get()?;
    }
    if st != A::Allowed {
        return Ok("accès refusé : Paramètres Windows › Confidentialité › Notifications".into());
    }
    if !*hooked {
        *hooked = true;
        let w = wake.clone();
        // pas toujours disponible pour une appli de bureau : dans ce cas on garde le contrôle toutes les 30 s
        if let Err(e) = l.NotificationChanged(&windows::Foundation::TypedEventHandler::new(move |_, _| {
            let _ = w.send(());
            Ok(())
        })) {
            crate::sys::log(&format!("notifications Windows : pas d'événement direct ({e}), contrôle toutes les 30 s"));
        }
    }
    let list = l.GetNotificationsAsync(NotificationKinds::Toast)?.get()?;
    let mut new = Vec::new();
    for n in list {
        let Ok(id) = n.Id() else { continue };
        if seen.contains(&id) || !is_discord(&n) {
            continue;
        }
        seen.insert(id);
        let Some((title, body)) = notif_texts(&n) else { continue };
        // message système de Discord (« toujours en cours d'exécution ») : pas un vrai message
        if title == "Discord" && body.contains("en cours d") {
            continue;
        }
        // DateTime Windows : centaines de ns depuis 1601
        let secs = n.CreationTime().map(|d| d.UniversalTime / 10_000_000 - 11_644_473_600).unwrap_or(0);
        let time = chrono::DateTime::from_timestamp(secs, 0)
            .map(|d| d.with_timezone(&Local).naive_local())
            .unwrap_or_else(|| Local::now().naive_local());
        new.push(Notif { title, body, time });
    }
    new.sort_by_key(|n| n.time);
    for n in new {
        out.send(Msg::Discord(n));
    }
    Ok("activées".into())
}

/// `givre.exe --notif-test` : vérifie l'accès aux notifications sans rien afficher de privé.
pub fn notif_selftest() {
    use windows::UI::Notifications::Management::UserNotificationListener as L;
    use windows::UI::Notifications::NotificationKinds;
    let r = (|| -> windows::core::Result<String> {
        let l = L::Current()?;
        let st = l.GetAccessStatus()?;
        let mut s = format!("accès : {:?}", st.0);
        if st == windows::UI::Notifications::Management::UserNotificationListenerAccessStatus::Allowed {
            let list = l.GetNotificationsAsync(NotificationKinds::Toast)?.get()?;
            let total = list.Size()?;
            let dc = list.into_iter().filter(is_discord).count();
            s += &format!(", {total} notification(s) dont {dc} de Discord");
        }
        Ok(s)
    })();
    println!("{}", r.unwrap_or_else(|e| format!("erreur : {e}")));
}

// ---------- Imprimante 3D (Elegoo, protocole SDCP sur WebSocket) ----------

pub fn spawn_printer(out: Out, ip: Arc<Mutex<String>>) {
    let _ = std::thread::Builder::new().name("imprimante".into()).spawn(move || {
        let mut last = Printer::default();
        loop {
            let addr = ip.lock().unwrap().trim().to_string();
            if !addr.is_empty() {
                let _ = printer_session(&addr, &out, &mut last);
                if last.online {
                    last.online = false;
                    out.send(Msg::Printer(last.clone()));
                }
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}

fn sdcp_request(mainboard: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!(
        r#"{{"Id":"","Data":{{"Cmd":0,"Data":{{}},"RequestID":"{:032x}","MainboardID":"{mainboard}","TimeStamp":{},"From":1}},"Topic":"sdcp/request/{mainboard}"}}"#,
        now.as_nanos(),
        now.as_secs()
    )
}

fn printer_session(ip: &str, out: &Out, last: &mut Printer) -> Result<(), Box<dyn std::error::Error>> {
    use tungstenite::{Error as WsError, Message};
    let addr = (ip, 3030).to_socket_addrs()?.next().ok_or("adresse invalide")?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(4))?;
    tcp.set_read_timeout(Some(Duration::from_secs(5)))?;
    let (mut ws, _) = tungstenite::client(format!("ws://{ip}:3030/websocket").as_str(), tcp).map_err(|e| e.to_string())?;
    let mut mainboard = String::new();
    ws.send(Message::text(sdcp_request(&mainboard)))?;
    let (mut last_ping, mut last_rx, mut last_sent) = (Instant::now(), Instant::now(), Instant::now() - Duration::from_secs(60));
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                last_rx = Instant::now();
                let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                if mainboard.is_empty() {
                    if let Some(m) = v["MainboardID"].as_str() {
                        mainboard = m.to_string();
                    }
                }
                let st = &v["Status"];
                if st.is_null() {
                    continue;
                }
                let pi = &st["PrintInfo"];
                let ticks = pi["CurrentTicks"].as_f64().unwrap_or(0.0);
                let total = pi["TotalTicks"].as_f64().unwrap_or(0.0);
                let p = Printer {
                    online: true,
                    status: pi["Status"].as_i64().unwrap_or(0),
                    progress: pi["Progress"].as_f64().unwrap_or(0.0).round() as i64,
                    layer: pi["CurrentLayer"].as_i64().unwrap_or(0),
                    total: pi["TotalLayer"].as_i64().unwrap_or(0),
                    left_min: ((total - ticks).max(0.0) / 60.0).round() as i64,
                    file: pi["Filename"].as_str().unwrap_or("").to_string(),
                };
                // n'envoie que les vrais changements (au plus ~1 fois par minute pour le temps restant)
                let mut cmp = p.clone();
                cmp.left_min = last.left_min;
                if cmp != *last || (p.left_min != last.left_min && last_sent.elapsed() > Duration::from_secs(55)) {
                    *last = p.clone();
                    last_sent = Instant::now();
                    out.send(Msg::Printer(p));
                }
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => last_rx = Instant::now(),
            Err(WsError::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.into()),
        }
        if last_ping.elapsed() >= Duration::from_secs(25) {
            ws.send(Message::text("ping"))?;
            ws.send(Message::text(sdcp_request(&mainboard)))?;
            last_ping = Instant::now();
        }
        if last_rx.elapsed() >= Duration::from_secs(70) {
            return Err("plus de réponse".into());
        }
    }
}

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

pub fn is_active(s: i64) -> bool {
    matches!(s, 1 | 5 | 6 | 7 | 10 | 12 | 13 | 15 | 16 | 18 | 19 | 20 | 21)
}
