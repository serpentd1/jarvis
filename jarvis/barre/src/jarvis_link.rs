//! Lien avec Jarvis (assistant vocal, API sur 127.0.0.1:47801) : on vérifie qu'il est là toutes les 5 s,
//! on lui envoie les questions du chat de l'île, et on le lance au démarrage si besoin.
//! Aucun en-tête Origin n'est envoyé (Jarvis refuse les requêtes venant d'une page web).

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use crate::data::{Msg, Out};

pub const BASE: &str = "http://127.0.0.1:47801";

fn agent(secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(2)).timeout(Duration::from_secs(secs)).build()
}

/// GET /etat de Jarvis : (état, micro) s'il répond.
pub fn status() -> Option<(String, bool)> {
    let v: Value = agent(3).get(&format!("{BASE}/etat")).call().ok()?.into_json().ok()?;
    if v["ok"].as_bool() == Some(false) {
        return None;
    }
    Some((v["etat"].as_str().unwrap_or("repos").to_string(), v["micro"].as_bool().unwrap_or(true)))
}

pub fn post(path: &str, body: Value, secs: u64) -> Result<Value, String> {
    agent(secs)
        .post(&format!("{BASE}{path}"))
        .send_json(body)
        .map_err(|e| match e {
            ureq::Error::Transport(_) => "Jarvis ne répond pas.".to_string(),
            e => crate::accounts::http_err(e),
        })?
        .into_json::<Value>()
        .map_err(|e| e.to_string())
}

/// Envoie sans attendre la réponse (écouter, micro, oublier, quitter).
pub fn fire(path: &'static str, body: Value) {
    std::thread::spawn(move || {
        if let Err(e) = post(path, body, 5) {
            crate::sys::log(&format!("Jarvis {path} : {e}"));
        }
    });
}

/// Question du chat de l'île posée à Jarvis ; la réponse revient comme une réponse d'IA.
pub fn ask(out: Out, id: u64, texte: String) {
    std::thread::spawn(move || {
        let r = post("/demander", json!({"texte": texte, "source": "ile"}), 90).and_then(|v| {
            v["reponse"].as_str().map(|s| s.to_string()).ok_or_else(|| v["erreur"].as_str().unwrap_or("réponse vide de Jarvis").to_string())
        });
        out.send(Msg::Answer(id, r));
    });
}

/// Fil de surveillance : état de Jarvis toutes les 5 s. Lance Jarvis au démarrage s'il ne répond pas.
pub fn spawn(out: Out, autostart: bool, manual: String) {
    let _ = std::thread::Builder::new().name("jarvis".into()).spawn(move || {
        let mut first = true;
        loop {
            let st = status();
            if first && st.is_none() && autostart {
                launch(&manual);
            }
            first = false;
            out.send(Msg::JarvisLink(st));
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

/// Cherche Jarvis.exe : chemin donné, dossier de givre.exe, puis jarvis\Jarvis.exe en remontant jusqu'à 4 dossiers.
pub fn find_exe(manual: &str) -> Option<PathBuf> {
    let m = manual.trim().trim_matches('"');
    if !m.is_empty() {
        let p = PathBuf::from(m);
        return p.is_file().then_some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();
    let direct = dir.join("Jarvis.exe");
    if direct.is_file() {
        return Some(direct);
    }
    for _ in 0..=4 {
        let p = dir.join("jarvis").join("Jarvis.exe");
        if p.is_file() {
            return Some(p);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

/// Lance Jarvis sans fenêtre. Introuvable : on ne fait rien.
pub fn launch(manual: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Some(p) = find_exe(manual) else {
        crate::sys::log("Jarvis.exe introuvable : pas de lancement");
        return;
    };
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    match std::process::Command::new(&p).current_dir(dir).creation_flags(CREATE_NO_WINDOW).spawn() {
        Ok(_) => crate::sys::log(&format!("Jarvis lancé : {}", p.display())),
        Err(e) => crate::sys::log(&format!("Jarvis : lancement impossible ({e})")),
    }
}

/// Redémarre Jarvis (la voix) sans fermer l'appli : POST /redemarrer (Jarvis.exe relance le moteur).
/// S'il ne revient pas (ou ne répondait pas), on le relance nous-mêmes.
pub fn restart(manual: String) {
    std::thread::spawn(move || {
        if post("/redemarrer", json!({}), 5).is_ok() {
            std::thread::sleep(Duration::from_secs(3));
            for _ in 0..20 {
                if status().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        launch(&manual);
    });
}

/// Retire les balises <think>…</think> qu'ajoutent certains modèles.
pub fn strip_think(s: &str) -> String {
    let mut out = s.to_string();
    while let (Some(a), Some(b)) = (out.find("<think>"), out.find("</think>")) {
        if b < a {
            break;
        }
        out.replace_range(a..b + "</think>".len(), "");
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn balises_think() {
        assert_eq!(super::strip_think("<think>hmm\nbon</think>\nBonjour !"), "Bonjour !");
        assert_eq!(super::strip_think("Salut"), "Salut");
    }
}
