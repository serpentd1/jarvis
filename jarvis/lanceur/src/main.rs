#![windows_subsystem = "windows"]
//! Jarvis.exe — lance le moteur de Jarvis (Python, dans .venv) sans aucune fenêtre.
//! - une seule instance à la fois ;
//! - relance le moteur s'il plante (mais abandonne s'il plante en boucle) ;
//! - « Redémarrer Jarvis » (code 3) le relance tout de suite, « Quitter » (code 0) arrête tout ;
//! - « Couper la voix » (code 4) : le moteur s'arrête mais la barre reste ; Jarvis.exe attend alors,
//!   presque sans mémoire, que la barre redemande la voix (événement « Local\Jarvis-voix ») ;
//! - « Allumer la voix au démarrage du PC » décoché (fichier %APPDATA%\Jarvis\sans-voix-au-demarrage) :
//!   au démarrage de Windows, seulement la barre ;
//! - si on tue Jarvis.exe, le moteur s'arrête aussi (objet « job » de Windows),
//!   sans emporter les programmes que Jarvis a lancés (Givre, Discord…).

use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, OpenMutexW, WaitForSingleObject};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};

const SANS_FENETRE: u32 = 0x0800_0000;
const CODE_REDEMARRER: i32 = 3;
const CODE_SANS_VOIX: i32 = 4;
/// Droit d'attendre un objet (pour savoir si la barre tourne encore).
const SYNCHRONIZE: u32 = 0x0010_0000;

fn large(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn alerte(texte: &str) {
    unsafe {
        MessageBoxW(std::ptr::null_mut(), large(texte).as_ptr(), large("Jarvis").as_ptr(), MB_OK | MB_ICONWARNING);
    }
}

/// Demande à la barre de Jarvis (127.0.0.1:47802) d'ouvrir la grande île. Sans réponse : rien.
fn ouvrir_grande_ile() {
    use std::io::Write;
    let corps = r#"{"quoi":"grande","section":"accueil"}"#;
    let adresse = std::net::SocketAddr::from(([127, 0, 0, 1], 47802));
    if let Ok(mut flux) = std::net::TcpStream::connect_timeout(&adresse, Duration::from_secs(2)) {
        let _ = flux.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = write!(
            flux,
            "POST /ouvrir HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corps}",
            corps.len()
        );
        let _ = std::io::Read::read(&mut flux, &mut [0u8; 256]);
    }
}

/// La barre de Jarvis (l'île) : à côté de Jarvis.exe une fois installé, sinon la version compilée.
fn barre_exe(dossier: &std::path::Path) -> Option<PathBuf> {
    [dossier.join("JarvisBarre.exe"), dossier.join("barre").join("target").join("release").join("JarvisBarre.exe")]
        .into_iter()
        .find(|p| p.is_file())
}

/// La barre tourne-t-elle ? (elle garde le verrou « JarvisBarre-instance-unique »)
fn barre_vivante() -> bool {
    unsafe {
        let h = OpenMutexW(SYNCHRONIZE, 0, large("JarvisBarre-instance-unique").as_ptr());
        if h.is_null() {
            return false;
        }
        CloseHandle(h);
        true
    }
}

/// Voix coupée : on attend que la barre la redemande. false = la barre s'est fermée (tout s'arrête).
fn attendre_voix(evenement: HANDLE) -> bool {
    let mut absente = 0;
    loop {
        if unsafe { WaitForSingleObject(evenement, 2000) } == WAIT_OBJECT_0 {
            return true;
        }
        // la barre peut se relancer (mise à jour…) : on lui laisse 10 s avant de tout arrêter
        absente = if barre_vivante() { 0 } else { absente + 1 };
        if absente >= 5 {
            return false;
        }
    }
}

/// Démarrage de Windows sans la voix : demandé par la barre (réglage « Allumer la voix au démarrage du PC »).
fn demarrer_sans_voix() -> bool {
    let depuis_windows = std::env::args().any(|a| a == "--demarrage") || unsafe { GetTickCount64() } < 4 * 60 * 1000;
    let fichier = std::env::var("APPDATA").map(|a| PathBuf::from(a).join("Jarvis").join("sans-voix-au-demarrage"));
    depuis_windows && fichier.is_ok_and(|f| f.is_file())
}

fn creer_job() -> Option<HANDLE> {
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return None;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        Some(job)
    }
}

fn main() {
    unsafe {
        CreateMutexW(std::ptr::null(), 0, large("Local\\Jarvis-lanceur").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            // Jarvis tourne déjà : un double-clic sur le raccourci ouvre la grande île au lieu de ne rien faire
            ouvrir_grande_ile();
            return;
        }
    }
    let dossier = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    // On lance directement le vrai python.exe (celui indiqué par .venv\pyvenv.cfg), sans fenêtre, en lui
    // désignant l'environnement de Jarvis (__PYVENV_LAUNCHER__). Pas de pythonw.exe ni de « trampoline » :
    // certains antivirus (F-Secure DeepGuard…) les suppriment ou les bloquent.
    let venv = dossier.join(".venv");
    let scripts = venv.join("Scripts");
    let base = std::fs::read_to_string(venv.join("pyvenv.cfg")).ok().and_then(|cfg| {
        cfg.lines()
            .find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == "home").map(|(_, v)| PathBuf::from(v.trim())))
    });
    let interpretes = [
        base.as_ref().map(|b| (b.join("python.exe"), true)),
        Some((scripts.join("python.exe"), false)),
        Some((scripts.join("pythonw.exe"), false)),
    ];
    let choix = interpretes.into_iter().flatten().find(|(p, _)| p.is_file());
    let script = dossier.join("main.py");
    let Some((python, direct)) = choix.filter(|_| script.is_file()) else {
        alerte(
            "Jarvis ne trouve pas Python à côté de Jarvis.exe.\n\n\
             - Si Jarvis n'est pas encore installé : lance « Installer-Jarvis ».\n\
             - Sinon, ton antivirus a peut-être mis un fichier en quarantaine : restaure-le, \
             ajoute une exception pour le dossier de Jarvis, puis relance l'installeur.",
        );
        return;
    };
    let job = creer_job();
    // la barre fait « SetEvent » sur cet événement pour rallumer la voix
    let evenement = unsafe { CreateEventW(std::ptr::null(), 0, 0, large("Local\\Jarvis-voix").as_ptr()) };
    if demarrer_sans_voix() {
        if !barre_vivante() {
            if let Some(b) = barre_exe(&dossier) {
                let _ = Command::new(&b).current_dir(b.parent().unwrap_or(&dossier)).creation_flags(SANS_FENETRE).spawn();
            }
        }
        std::thread::sleep(Duration::from_secs(3));
        if evenement.is_null() || !attendre_voix(evenement) {
            return;
        }
    }
    let mut plantages: Vec<Instant> = Vec::new();
    loop {
        let mut commande = Command::new(&python);
        commande
            .arg(&script)
            .arg("--service")
            .arg("--lanceur")
            .arg(std::process::id().to_string())
            .current_dir(&dossier)
            .creation_flags(SANS_FENETRE);
        if direct {
            commande.env("__PYVENV_LAUNCHER__", scripts.join("python.exe"));
        }
        let mut moteur = match commande.spawn() {
            Ok(m) => m,
            Err(e) => {
                alerte(&format!("Impossible de démarrer Jarvis : {e}"));
                return;
            }
        };
        if let Some(job) = job {
            unsafe {
                AssignProcessToJobObject(job, moteur.as_raw_handle() as HANDLE);
            }
        }
        match moteur.wait().ok().and_then(|s| s.code()) {
            Some(0) => return,
            Some(CODE_REDEMARRER) => continue,
            Some(CODE_SANS_VOIX) => {
                if evenement.is_null() || !attendre_voix(evenement) {
                    return;
                }
                plantages.clear();
                continue;
            }
            _ => {}
        }
        plantages.retain(|t| t.elapsed() < Duration::from_secs(120));
        plantages.push(Instant::now());
        if plantages.len() >= 4 {
            alerte("Jarvis s'est arrêté plusieurs fois de suite.\n\nRegarde le journal : %APPDATA%\\Jarvis\\jarvis.log");
            return;
        }
        std::thread::sleep(Duration::from_secs(3));
    }
}
