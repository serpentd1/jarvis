//! Pouvoirs de Jarvis sur le PC, en Rust : fichiers et dossiers, code des projets, pages web, programmes,
//! PowerShell, processus, extinction du PC.
//!
//! Sécurité (réglages « pouvoirs » de config.json, et menu de l'icône) :
//! - lire, lister, créer un NOUVEAU fichier/dossier, lancer un programme par son nom : tout de suite ;
//! - écraser, modifier, supprimer, ouvrir un .exe/.bat…, tuer un processus, éteindre/redémarrer/veille,
//!   PowerShell qui change quelque chose : une QUESTION est renvoyée, et rien n'est fait avant le « oui » ;
//! - supprimer = corbeille (par défaut) ; avant chaque écrasement ou modification, une copie de sauvegarde ;
//! - les dossiers du système (Windows, Program Files, ProgramData…) sont interdits en écriture ;
//! - PowerShell est coupé au bout de `delai_powershell` secondes ; les commandes dangereuses sont refusées ;
//! - tout est noté dans %APPDATA%\Jarvis\donnees\actions.log.
//!
//! Utilisé par : le chat de l'île (cerveau Rust, actions « pc ») et l'API locale (GET /outils,
//! POST /outil, POST /outil/confirmer) que la voix de Jarvis appelle.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const SANS_FENETRE: u32 = 0x0800_0000;
const LECTURE_MAX: usize = 20_000;
const MODIF_IA_MAX: usize = 40_000;
const CONFIRMATION_VALIDE: Duration = Duration::from_secs(180);
const PROGRAMMES: [&str; 13] = ["exe", "bat", "cmd", "ps1", "vbs", "msi", "com", "scr", "js", "jse", "wsf", "reg", "lnk"];
const PROCESSUS_PROTEGES: [&str; 18] = [
    "system", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "svchost.exe",
    "dwm.exe", "explorer.exe", "fontdrvhost.exe", "registry", "memory compression", "jarvis.exe", "jarvisbarre.exe",
    "python.exe", "pythonw.exe", "msmpeng.exe",
];

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct PouvoirsCfg {
    /// Tout couper d'un coup.
    pub actif: bool,
    pub powershell: bool,
    pub delai_powershell: f32,
    /// Dossier des fichiers donnés sans chemin (vide = Documents\Jarvis).
    pub dossier_travail: String,
    /// Supprimer = corbeille (sinon suppression définitive, toujours après confirmation).
    pub corbeille: bool,
    /// Dossiers interdits en plus de ceux du système.
    pub chemins_interdits: Vec<String>,
}

impl Default for PouvoirsCfg {
    fn default() -> Self {
        PouvoirsCfg { actif: true, powershell: true, delai_powershell: 30.0, dossier_travail: String::new(), corbeille: true, chemins_interdits: Vec::new() }
    }
}

/// Résultat d'un outil.
#[derive(Debug, Clone, PartialEq)]
pub enum Sortie {
    Fait(String),
    Refus(String),
    /// Il faut un « oui » : (numéro de la question, question).
    Question(u64, String),
}

impl Sortie {
    pub fn texte(&self) -> &str {
        match self {
            Sortie::Fait(t) | Sortie::Refus(t) | Sortie::Question(_, t) => t,
        }
    }
    pub fn json(&self) -> Value {
        match self {
            Sortie::Fait(t) => json!({"ok": true, "dire": t}),
            Sortie::Refus(t) => json!({"ok": false, "dire": t}),
            Sortie::Question(id, t) => json!({"ok": true, "dire": t, "confirmer": id}),
        }
    }
}

type Action = Box<dyn FnOnce() -> Sortie + Send>;

struct Etat {
    cfg: PouvoirsCfg,
    attente: HashMap<u64, (Instant, Action)>,
    suivant: u64,
}

static ETAT: Mutex<Option<Etat>> = Mutex::new(None);
/// Le dernier appel demandé (outil, arguments), pour « réessaie l'appel ».
static DERNIER_APPEL: Mutex<Option<(String, Value)>> = Mutex::new(None);

fn avec<R>(f: impl FnOnce(&mut Etat) -> R) -> R {
    let mut g = ETAT.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.get_or_insert_with(|| Etat {
        cfg: PouvoirsCfg::default(),
        attente: HashMap::new(),
        suivant: 0,
    });
    f(e)
}

/// Réglages des pouvoirs (appelé au démarrage et à chaque changement). L'IA est choisie dans ia.rs.
pub fn regler(cfg: PouvoirsCfg) {
    avec(|e| e.cfg = cfg);
}

fn cfg() -> PouvoirsCfg {
    avec(|e| e.cfg.clone())
}

fn question(texte: String, action: Action) -> Sortie {
    let id = avec(|e| {
        e.attente.retain(|_, (t, _)| t.elapsed() < CONFIRMATION_VALIDE);
        e.suivant += 1;
        e.attente.insert(e.suivant, (Instant::now(), action));
        e.suivant
    });
    Sortie::Question(id, texte)
}

/// Réponse à une question : `oui` exécute l'action en attente, `non` l'oublie.
pub fn confirmer(id: u64, oui: bool) -> Sortie {
    let action = avec(|e| e.attente.remove(&id));
    match action {
        Some((t, a)) if oui && t.elapsed() < CONFIRMATION_VALIDE => a(),
        Some(_) if oui => Sortie::Refus("J'ai attendu trop longtemps : redemande-moi.".into()),
        Some(_) => Sortie::Fait("D'accord, je ne touche à rien.".into()),
        None => Sortie::Refus("Il n'y a plus rien à confirmer.".into()),
    }
}

// ───────────────────────── chemins ─────────────────────────

fn large(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn sans_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'À' | 'Â' => 'A',
            'É' | 'È' | 'Ê' => 'E',
            'Ç' => 'C',
            c => c,
        })
        .collect()
}

fn dossier_connu(csidl: i32, secours: &str) -> PathBuf {
    let mut buf = [0u16; 260];
    let r = unsafe { windows_sys::Win32::UI::Shell::SHGetFolderPathW(std::ptr::null_mut(), csidl, std::ptr::null_mut(), 0, buf.as_mut_ptr()) };
    let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
    if r == 0 && n > 0 {
        return PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    }
    maison().join(secours)
}

fn maison() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\"))
}

/// bureau, documents, téléchargements, images, musique, vidéos.
pub fn alias(mot: &str) -> Option<PathBuf> {
    let m = sans_accents(mot.trim()).to_lowercase();
    let m = m.trim_start_matches("mes ").trim_start_matches("mon ").trim_start_matches("ma ").trim_start_matches("le ");
    Some(match m {
        "bureau" | "desktop" => dossier_connu(0x10, "Desktop"),
        "documents" | "document" => dossier_connu(0x05, "Documents"),
        "telechargements" | "telechargement" | "downloads" => maison().join("Downloads"),
        "images" | "photos" | "pictures" => dossier_connu(0x27, "Pictures"),
        "musique" | "musiques" | "music" => dossier_connu(0x0D, "Music"),
        "videos" | "video" => dossier_connu(0x0E, "Videos"),
        _ => return None,
    })
}

fn developper_variables(s: &str) -> String {
    let mut out = String::new();
    let mut reste = s;
    while let Some(a) = reste.find('%') {
        let Some(b) = reste[a + 1..].find('%') else { break };
        let nom = &reste[a + 1..a + 1 + b];
        out.push_str(&reste[..a]);
        match std::env::var(nom) {
            Ok(v) if !nom.is_empty() => out.push_str(&v),
            _ => out.push_str(&reste[a..a + b + 2]),
        }
        reste = &reste[a + b + 2..];
    }
    out.push_str(reste);
    if let Some(r) = out.strip_prefix('~') {
        return format!("{}{}", maison().display(), r);
    }
    out
}

pub fn dossier_travail(c: &PouvoirsCfg) -> PathBuf {
    let d = c.dossier_travail.trim();
    if d.is_empty() { dossier_connu(0x05, "Documents").join("Jarvis") } else { PathBuf::from(developper_variables(d)) }
}

/// « bureau\notes.txt », « ~\x », « %USERPROFILE%\x », « C:\x » ou « notes.txt » (dossier de travail).
pub fn resoudre(chemin: &str, base: &Path) -> PathBuf {
    let c = chemin.trim().trim_matches(|ch: char| "\"'«» ".contains(ch)).replace('/', "\\");
    let c = developper_variables(&c);
    if c.is_empty() {
        return base.to_path_buf();
    }
    let p = PathBuf::from(&c);
    if p.is_absolute() {
        return p;
    }
    let (premier, reste) = c.split_once('\\').unwrap_or((&c, ""));
    if let Some(d) = alias(premier) {
        return if reste.is_empty() { d } else { d.join(reste) };
    }
    base.join(p)
}

fn zones_interdites(extra: &[String]) -> Vec<PathBuf> {
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let sys = env("SystemDrive", "C:") + "\\";
    let mut z: Vec<PathBuf> = vec![
        env("SystemRoot", "C:\\Windows").into(),
        env("ProgramFiles", "C:\\Program Files").into(),
        env("ProgramFiles(x86)", "C:\\Program Files (x86)").into(),
        env("ProgramW6432", "C:\\Program Files").into(),
        env("ProgramData", "C:\\ProgramData").into(),
        PathBuf::from(&sys).join("$Recycle.Bin"),
        PathBuf::from(&sys).join("System Volume Information"),
        PathBuf::from(&sys).join("Recovery"),
        PathBuf::from(&sys).join("Boot"),
    ];
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        z.push(PathBuf::from(l).join("Microsoft"));
    }
    if let Ok(a) = std::env::var("APPDATA") {
        z.push(PathBuf::from(a).join("Microsoft"));
    }
    z.push(maison().join("NTUSER.DAT"));
    z.extend(extra.iter().filter(|x| !x.trim().is_empty()).map(|x| PathBuf::from(developper_variables(x.trim()))));
    z
}

fn absolu(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Raison du refus si `p` touche au système (écrire, supprimer, déplacer), sinon None.
pub fn interdit(p: &Path, extra: &[String]) -> Option<String> {
    let p = absolu(p);
    let racine = p.ancestors().last().map(|r| r.to_path_buf()).unwrap_or_default();
    if p.parent().is_none() || p == racine {
        return Some("c'est la racine d'un disque".into());
    }
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if p.parent().is_some_and(|d| d.parent().is_none()) && matches!(ext.as_str(), "sys" | "ini" | "dat") {
        return Some("c'est un fichier du système".into());
    }
    let bas = p.to_string_lossy().to_lowercase().trim_end_matches('\\').to_string();
    for z in zones_interdites(extra) {
        let zb = absolu(&z).to_string_lossy().to_lowercase().trim_end_matches('\\').to_string();
        if zb.len() > 3 && (bas == zb || bas.starts_with(&(zb.clone() + "\\"))) {
            return Some(format!("c'est un dossier protégé du système ({})", z.display()));
        }
    }
    None
}

// ───────────────────────── PowerShell ─────────────────────────

const LECTURE_PS: [&str; 34] = [
    "get-", "test-", "measure-", "select-", "where-object", "sort-object", "group-object", "format-table", "format-list",
    "format-wide", "out-string", "write-output", "write-host", "convertto-", "convertfrom-", "resolve-path",
    "split-path", "join-path", "compare-object", "dir", "ls", "gci", "gc", "cat", "type", "pwd", "whoami", "hostname",
    "ipconfig", "ping", "echo", "systeminfo", "tasklist", "$env:",
];
const INTERDIT_PS: [&str; 30] = [
    "format-volume", "clear-disk", "initialize-disk", "remove-partition", "diskpart", "bcdedit", "vssadmin", "wbadmin",
    "cipher /w", "set-mppreference", "add-mppreference", "disable-computerrestore", "reagentc", "takeown", "icacls",
    "set-executionpolicy", "invoke-webrequest", "iwr ", "invoke-restmethod", "irm ", "downloadstring", "downloadfile",
    "start-bitstransfer", "-encodedcommand", " -enc ", "frombase64string", "reg delete hklm", "reg.exe delete hklm",
    "hklm:", "net user",
];

/// Vrai si la commande ne fait que LIRE (Get-…, dir, ipconfig…) : pas besoin de confirmation.
pub fn ps_lecture_seule(commande: &str) -> bool {
    let c = commande.trim().to_lowercase();
    if c.is_empty() || ["{", "}", ">", "`", "$(", "iex", "invoke-", "start-", "&", "@(", "get-credential"].iter().any(|x| c.contains(x)) {
        return false;
    }
    c.split([';', '\n']).flat_map(|i| i.split('|')).all(|etape| {
        let mot = etape.split_whitespace().next().unwrap_or("");
        mot.is_empty()
            || LECTURE_PS.iter().any(|l| if l.ends_with('-') || l.ends_with(':') { mot.starts_with(l) } else { mot == *l })
    })
}

/// Commandes toujours refusées (abîment Windows, désactivent l'antivirus, téléchargent…).
pub fn ps_interdit(commande: &str) -> bool {
    let c = format!(" {} ", commande.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" "));
    if INTERDIT_PS.iter().any(|x| c.contains(x)) {
        return true;
    }
    let efface = ["remove-item", " del ", " rd ", " rm ", "rmdir", " erase "].iter().any(|x| c.contains(x));
    let systeme = ["\\windows", "system32", "program files", "programfiles", "programdata"].iter().any(|x| c.contains(x));
    efface && systeme
}

/// Lance PowerShell sans fenêtre. (code, sortie) ; code None = coupé parce que trop long.
pub fn executer_ps(commande: &str, delai: Duration) -> (Option<i32>, String) {
    use std::os::windows::process::CommandExt;
    let script = format!("[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ProgressPreference='SilentlyContinue'; {commande}");
    let enfant = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(SANS_FENETRE)
        .spawn();
    let mut enfant = match enfant {
        Ok(e) => e,
        Err(e) => return (Some(-1), format!("PowerShell introuvable : {e}")),
    };
    let lire = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut v = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut v);
            }
            v
        })
    };
    let out = lire(enfant.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let err = lire(enfant.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let debut = Instant::now();
    let code = loop {
        match enfant.try_wait() {
            Ok(Some(s)) => break Some(s.code().unwrap_or(-1)),
            Ok(None) if debut.elapsed() < delai => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = Command::new("taskkill").args(["/F", "/T", "/PID", &enfant.id().to_string()]).creation_flags(SANS_FENETRE).output();
                let _ = enfant.kill();
                let _ = enfant.wait();
                break None;
            }
        }
    };
    let mut texte = String::from_utf8_lossy(&out.join().unwrap_or_default()).to_string();
    texte.push_str(&String::from_utf8_lossy(&err.join().unwrap_or_default()));
    (code, texte.trim().to_string())
}

// ───────────────────────── petits outils ─────────────────────────

fn journal_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("Jarvis").join("donnees")
}

pub fn journaliser(action: &str, detail: &str, ok: bool) {
    use std::io::Write;
    let d = journal_dir();
    let _ = std::fs::create_dir_all(&d);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join("actions.log")) {
        let _ = writeln!(f, "{}\t{}\t{action}\t{detail}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), if ok { "ok" } else { "ÉCHEC" });
    }
    crate::sys::log(&format!("pouvoirs : {action} {detail}"));
}

/// Copie de sécurité avant d'écraser ou de modifier un fichier.
pub fn sauvegarder(p: &Path) -> Option<PathBuf> {
    if !p.is_file() {
        return None;
    }
    let d = journal_dir().join("sauvegardes");
    std::fs::create_dir_all(&d).ok()?;
    let copie = d.join(format!("{}_{}", chrono::Local::now().format("%Y%m%d-%H%M%S"), p.file_name()?.to_string_lossy()));
    std::fs::copy(p, &copie).ok()?;
    Some(copie)
}

/// Met à la corbeille (récupérable).
pub fn corbeille(p: &Path) -> bool {
    use windows_sys::Win32::UI::Shell::*;
    let mut de: Vec<u16> = absolu(p).as_os_str().to_string_lossy().encode_utf16().collect();
    de.extend([0, 0]);
    let mut op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
    op.wFunc = FO_DELETE;
    op.pFrom = de.as_ptr();
    op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as u16;
    unsafe { SHFileOperationW(&mut op) == 0 && op.fAnyOperationsAborted == 0 && !p.exists() }
}

fn ouvrir(p: &Path) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let r = unsafe {
        ShellExecuteW(std::ptr::null_mut(), large("open").as_ptr(), large(&p.to_string_lossy()).as_ptr(), std::ptr::null(), std::ptr::null(), 1)
    };
    r as isize > 32
}

pub fn nom_fichier(texte: &str) -> String {
    let mut s = String::new();
    for c in sans_accents(texte).to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    let s: String = s.trim_matches('-').chars().take(50).collect();
    if s.is_empty() { "page".into() } else { s }
}

/// Le code d'un bloc ``` ``` (ou tout le texte s'il n'y en a pas).
pub fn extraire_code(texte: &str) -> String {
    let t = crate::jarvis_link::strip_think(texte);
    if let Some(a) = t.find("```") {
        let apres = &t[a + 3..];
        let debut = apres.find('\n').map(|i| i + 1).unwrap_or(0);
        if let Some(b) = apres[debut..].find("```") {
            return apres[debut..debut + b].trim_end().to_string() + "\n";
        }
    }
    t.trim().to_string() + "\n"
}

/// Demande à l'IA (ia.rs : LM Studio, Ollama ou une clé d'API) de réécrire un fichier ou de créer une page.
fn ia(systeme: &str, demande: &str) -> Result<String, String> {
    let req = crate::ia::Requete {
        messages: vec![json!({"role": "system", "content": systeme}), json!({"role": "user", "content": demande})],
        outils: Vec::new(),
        max_mots: 8000,
        temperature: 0.3,
        json: false,
        rapide: false,
        contexte: 16384,
        garder: "10m".into(),
    };
    match crate::ia::discuter(&req, None) {
        Ok(x) => Ok(x.contenu),
        Err(crate::ia::Echec::Injoignable) => Err("Je n'arrive à joindre aucune IA (LM Studio, Ollama ou une clé d'API).".into()),
        Err(crate::ia::Echec::Autre(t)) => Err(format!("IA : {t}")),
    }
}

fn nom(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

fn copier_dossier(de: &Path, vers: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(vers)?;
    for e in std::fs::read_dir(de)? {
        let e = e?;
        let cible = vers.join(e.file_name());
        if e.file_type()?.is_dir() { copier_dossier(&e.path(), &cible)? } else { std::fs::copy(e.path(), cible).map(|_| ())? }
    }
    Ok(())
}

fn diff_lignes(avant: &str, apres: &str) -> usize {
    let a: Vec<&str> = avant.lines().collect();
    let b: Vec<&str> = apres.lines().collect();
    let communes = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    let fin = a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count().min(a.len().min(b.len()) - communes);
    (a.len() - communes - fin).max(b.len() - communes - fin)
}

// ───────────────────────── les outils ─────────────────────────

struct Outil {
    nom: &'static str,
    description: &'static str,
    /// (nom, description, obligatoire)
    params: &'static [(&'static str, &'static str, bool)],
}

const CH: &str = "chemin : « bureau\\x.txt », « documents\\projet », « C:\\…\\x », ou juste un nom (dossier Documents\\Jarvis)";

const OUTILS: [Outil; 35] = [
    Outil { nom: "alarme_telephone", description: "Crée une alarme dans l'appli Horloge du TÉLÉPHONE (pas du PC).", params: &[("heure", "heure, ex. 07:30", true), ("nom", "nom de l'alarme", false)] },
    Outil { nom: "minuteur_telephone", description: "Lance un minuteur sur le TÉLÉPHONE (pas sur le PC).", params: &[("duree", "durée, ex. 5 minutes", true), ("nom", "nom du minuteur", false)] },
    Outil { nom: "sonner_telephone", description: "Fait sonner le TÉLÉPHONE à fond (même en silencieux), avec vibration et flash, pour le retrouver.", params: &[] },
    Outil { nom: "arreter_sonnerie_telephone", description: "Arrête la sonnerie du téléphone.", params: &[] },
    Outil { nom: "localiser_telephone", description: "Dit où est le téléphone (même Wi-Fi ? dernière position, batterie).", params: &[] },
    Outil { nom: "batterie_telephone", description: "Dit la batterie du téléphone.", params: &[] },
    Outil { nom: "messages_telephone", description: "Lit les notifications reçues sur le téléphone (SMS, WhatsApp, Discord…) : filtre = importants, sms ou tous.", params: &[("filtre", "importants, sms ou tous", false)] },
    Outil { nom: "mode_telephone", description: "Met le téléphone en silencieux, vibreur ou sonnerie.", params: &[("mode", "silencieux, vibreur ou sonnerie", true)] },
    Outil { nom: "lampe_telephone", description: "Allume ou éteint la lampe torche du téléphone.", params: &[("etat", "allumer ou eteindre", true)] },
    Outil { nom: "photo_telephone", description: "Ouvre l'appareil photo du téléphone.", params: &[] },
    Outil { nom: "repondre_telephone", description: "Envoie un message depuis le téléphone (réponse WhatsApp/SMS…) à quelqu'un, après confirmation.", params: &[("a", "nom de la personne", true), ("texte", "message à envoyer", true)] },
    Outil { nom: "appeler_telephone", description: "Appelle quelqu'un avec le téléphone : appel normal, ou appel vocal WhatsApp (appli = whatsapp). Toujours après confirmation.", params: &[("a", "nom ou numéro", true), ("appli", "telephone (par défaut) ou whatsapp", false)] },
    Outil { nom: "appeler_whatsapp_pc", description: "Lance un appel vocal WHATSAPP depuis l'appli WhatsApp du PC (quand l'utilisateur dit « avec WhatsApp », « sur WhatsApp », « depuis WhatsApp »). Toujours après confirmation.", params: &[("a", "nom du contact ou numéro", true)] },
    Outil { nom: "message_vers_telephone", description: "Affiche une notification sur le téléphone (un rappel, un texte à garder).", params: &[("texte", "texte à afficher", true)] },

    Outil { nom: "lister_dossier", description: "Liste ce qu'il y a dans un dossier.", params: &[("chemin_dossier", CH, false)] },
    Outil { nom: "lire_fichier", description: "Lit un fichier texte ou de code.", params: &[("chemin_fichier", CH, true)] },
    Outil { nom: "creer_fichier", description: "Crée (ou écrase, après confirmation) un fichier avec ce contenu.", params: &[("chemin_fichier", CH, true), ("contenu", "texte complet du fichier", false)] },
    Outil { nom: "ajouter_au_fichier", description: "Ajoute du texte à la fin d'un fichier.", params: &[("chemin_fichier", CH, true), ("texte", "texte à ajouter", true)] },
    Outil { nom: "remplacer_dans_fichier", description: "Remplace un texte exact par un autre dans un fichier (après confirmation).", params: &[("chemin_fichier", CH, true), ("ancien", "texte exact à trouver", true), ("nouveau", "texte de remplacement", true)] },
    Outil { nom: "modifier_fichier_ia", description: "Modifie un fichier de code ou une page selon une consigne (l'IA réécrit, l'utilisateur confirme).", params: &[("chemin_fichier", CH, true), ("consigne", "ce qu'il faut changer", true)] },
    Outil { nom: "creer_dossier", description: "Crée un dossier.", params: &[("chemin_dossier", CH, true)] },
    Outil { nom: "deplacer", description: "Déplace ou renomme un fichier ou un dossier.", params: &[("source", CH, true), ("destination", "nouveau chemin ou dossier", true)] },
    Outil { nom: "copier_fichier", description: "Copie un fichier ou un dossier.", params: &[("source", CH, true), ("destination", "chemin ou dossier de destination", true)] },
    Outil { nom: "supprimer", description: "Met un fichier ou un dossier à la corbeille (après confirmation).", params: &[("chemin_element", CH, true)] },
    Outil { nom: "ouvrir_fichier", description: "Ouvre un fichier ou un dossier avec son programme (page web = navigateur).", params: &[("chemin_element", CH, true)] },
    Outil { nom: "creer_page_web", description: "Crée une page web (HTML, CSS, JavaScript) sur un sujet et l'ouvre dans le navigateur.", params: &[("sujet", "ce que doit contenir la page", true), ("nom", "nom court de la page", false)] },
    Outil { nom: "modifier_page_web", description: "Modifie une page web créée par Jarvis selon une consigne, puis la rouvre.", params: &[("nom", "nom de la page", true), ("consigne", "ce qu'il faut changer", true)] },
    Outil { nom: "executer_powershell", description: "Exécute une commande PowerShell et dit le résultat (confirmation si elle modifie le PC).", params: &[("commande", "commande PowerShell", true)] },
    Outil { nom: "fermer_processus", description: "Force la fermeture d'un processus par son nom (ex. chrome.exe), après confirmation.", params: &[("nom", "nom du processus", true)] },
    Outil { nom: "lancer_programme", description: "Lance un programme ou une application par son nom (Chrome, Discord, Bloc-notes…).", params: &[("nom", "nom de l'application", true)] },
    Outil { nom: "alimentation_pc", description: "Éteint, redémarre, met en veille ou verrouille le PC (confirmation sauf verrouiller), ou annule une extinction prévue.", params: &[("action", "eteindre, redemarrer, veille, verrouiller ou annuler", true)] },
    Outil { nom: "reessayer_appel", description: "Refait le dernier appel (téléphone ou WhatsApp), par exemple quand il n'a pas marché.", params: &[] },
    Outil { nom: "lister_fenetres", description: "Dit quelles applications et fenêtres sont ouvertes sur le PC (et les onglets visibles du navigateur).", params: &[] },
    Outil { nom: "premier_plan", description: "Met une application ou une fenêtre déjà ouverte au premier plan, devant les autres (navigateur, Discord, Spotify…).", params: &[("nom", "application ou titre de la fenêtre, ex. navigateur", true)] },
    Outil { nom: "ranger_dossier", description: "Range un dossier (le bureau par défaut) : met les fichiers dans des dossiers par type (Images, Documents, Vidéos, Musique, Archives), après confirmation. Ne touche pas aux raccourcis ni aux programmes.", params: &[("chemin_dossier", "dossier à ranger, bureau par défaut", false)] },
];

/// Schémas des outils au format d'Ollama (« tools »).
pub fn outils() -> Value {
    Value::Array(
        OUTILS
            .iter()
            .map(|o| {
                let props: serde_json::Map<String, Value> = o.params.iter().map(|(n, d, _)| (n.to_string(), json!({"type": "string", "description": d}))).collect();
                let req: Vec<&str> = o.params.iter().filter(|p| p.2).map(|p| p.0).collect();
                json!({"type": "function", "function": {"name": o.nom, "description": o.description,
                    "parameters": {"type": "object", "properties": props, "required": req}}})
            })
            .collect(),
    )
}

/// Liste courte pour la consigne du cerveau de l'île (actions JSON).
pub fn resume_outils() -> String {
    OUTILS
        .iter()
        .map(|o| format!("{}({}) : {}", o.nom, o.params.iter().map(|p| p.0).collect::<Vec<_>>().join(", "), o.description))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Raccourcis vocaux (pour la voix, encore en Python : le pont pc_rust.py les enregistre tels quels).
/// Le motif s'applique au texte normalisé (minuscules, sans accents) ; "$x" = groupe x, "$$" = toute la phrase.
pub fn regles() -> Value {
    let tel = r"(?:mon |le |ton )?(?:telephone|portable|tel|smartphone)";
    json!([
        {"motif": format!(r"^(?:fais|fait|faire) sonner {tel}\b"), "outil": "sonner_telephone", "args": {}, "priorite": 5},
        {"motif": r"^(?:arrete|stop|stoppe|coupe)(?: de)? (?:sonner|la sonnerie)|^c'est bon je l'ai (?:trouve|retrouve)", "outil": "arreter_sonnerie_telephone", "args": {}, "priorite": 5},
        {"motif": format!(r"^(?:ou est|ou se trouve|j'ai perdu|je trouve plus|je ne trouve plus|retrouve) {tel}"), "outil": "localiser_telephone", "args": {}, "priorite": 5},
        {"motif": format!(r"\b(?:alarme|reveil|reveille)\b.*\b(?:sur|dans) {tel}\b|\b(?:sur|dans) {tel}\b.*\b(?:alarme|reveil)\b"), "outil": "alarme_telephone", "args": {"heure": "$$"}, "priorite": 5},
        {"motif": format!(r"\b(?:minuteur|minuterie|timer)\b.*\b(?:sur|dans) {tel}\b|\b(?:sur|dans) {tel}\b.*\b(?:minuteur|minuterie|timer)\b"), "outil": "minuteur_telephone", "args": {"duree": "$$"}, "priorite": 5},
        {"motif": format!(r"\bbatterie\b.*{tel}\b"), "outil": "batterie_telephone", "args": {}, "priorite": 5},
        {"motif": r"\b(?:messages?|notifs?|notifications?|sms)\b.*\bimportants?\b|\bimportants?\b.*\b(?:messages?|telephone|portable)\b", "outil": "messages_telephone", "args": {"filtre": "importants"}, "priorite": 5},
        {"motif": r"^(?:lis|lit|lire|montre)(?: moi)? (?:mes |les )?(?:derniers? )?sms", "outil": "messages_telephone", "args": {"filtre": "sms"}, "priorite": 5},
        {"motif": format!(r"^mets? {tel} en (?P<mode>silencieux|vibreur|sonnerie|normal)"), "outil": "mode_telephone", "args": {"mode": "$mode"}, "priorite": 5},
        {"motif": format!(r"^(?:allume|active|ouvre) (?:la )?(?:lampe torche|lampe|torche|flash) (?:du|de mon|de ton) (?:telephone|portable|tel)"), "outil": "lampe_telephone", "args": {"etat": "allumer"}, "priorite": 5},
        {"motif": format!(r"^(?:eteins|coupe|ferme) (?:la )?(?:lampe torche|lampe|torche|flash) (?:du|de mon|de ton) (?:telephone|portable|tel)"), "outil": "lampe_telephone", "args": {"etat": "eteindre"}, "priorite": 5},
        {"motif": format!(r"^(?:prends|prend) (?:une )?photo(?: (?:avec|sur) {tel})?$"), "outil": "photo_telephone", "args": {}, "priorite": 5},
        // « avec WhatsApp » = l'appli WhatsApp du PC ; « avec WhatsApp sur mon téléphone » = WhatsApp du téléphone ;
        // sans rien préciser (règle suivante) = appel normal avec le téléphone
        {"motif": format!(r"^(?:appelle|appel|appeler|telephone a|passe un appel a) (?P<a>.+?)(?: (?:avec|sur|par|via|depuis|en|dans))? what'?s ?app (?:sur|avec|depuis|de|du) {tel}$"), "outil": "appeler_telephone", "args": {"a": "$a", "appli": "whatsapp"}, "priorite": 3},
        {"motif": r"^(?:appelle|appel|appeler|telephone a|passe un appel a) (?P<a>.+?)(?: (?:avec|sur|par|via|depuis|en|dans))? what'?s ?app(?: (?:sur|du|de) (?:le |mon |l')?(?:pc|ordi|ordinateur))?$", "outil": "appeler_whatsapp_pc", "args": {"a": "$a"}, "priorite": 4},
        {"motif": format!(r"^(?:appelle|appel|appeler|telephone a) (?P<a>.+?)(?: (?:avec|sur|depuis) {tel})?$"), "outil": "appeler_telephone", "args": {"a": "$a"}, "priorite": 6},
        {"motif": format!(r"^(?:reponds|repond|envoie un (?:sms|message)) a (?P<a>.+?) (?:sur {tel}|sur whatsapp|par sms) (?:que |qui dit |pour dire que )?(?P<texte>.+)$"), "outil": "repondre_telephone", "args": {"a": "$a", "texte": "$texte"}, "priorite": 5},
        {"motif": r"^(?:reessaie|reessaye|reesaie|re essaie|recommence|refais|retente|relance)(?: l'?\s?appel| d'?\s?appeler| de l'?\s?appeler| encore)?$", "outil": "reessayer_appel", "args": {}, "priorite": 5},
        {"motif": r"^(?:dis moi|quel|quels|quelle|quelles|liste|montre|donne|c'est quoi)\b.*\b(?:applications?|applis?|fenetres?|programmes?|logiciels?)\b.*\b(?:ouverte?s?|lancee?s?|en cours|arriere plan)\b", "outil": "lister_fenetres", "args": {}, "priorite": 5},
        {"motif": r"^(?:mets?|met|remets?|passe|ramene|affiche)(?: moi)? (?P<nom>.+?) (?:en|au) (?:premier plan|avant plan|avant)$", "outil": "premier_plan", "args": {"nom": "$nom"}, "priorite": 5},
        {"motif": r"^(?:fais|fait|faire)(?: moi)?(?: du| le| un)? (?:tri|menage|rangement) (?:sur|dans|de|du) (?P<d>.+)$", "outil": "ranger_dossier", "args": {"chemin_dossier": "$d"}, "priorite": 5},
        {"motif": r"^(?:range|ranger|trie|trier)(?: moi)? (?P<d>(?:mon |le |mes |ma )?(?:bureau|telechargements?|documents?))$", "outil": "ranger_dossier", "args": {"chemin_dossier": "$d"}, "priorite": 5},
    ])
}

fn vers_tel(cmd: Value, fait: String) -> Sortie {
    match crate::telephone::envoyer(cmd) {
        Ok(true) => Sortie::Fait(fait),
        Ok(false) => Sortie::Fait(format!("{fait} Ton téléphone ne répond pas en ce moment : ce sera fait dès qu'il se reconnecte.")),
        Err(e) => Sortie::Refus(e),
    }
}

/// Outils du téléphone (ne dépendent pas de « Pouvoirs sur le PC », seulement de l'appairage).
fn outil_telephone(nom_outil: &str, s: &dyn Fn(&str) -> String) -> Option<Sortie> {
    use crate::telephone as t;
    Some(match nom_outil {
        "alarme_telephone" => match t::heure_dans(&s("heure")) {
            Some((h, m)) => vers_tel(json!({"type": "alarme", "heure": h, "minute": m, "nom": s("nom")}), format!("J'envoie une alarme pour {h} h {m:02} à ton téléphone.")),
            None => Sortie::Refus("À quelle heure ? Dis-la en chiffres, par exemple « 7 h 30 ».".into()),
        },
        "minuteur_telephone" => match t::duree(&s("duree")) {
            Some(sec) => vers_tel(json!({"type": "minuteur", "secondes": sec, "nom": s("nom")}), format!("Je lance un minuteur de {} sur ton téléphone.", crate::island::fmt_dur(sec as f64))),
            None => Sortie::Refus("Combien de temps ? Par exemple « 10 minutes ».".into()),
        },
        "sonner_telephone" => {
            if t::nb_appareils() == 0 {
                let _ = ouvrir(Path::new("https://www.google.com/android/find"));
                return Some(Sortie::Fait("Aucun téléphone n'est relié à Jarvis : j'ouvre « Localiser mon appareil » de Google, clique sur « Faire sonner ».".into()));
            }
            vers_tel(json!({"type": "sonner"}), "Je fais sonner ton téléphone à fond, même en silencieux. Dis « arrête de sonner » quand tu l'as trouvé.".into())
        }
        "arreter_sonnerie_telephone" => vers_tel(json!({"type": "arreter"}), "J'arrête la sonnerie.".into()),
        "localiser_telephone" => {
            let _ = t::envoyer(json!({"type": "statut"}));
            Sortie::Fait(t::resume_statut("ou"))
        }
        "batterie_telephone" => Sortie::Fait(t::resume_statut("batterie")),
        "messages_telephone" => Sortie::Fait(t::resume_messages(&s("filtre"))),
        "mode_telephone" => {
            let m = sans_accents(&s("mode")).to_lowercase();
            let m = if m.contains("silen") { "silencieux" } else if m.contains("vib") { "vibreur" } else { "sonnerie" };
            vers_tel(json!({"type": "mode", "mode": m}), format!("Je mets ton téléphone en mode {m}."))
        }
        "lampe_telephone" => {
            let on = !sans_accents(&s("etat")).to_lowercase().contains("etein");
            vers_tel(json!({"type": "lampe", "allumer": on}), if on { "J'allume la lampe du téléphone.".into() } else { "J'éteins la lampe du téléphone.".into() })
        }
        "photo_telephone" => vers_tel(json!({"type": "photo"}), "J'ouvre l'appareil photo de ton téléphone.".into()),
        "message_vers_telephone" => vers_tel(json!({"type": "message", "texte": s("texte")}), "C'est envoyé sur ton téléphone.".into()),
        "repondre_telephone" | "appeler_telephone" => {
            let (a, texte) = (s("a"), s("texte"));
            if a.is_empty() {
                return Some(Sortie::Refus("À qui ?".into()));
            }
            if t::nb_appareils() == 0 {
                return Some(Sortie::Refus("Aucun téléphone n'est relié. Menu de l'icône → « Relier un téléphone… ».".into()));
            }
            let appel = nom_outil == "appeler_telephone";
            let whatsapp = appel && sans_accents(&s("appli")).to_lowercase().contains("whats");
            let q = if whatsapp {
                format!("Tu es sûr de vouloir appeler {a} avec WhatsApp ?")
            } else if appel {
                format!("Tu es sûr de vouloir appeler {a} avec ton téléphone ?")
            } else {
                format!("J'envoie à {a} : « {texte} » ?")
            };
            let cmd = if whatsapp {
                json!({"type": "appeler", "a": a, "appli": "whatsapp"})
            } else if appel {
                json!({"type": "appeler", "a": a})
            } else {
                json!({"type": "repondre", "a": a, "texte": texte})
            };
            let fait = if whatsapp {
                format!("J'appelle {a} avec WhatsApp.")
            } else if appel {
                format!("J'appelle {a}.")
            } else {
                format!("C'est parti pour {a}.")
            };
            question(q, Box::new(move || {
                journaliser(if appel { "appeler (téléphone)" } else { "message (téléphone)" }, &cmd.to_string(), true);
                vers_tel(cmd, fait)
            }))
        }
        _ => return None,
    })
}

// ───────────────────────── appel WhatsApp sur le PC ─────────────────────────

/// Le script qui pilote l'appli WhatsApp du PC (recherche du contact, puis bouton « Appel vocal »).
const WHATSAPP_PS: &str = include_str!("scripts/whatsapp_pc.ps1");

/// « le 06 12 34 56 78 » → « +33612345678 » ; « 33 7 11 22 33 44 » → « +33711223344 » ; un nom reste un nom.
pub fn cible_whatsapp(a: &str) -> String {
    let a = a.trim();
    let chiffres: String = a.chars().filter(|c| c.is_ascii_digit()).collect();
    let lettres = a.chars().filter(|c| c.is_alphabetic()).count();
    if chiffres.len() < 9 || lettres > 3 {
        return a.to_string();
    }
    if chiffres.len() == 10 && chiffres.starts_with('0') {
        format!("+33{}", &chiffres[1..])
    } else if chiffres.len() >= 11 {
        format!("+{}", chiffres.trim_start_matches("00"))
    } else {
        chiffres
    }
}

/// Lance le script : renvoie sa ligne de résultat (« CONTACT:… », « APPEL:… » ou « ERREUR:… »).
fn whatsapp_pc(mode: &str, cible: &str, attendu: &str) -> String {
    use std::os::windows::process::CommandExt;
    let chemin = std::env::temp_dir().join("jarvis-whatsapp.ps1");
    // avec la marque UTF-8 au début : sinon PowerShell lit mal les accents
    if std::fs::write(&chemin, format!("\u{FEFF}{WHATSAPP_PS}")).is_err() {
        return "ERREUR:fenetre".into();
    }
    let sortie = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&chemin)
        .args(["-Mode", mode])
        .env("JARVIS_WA_CIBLE", cible)
        .env("JARVIS_WA_ATTENDU", attendu)
        .creation_flags(0x0800_0000) // sans fenêtre noire
        .output();
    let Ok(sortie) = sortie else { return "ERREUR:fenetre".into() };
    String::from_utf8_lossy(&sortie.stdout)
        .lines()
        .map(|l| l.trim().trim_start_matches('\u{FEFF}'))
        .find(|l| l.starts_with("CONTACT:") || l.starts_with("APPEL:") || l.starts_with("ERREUR:"))
        .unwrap_or("ERREUR:fenetre")
        .to_string()
}

/// Ce que Jarvis dit quand le script échoue.
pub fn erreur_whatsapp(ligne: &str, a: &str) -> String {
    let mut p = ligne.trim_start_matches("ERREUR:").splitn(2, ':');
    let (raison, nom) = (p.next().unwrap_or(""), p.next().unwrap_or(a));
    match raison {
        "absent" => "WhatsApp n'est pas installé sur ce PC, ou il ne s'ouvre pas.".into(),
        "introuvable" => format!("Je ne trouve pas {a} dans WhatsApp sur le PC."),
        "pas_d_appel" => format!("Je ne peux pas appeler {nom} dans WhatsApp : il n'y a pas de bouton d'appel."),
        "autre_contact" => format!("WhatsApp a ouvert la discussion de {nom} au lieu de la bonne : je n'appelle pas."),
        _ => "Je n'arrive pas à utiliser la fenêtre de WhatsApp sur le PC.".into(),
    }
}

/// « Appelle maman avec WhatsApp » : on ouvre d'abord la discussion pour connaître le VRAI nom du contact,
/// on le dit dans la question, et l'appel ne part qu'après le « oui ».
fn appeler_whatsapp_pc(a: &str) -> Sortie {
    let a = a.trim().trim_start_matches("le ").trim_start_matches("au ").trim().to_string();
    if a.is_empty() {
        return Sortie::Refus("Qui veux-tu appeler avec WhatsApp ?".into());
    }
    let cible = cible_whatsapp(&a);
    let r = whatsapp_pc("ouvrir", &cible, "");
    let Some(nom) = r.strip_prefix("CONTACT:").map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) else {
        journaliser("appeler (WhatsApp PC)", &format!("{cible} : {r}"), false);
        return Sortie::Refus(erreur_whatsapp(&r, &a));
    };
    question(
        format!("Tu es sûr de vouloir appeler {nom} avec WhatsApp ?"),
        Box::new(move || {
            let r = whatsapp_pc("appeler", &cible, &nom);
            let ok = r.starts_with("APPEL:");
            journaliser("appeler (WhatsApp PC)", &format!("{nom} : {r}"), ok);
            if ok { Sortie::Fait(format!("J'appelle {nom} avec WhatsApp.")) } else { Sortie::Refus(erreur_whatsapp(&r, &nom)) }
        }),
    )
}

// ───────────────────────── fenêtres et rangement ─────────────────────────

const NAVIGATEURS: [&str; 5] = ["chrome", "msedge", "firefox", "brave", "opera"];

/// « chrome » → « Chrome », « explorer » → « l'Explorateur de fichiers »… (applis du Store : leur titre).
fn nom_appli(exe: &str, titre: &str) -> String {
    let n = match exe {
        "chrome" => "Chrome",
        "msedge" => "Edge",
        "firefox" => "Firefox",
        "brave" => "Brave",
        "opera" => "Opera",
        "explorer" => "Explorateur de fichiers",
        "discord" => "Discord",
        "spotify" => "Spotify",
        "deezer" => "Deezer",
        "code" => "VS Code",
        "claude" => "Claude",
        "whatsapp" | "whatsapp.root" => "WhatsApp",
        "notepad" => "Bloc-notes",
        "steam" | "steamwebhelper" => "Steam",
        "phoneexperiencehost" => "Lien avec Windows",
        "ollama app" | "ollama" => "Ollama",
        "systemsettings" => "Paramètres",
        "applicationframehost" | "" => return titre_court(titre),
        autre => {
            let mut c = autre.chars();
            return c.next().map(|p| p.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
        }
    };
    n.to_string()
}

/// « YouTube - Google Chrome » → « YouTube » (titres raccourcis pour être dits à voix haute).
fn titre_court(titre: &str) -> String {
    let t = titre.trim();
    let t = [" - Google Chrome", " — Mozilla Firefox", " - Mozilla Firefox", " - Microsoft\u{200b} Edge", " - Microsoft Edge", " - Brave", " - Opera"]
        .iter()
        .fold(t.to_string(), |t, fin| t.strip_suffix(fin).map(str::to_string).unwrap_or(t));
    let t = t.trim().to_string();
    if t.chars().count() > 40 { t.chars().take(40).collect::<String>() + "…" } else { t }
}

/// Clé de comparaison : minuscules, sans accents, sans « mon », « la fenêtre »…
fn cle_nom(s: &str) -> String {
    let s = sans_accents(s).to_lowercase().replace('\'', " ");
    s.split_whitespace()
        .filter(|m| !["mon", "ma", "mes", "le", "la", "les", "l", "fenetre", "appli", "application", "de", "du"].contains(m))
        .collect::<Vec<_>>()
        .join(" ")
}

/// ["a", "b", "c"] → « a, b et c ».
fn joindre_et(v: &[String]) -> String {
    match v {
        [] => String::new(),
        [seul] => seul.clone(),
        [debut @ .., dernier] => format!("{} et {dernier}", debut.join(", ")),
    }
}

/// Dossier de rangement selon l'extension (None = on ne touche pas : raccourcis, programmes, inconnus).
fn categorie_fichier(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "heic" | "svg" | "ico" | "tif" | "tiff" | "avif" => "Images",
        "pdf" | "doc" | "docx" | "odt" | "txt" | "rtf" | "md" | "xls" | "xlsx" | "ods" | "csv" | "ppt" | "pptx" | "odp" => "Documents",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "wmv" | "m4v" => "Vidéos",
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "wma" => "Musique",
        "zip" | "rar" | "7z" | "tar" | "gz" => "Archives",
        _ => return None,
    })
}

/// Une liste d'une ligne dans config.toml : `cle = ["a", "b"]` dans la section `[section]`.
pub fn lire_liste_toml(txt: &str, section: &str, cle: &str) -> Vec<String> {
    let mut dans = false;
    for l in txt.lines().map(str::trim) {
        if l.starts_with('[') {
            dans = l == format!("[{section}]");
            continue;
        }
        let Some(v) = l.strip_prefix(cle).map(str::trim_start).and_then(|r| r.strip_prefix('=')).filter(|_| dans) else { continue };
        let v = v.split('#').next().unwrap_or("");
        return v.split('"').skip(1).step_by(2).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }
    Vec::new()
}

/// Les personnes à appeler avec WhatsApp quand on dit juste « appelle … » ([whatsapp] contacts de config.toml).
/// Relu à chaque appel : une personne ajoutée marche tout de suite, sans redémarrer.
fn contacts_whatsapp() -> Vec<String> {
    crate::jarvis_link::find_exe("")
        .and_then(|p| std::fs::read_to_string(p.with_file_name("config.toml")).ok())
        .map(|t| lire_liste_toml(&t, "whatsapp", "contacts"))
        .unwrap_or_default()
}

/// « Anaïs », « Anaïse », « Anays », « Anaisse » → la même clé (la transcription écrit les prénoms de mille façons).
fn cle_prenom(s: &str) -> String {
    let s: String = sans_accents(s).to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
    let s = s.replace('y', "i").replace("ae", "ai").replace("ei", "ai").replace("ss", "s");
    s.trim_end_matches('e').trim_end_matches('s').to_string()
}

/// Le contact WhatsApp (écrit comme dans la liste) dont on parle dans « a », s'il y en a un.
pub fn contact_whatsapp(a: &str, contacts: &[String]) -> Option<String> {
    let tout = cle_prenom(a);
    let mots: Vec<String> = a.split_whitespace().map(cle_prenom).filter(|m| m.len() >= 2).collect();
    contacts
        .iter()
        .find(|c| {
            let k = cle_prenom(c);
            k.len() >= 2 && (k == tout || mots.contains(&k))
        })
        .cloned()
}

/// Appelle un outil. Ne bloque pas longtemps, sauf PowerShell (≤ délai) et l'IA (modifier, page web).
pub fn appeler(nom_outil: &str, args: &Value) -> Sortie {
    let texte_de = |k: &str| match &args[k] {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    };
    if nom_outil == "reessayer_appel" {
        let dernier = DERNIER_APPEL.lock().ok().and_then(|d| d.clone());
        return match dernier {
            Some((outil, a)) => appeler(&outil, &a),
            None => Sortie::Refus("Je n'ai pas encore passé d'appel à refaire.".into()),
        };
    }
    if nom_outil == "appeler_telephone" || nom_outil == "appeler_whatsapp_pc" {
        if let Ok(mut d) = DERNIER_APPEL.lock() {
            *d = Some((nom_outil.to_string(), args.clone()));
        }
    }
    if nom_outil == "appeler_whatsapp_pc" {
        return appeler_whatsapp_pc(&texte_de("a"));
    }
    // « appelle Anaïs » tout court : si elle est dans la liste WhatsApp de config.toml, on passe par WhatsApp
    if nom_outil == "appeler_telephone" && texte_de("appli").trim().is_empty() && texte_de("texte").trim().is_empty() {
        if let Some(nom) = contact_whatsapp(&texte_de("a"), &contacts_whatsapp()) {
            return appeler_whatsapp_pc(&nom);
        }
    }
    if let Some(r) = outil_telephone(nom_outil, &texte_de) {
        return r;
    }
    let c = cfg();
    if !c.actif {
        return Sortie::Refus("Mes pouvoirs sur le PC sont désactivés (menu de l'icône, « Pouvoirs sur le PC »).".into());
    }
    let base = dossier_travail(&c);
    let s = |k: &str| match &args[k] {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    };
    let ch = |k: &str| resoudre(&s(k), &base);
    let extra = c.chemins_interdits.clone();
    let refus = |p: &Path, action: &str| -> Option<Sortie> {
        interdit(p, &extra).map(|r| {
            journaliser(action, &format!("REFUSÉ {} : {r}", p.display()), false);
            Sortie::Refus(format!("Je ne touche pas à ça : {r}."))
        })
    };
    match nom_outil {
        "lister_dossier" => {
            let p = ch("chemin_dossier");
            let Ok(rd) = std::fs::read_dir(&p) else { return Sortie::Refus(format!("Je ne trouve pas le dossier {}.", nom(&p))) };
            let (mut dossiers, mut fichiers) = (Vec::new(), Vec::new());
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { dossiers.push(n) } else { fichiers.push(n) }
            }
            dossiers.sort_by_key(|x| x.to_lowercase());
            fichiers.sort_by_key(|x| x.to_lowercase());
            journaliser("lister", &p.display().to_string(), true);
            if dossiers.is_empty() && fichiers.is_empty() {
                return Sortie::Fait(format!("Le dossier {} est vide.", nom(&p)));
            }
            let mut m = Vec::new();
            if !dossiers.is_empty() {
                m.push(format!("{} dossier{} : {}", dossiers.len(), if dossiers.len() > 1 { "s" } else { "" }, dossiers.iter().take(8).cloned().collect::<Vec<_>>().join(", ")));
            }
            if !fichiers.is_empty() {
                m.push(format!("{} fichier{} : {}", fichiers.len(), if fichiers.len() > 1 { "s" } else { "" }, fichiers.iter().take(12).cloned().collect::<Vec<_>>().join(", ")));
            }
            Sortie::Fait(format!("Dans {}, il y a {}.", nom(&p), m.join(" ; et ")))
        }
        "lire_fichier" => {
            let p = ch("chemin_fichier");
            let Ok(octets) = std::fs::read(&p) else { return Sortie::Refus(format!("Je ne trouve pas le fichier {}.", nom(&p))) };
            let texte: String = String::from_utf8_lossy(&octets).chars().take(LECTURE_MAX).collect();
            journaliser("lire", &p.display().to_string(), true);
            if texte.chars().count() <= 600 {
                let t = texte.trim();
                Sortie::Fait(format!("Voici {} : {}", nom(&p), if t.is_empty() { "(vide)" } else { t }))
            } else {
                Sortie::Fait(format!("{} fait {} lignes. Il commence par : {}…", nom(&p), texte.lines().count(), texte.chars().take(400).collect::<String>().trim()))
            }
        }
        "creer_fichier" => {
            let p = ch("chemin_fichier");
            if let Some(r) = refus(&p, "créer") {
                return r;
            }
            let contenu = s("contenu");
            let existe = p.exists();
            let ecrire: Action = Box::new(move || {
                let copie = sauvegarder(&p);
                if let Some(d) = p.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                match std::fs::write(&p, contenu) {
                    Ok(_) => {
                        journaliser("écrire", &format!("{}{}", p.display(), copie.as_ref().map(|c| format!(" (sauvegarde : {})", c.display())).unwrap_or_default()), true);
                        Sortie::Fait(format!("C'est enregistré : {}{}", nom(&p), if copie.is_some() { ", l'ancien est sauvegardé." } else { "." }))
                    }
                    Err(e) => Sortie::Refus(format!("Je n'ai pas pu écrire le fichier : {e}")),
                }
            });
            if existe {
                let n = nom(&ch("chemin_fichier"));
                question(format!("Le fichier {n} existe déjà. Je l'écrase ? (je garde une copie)"), ecrire)
            } else {
                ecrire()
            }
        }
        "ajouter_au_fichier" => {
            use std::io::Write;
            let p = ch("chemin_fichier");
            if let Some(r) = refus(&p, "ajouter") {
                return r;
            }
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let deja = p.metadata().map(|m| m.len() > 0).unwrap_or(false);
            match std::fs::OpenOptions::new().create(true).append(true).open(&p).and_then(|mut f| write!(f, "{}{}", if deja { "\n" } else { "" }, s("texte"))) {
                Ok(_) => {
                    journaliser("ajouter", &p.display().to_string(), true);
                    Sortie::Fait(format!("C'est ajouté à {}.", nom(&p)))
                }
                Err(e) => Sortie::Refus(format!("Je n'ai pas pu écrire : {e}")),
            }
        }
        "remplacer_dans_fichier" => {
            let p = ch("chemin_fichier");
            if let Some(r) = refus(&p, "remplacer") {
                return r;
            }
            let Ok(texte) = std::fs::read_to_string(&p) else { return Sortie::Refus(format!("Je ne trouve pas le fichier texte {}.", nom(&p))) };
            let (ancien, nouveau) = (s("ancien"), s("nouveau"));
            let n = if ancien.is_empty() { 0 } else { texte.matches(&ancien).count() };
            if n == 0 {
                return Sortie::Refus(format!("Je ne trouve pas « {} » dans {}.", ancien.chars().take(60).collect::<String>(), nom(&p)));
            }
            let q = format!("Je remplace {n} fois « {} » par « {} » dans {} ?", ancien.chars().take(60).collect::<String>(), nouveau.chars().take(60).collect::<String>(), nom(&p));
            question(q, Box::new(move || {
                sauvegarder(&p);
                match std::fs::write(&p, texte.replace(&ancien, &nouveau)) {
                    Ok(_) => {
                        journaliser("remplacer", &format!("{} : {n} fois", p.display()), true);
                        Sortie::Fait(format!("C'est remplacé {n} fois dans {}.", nom(&p)))
                    }
                    Err(e) => Sortie::Refus(format!("Je n'ai pas pu écrire : {e}")),
                }
            }))
        }
        "modifier_fichier_ia" => modifier_ia(ch("chemin_fichier"), &s("consigne"), &extra, false),
        "creer_dossier" => {
            let p = ch("chemin_dossier");
            if let Some(r) = refus(&p, "créer dossier") {
                return r;
            }
            match std::fs::create_dir_all(&p) {
                Ok(_) => {
                    journaliser("créer dossier", &p.display().to_string(), true);
                    Sortie::Fait(format!("Le dossier {} est prêt.", nom(&p)))
                }
                Err(e) => Sortie::Refus(format!("Je n'ai pas pu créer le dossier : {e}")),
            }
        }
        "deplacer" | "copier_fichier" => {
            let copier = nom_outil == "copier_fichier";
            let action = if copier { "copier" } else { "déplacer" };
            let (src, mut dst) = (ch("source"), ch("destination"));
            if !src.exists() {
                return Sortie::Refus(format!("Je ne trouve pas {}.", nom(&src)));
            }
            if let Some(r) = refus(&dst, action).or_else(|| if copier { None } else { refus(&src, action) }) {
                return r;
            }
            if dst.is_dir() {
                dst = dst.join(src.file_name().unwrap_or_default());
            }
            if dst.exists() {
                return Sortie::Refus(format!("{} existe déjà là-bas : je ne l'écrase pas. Choisis un autre nom.", nom(&dst)));
            }
            if let Some(d) = dst.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let r = if copier {
                if src.is_dir() { copier_dossier(&src, &dst) } else { std::fs::copy(&src, &dst).map(|_| ()) }
            } else {
                std::fs::rename(&src, &dst).or_else(|_| {
                    // autre disque : copie puis corbeille de l'original
                    if src.is_dir() { copier_dossier(&src, &dst)? } else { std::fs::copy(&src, &dst).map(|_| ())? }
                    if corbeille(&src) { Ok(()) } else { Err(std::io::Error::other("original pas supprimé")) }
                })
            };
            match r {
                Ok(_) => {
                    journaliser(action, &format!("{} -> {}", src.display(), dst.display()), true);
                    Sortie::Fait(format!("C'est {} : {}.", if copier { "copié" } else { "déplacé" }, dst.display()))
                }
                Err(e) => Sortie::Refus(format!("Ça n'a pas marché : {e}")),
            }
        }
        "supprimer" => {
            let p = ch("chemin_element");
            if let Some(r) = refus(&p, "supprimer") {
                return r;
            }
            if !p.exists() {
                return Sortie::Refus(format!("Je ne trouve pas {}.", nom(&p)));
            }
            let quoi = if p.is_dir() { "le dossier" } else { "le fichier" };
            let vers_corbeille = c.corbeille;
            let q = format!("Je mets {quoi} {} {} ?", nom(&p), if vers_corbeille { "à la corbeille" } else { "à la poubelle DÉFINITIVEMENT" });
            question(q, Box::new(move || {
                let ok = if vers_corbeille {
                    corbeille(&p)
                } else {
                    (if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) }).is_ok()
                };
                journaliser(if vers_corbeille { "supprimer (corbeille)" } else { "supprimer (définitif)" }, &p.display().to_string(), ok);
                if ok {
                    Sortie::Fait(format!("C'est fait, {} est {}.", nom(&p), if vers_corbeille { "dans la corbeille" } else { "supprimé" }))
                } else {
                    Sortie::Refus("Je n'ai pas réussi à le supprimer.".into())
                }
            }))
        }
        "ouvrir_fichier" => {
            let p = ch("chemin_element");
            if !p.exists() {
                return Sortie::Refus(format!("Je ne trouve pas {}.", nom(&p)));
            }
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let faire: Action = Box::new(move || {
                let ok = ouvrir(&p);
                journaliser("ouvrir", &p.display().to_string(), ok);
                if ok { Sortie::Fait(format!("J'ouvre {}.", nom(&p))) } else { Sortie::Refus("Windows n'a pas pu l'ouvrir.".into()) }
            });
            if PROGRAMMES.contains(&ext.as_str()) {
                question(format!("{} est un programme. Je le lance ?", nom(&ch("chemin_element"))), faire)
            } else {
                faire()
            }
        }
        "creer_page_web" => {
            let sujet = s("sujet");
            let n = s("nom");
            let p = base.join("pages web").join(format!("{}.html", nom_fichier(if n.trim().is_empty() { &sujet } else { &n })));
            if let Some(r) = refus(&p, "page web") {
                return r;
            }
            let html = ia(
                "Tu crées une page web en UN SEUL fichier HTML (CSS dans <style>, JavaScript dans <script>), jolie, moderne, \
en français, sans ressource externe. Réponds uniquement avec le code dans un bloc ```html```.",
                &format!("Crée une page web : {sujet}"),
            )
            .map(|r| extraire_code(&r))
            .ok()
            .filter(|h| h.contains('<'))
            .unwrap_or_else(|| {
                format!("<!doctype html><html lang=\"fr\"><meta charset=\"utf-8\"><title>{sujet}</title><body style=\"font-family:Segoe UI;margin:40px\"><h1>{sujet}</h1><p>Page créée par Jarvis.</p></body></html>\n")
            });
            let existe = p.exists();
            let ecrire: Action = Box::new(move || {
                sauvegarder(&p);
                if let Some(d) = p.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                if let Err(e) = std::fs::write(&p, html) {
                    return Sortie::Refus(format!("Je n'ai pas pu enregistrer la page : {e}"));
                }
                journaliser("page web", &p.display().to_string(), true);
                ouvrir(&p);
                let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                Sortie::Fait(format!("Ta page est prête, je l'ouvre dans le navigateur ({}). Dis « modifie la page {stem} » pour la changer.", p.display()))
            });
            if existe { question("Cette page existe déjà. Je la remplace ? (je garde une copie)".into(), ecrire) } else { ecrire() }
        }
        "modifier_page_web" => {
            let n = s("nom");
            let mut p = base.join("pages web").join(format!("{}.html", nom_fichier(&n)));
            if !p.is_file() {
                p = resoudre(&n, &base);
            }
            modifier_ia(p, &s("consigne"), &extra, true)
        }
        "executer_powershell" => {
            let commande = s("commande");
            if !c.powershell {
                return Sortie::Refus("PowerShell est désactivé dans mes réglages.".into());
            }
            if commande.trim().is_empty() {
                return Sortie::Refus("Quelle commande ?".into());
            }
            if ps_interdit(&commande) {
                journaliser("powershell", &format!("REFUSÉ : {commande}"), false);
                return Sortie::Refus("Je refuse cette commande : elle peut abîmer Windows, couper l'antivirus ou télécharger des choses.".into());
            }
            let delai = Duration::from_secs_f32(c.delai_powershell.clamp(1.0, 600.0));
            let lecture = ps_lecture_seule(&commande);
            let cmd = commande.clone();
            let faire: Action = Box::new(move || {
                let (code, sortie) = executer_ps(&cmd, delai);
                journaliser("powershell", &format!("{cmd} -> code {code:?}"), code == Some(0));
                let debut: String = sortie.chars().take(500).collect::<String>() + if sortie.chars().count() > 500 { "…" } else { "" };
                match code {
                    None => Sortie::Refus(format!("La commande a pris plus de {} secondes : je l'ai arrêtée.", delai.as_secs())),
                    Some(0) if debut.is_empty() => Sortie::Fait("C'est fait, sans message.".into()),
                    Some(0) => Sortie::Fait(format!("Fait. Résultat : {debut}")),
                    Some(_) => Sortie::Refus(format!("La commande a échoué : {}", if debut.is_empty() { "pas de message".into() } else { debut })),
                }
            });
            if lecture {
                faire()
            } else {
                question(format!("Je lance cette commande PowerShell : « {} » ? Elle peut modifier le PC.", commande.chars().take(200).collect::<String>()), faire)
            }
        }
        "fermer_processus" => {
            use std::os::windows::process::CommandExt;
            let mut n = s("nom").trim().to_lowercase();
            if n.is_empty() {
                return Sortie::Refus("Quel processus ?".into());
            }
            if !n.ends_with(".exe") {
                n.push_str(".exe");
            }
            if PROCESSUS_PROTEGES.contains(&n.as_str()) || PROCESSUS_PROTEGES.contains(&n.trim_end_matches(".exe")) {
                return Sortie::Refus(format!("Je ne ferme pas {n} : c'est indispensable à Windows ou à moi."));
            }
            let liste = Command::new("tasklist").args(["/FO", "CSV", "/NH", "/FI", &format!("IMAGENAME eq {n}")]).creation_flags(SANS_FENETRE).output();
            let combien = liste.map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.to_lowercase().starts_with(&format!("\"{n}\""))).count()).unwrap_or(0);
            if combien == 0 {
                return Sortie::Refus(format!("Aucun processus {n} ne tourne."));
            }
            let q = format!("Je force la fermeture de {n} ({combien} processus) ? Ce qui n'est pas enregistré sera perdu.");
            question(q, Box::new(move || {
                let ok = Command::new("taskkill").args(["/F", "/T", "/IM", &n]).creation_flags(SANS_FENETRE).status().map(|s| s.success()).unwrap_or(false);
                journaliser("tuer processus", &n, ok);
                if ok { Sortie::Fait(format!("J'ai fermé {n}.")) } else { Sortie::Refus(format!("Je n'ai pas pu fermer {n}.")) }
            }))
        }
        "lancer_programme" => lancer_programme(&s("nom")),
        "alimentation_pc" => alimentation(&sans_accents(&s("action")).to_lowercase()),
        "lister_fenetres" => {
            let fen = crate::sys::fenetres_ouvertes();
            let mut applis: Vec<(String, Vec<String>)> = Vec::new();
            for f in &fen {
                let n = nom_appli(&f.exe, &f.titre);
                let titre = titre_court(&f.titre);
                match applis.iter_mut().find(|(a, _)| *a == n) {
                    Some((_, t)) => t.push(titre),
                    None => applis.push((n, vec![titre])),
                }
            }
            journaliser("lister fenêtres", &applis.len().to_string(), true);
            if applis.is_empty() {
                return Sortie::Fait("Aucune appli n'est ouverte, à part moi.".into());
            }
            let morceaux: Vec<String> = applis
                .iter()
                .map(|(n, t)| if NAVIGATEURS.iter().any(|b| nom_appli(b, "") == *n) { format!("{n} ({})", t.join(", ")) } else { n.clone() })
                .collect();
            let s = if applis.len() > 1 { "s" } else { "" };
            Sortie::Fait(format!("Tu as {} appli{s} ouverte{s} : {}.", applis.len(), joindre_et(&morceaux)))
        }
        "premier_plan" => {
            let demande = s("nom");
            let q = cle_nom(&demande);
            let fen = crate::sys::fenetres_ouvertes();
            let veut_nav = ["navigateur", "internet", "google", "web"].iter().any(|m| q.contains(m));
            let choix = fen
                .iter()
                .find(|f| veut_nav && NAVIGATEURS.contains(&f.exe.as_str()))
                .or_else(|| {
                    fen.iter().find(|f| {
                        let n = cle_nom(&nom_appli(&f.exe, &f.titre));
                        !q.is_empty() && (n.contains(&q) || (n.len() >= 3 && q.contains(&n)) || (q.len() >= 3 && f.exe.contains(&q)))
                    })
                })
                .or_else(|| fen.iter().find(|f| q.len() >= 3 && cle_nom(&f.titre).contains(&q)));
            let Some(f) = choix else {
                return Sortie::Refus(format!("Je ne trouve pas de fenêtre « {} » ouverte.", demande.trim()));
            };
            let n = nom_appli(&f.exe, &f.titre);
            let ok = crate::sys::mettre_devant(f.h);
            journaliser("premier plan", &n, ok);
            if ok { Sortie::Fait(format!("Je mets {n} au premier plan.")) } else { Sortie::Refus(format!("Windows ne me laisse pas mettre {n} devant. Clique dessus dans la barre des tâches.")) }
        }
        "ranger_dossier" => {
            let p = if s("chemin_dossier").trim().is_empty() { alias("bureau").unwrap_or_else(|| base.clone()) } else { ch("chemin_dossier") };
            if let Some(r) = refus(&p, "ranger") {
                return r;
            }
            let Ok(rd) = std::fs::read_dir(&p) else { return Sortie::Refus(format!("Je ne trouve pas le dossier {}.", nom(&p))) };
            let mut plan: Vec<(PathBuf, PathBuf)> = Vec::new();
            let mut compte: Vec<(&'static str, usize)> = Vec::new();
            for e in rd.flatten() {
                if !e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let fichier = e.file_name().to_string_lossy().to_string();
                let ext = e.path().extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
                let Some(cat) = categorie_fichier(&ext) else { continue };
                let dst = p.join(cat).join(&fichier);
                if fichier.starts_with('.') || dst.exists() {
                    continue;
                }
                match compte.iter_mut().find(|(c, _)| *c == cat) {
                    Some((_, n)) => *n += 1,
                    None => compte.push((cat, 1)),
                }
                plan.push((e.path(), dst));
            }
            let dossier = nom(&p);
            if plan.is_empty() {
                return Sortie::Fait(format!("{dossier} est déjà rangé : rien à déplacer (je ne touche pas aux raccourcis, aux programmes ni aux dossiers)."));
            }
            let total = plan.len();
            let detail = joindre_et(&compte.iter().map(|(c, n)| format!("{n} dans {c}")).collect::<Vec<_>>());
            question(
                format!("Je range {total} fichier{} de {dossier} : {detail}. Les raccourcis, programmes et dossiers ne bougent pas. On y va ?", if total > 1 { "s" } else { "" }),
                Box::new(move || {
                    let mut ok = 0;
                    for (src, dst) in &plan {
                        if let Some(d) = dst.parent() {
                            let _ = std::fs::create_dir_all(d);
                        }
                        if !dst.exists() && std::fs::rename(src, dst).is_ok() {
                            ok += 1;
                        }
                    }
                    journaliser("ranger", &format!("{dossier} : {ok}/{total} fichiers"), ok > 0);
                    let s = if ok > 1 { "s" } else { "" };
                    Sortie::Fait(format!("C'est rangé : {ok} fichier{s} déplacé{s} dans {dossier}."))
                }),
            )
        }
        _ => Sortie::Refus("Je ne connais pas cet outil.".into()),
    }
}

fn modifier_ia(p: PathBuf, consigne: &str, extra: &[String], rouvrir: bool) -> Sortie {
    if let Some(r) = interdit(&p, extra) {
        journaliser("modifier (IA)", &format!("REFUSÉ {} : {r}", p.display()), false);
        return Sortie::Refus(format!("Je ne touche pas à ça : {r}."));
    }
    let Ok(avant) = std::fs::read_to_string(&p) else { return Sortie::Refus(format!("Je ne trouve pas le fichier texte {}.", nom(&p))) };
    if avant.len() > MODIF_IA_MAX {
        return Sortie::Refus(format!("{} est trop gros pour que je le réécrive en entier. Demande-moi de remplacer un morceau précis.", nom(&p)));
    }
    let reponse = match ia(
        "Tu modifies un fichier. Réponds UNIQUEMENT avec le fichier complet modifié, dans un seul bloc ``` ```, sans explication. \
Garde tout ce qui n'est pas concerné par la consigne.",
        &format!("Fichier {} :\n```\n{avant}\n```\n\nConsigne : {consigne}", nom(&p)),
    ) {
        Ok(r) => r,
        Err(e) => return Sortie::Refus(format!("L'IA ne répond pas : {e}")),
    };
    let apres = extraire_code(&reponse);
    if apres.trim().len() < 5 {
        return Sortie::Refus("L'IA n'a pas réussi à préparer la modification.".into());
    }
    if apres.trim() == avant.trim() {
        return Sortie::Fait(format!("L'IA n'a rien trouvé à changer dans {}.", nom(&p)));
    }
    let n = diff_lignes(&avant, &apres);
    let consigne = consigne.to_string();
    question(
        format!("J'ai préparé la modification de {} : environ {n} lignes changées. Je l'enregistre ?", nom(&p)),
        Box::new(move || {
            let copie = sauvegarder(&p);
            if let Err(e) = std::fs::write(&p, apres) {
                return Sortie::Refus(format!("Je n'ai pas pu écrire : {e}"));
            }
            journaliser("modifier (IA)", &format!("{} : {consigne} (sauvegarde : {:?})", p.display(), copie), true);
            if rouvrir {
                ouvrir(&p);
            }
            Sortie::Fait("C'est modifié. L'ancienne version est sauvegardée.".into())
        }),
    )
}

fn lancer_programme(n: &str) -> Sortie {
    let n = n.trim();
    if n.is_empty() {
        return Sortie::Refus("Quel programme ?".into());
    }
    let cle = sans_accents(n).to_lowercase();
    // 1) un raccourci du menu Démarrer dont le nom contient le mot ; 2) le nom tel quel (App Paths : chrome, notepad…)
    let mut dossiers = Vec::new();
    if let Ok(a) = std::env::var("APPDATA") {
        dossiers.push(PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    if let Ok(a) = std::env::var("ProgramData") {
        dossiers.push(PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    fn chercher(d: &Path, cle: &str, prof: u32, meilleur: &mut Option<(usize, PathBuf)>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && prof < 3 {
                chercher(&p, cle, prof + 1, meilleur);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
                let stem = sans_accents(&p.file_stem().unwrap_or_default().to_string_lossy()).to_lowercase();
                if stem.contains(cle) && !stem.contains("desinstall") && !stem.contains("uninstall") {
                    let score = stem.len();
                    if meilleur.as_ref().is_none_or(|(s, _)| score < *s) {
                        *meilleur = Some((score, p));
                    }
                }
            }
        }
    }
    let mut meilleur = None;
    for d in &dossiers {
        chercher(d, &cle, 0, &mut meilleur);
    }
    let ok = match &meilleur {
        Some((_, p)) => ouvrir(p),
        None => ouvrir(Path::new(n)),
    };
    journaliser("lancer", n, ok);
    if ok { Sortie::Fait(format!("Je lance {n}.")) } else { Sortie::Refus(format!("Je ne trouve pas l'application {n}.")) }
}

fn alimentation(action: &str) -> Sortie {
    use std::os::windows::process::CommandExt;
    let lancer = |prog: &str, args: &[&str]| Command::new(prog).args(args).creation_flags(SANS_FENETRE).spawn().is_ok();
    match action.trim() {
        "verrouiller" => {
            let ok = lancer("rundll32.exe", &["user32.dll,LockWorkStation"]);
            journaliser("verrouiller", "", ok);
            Sortie::Fait("Je verrouille le PC.".into())
        }
        "annuler" => {
            let ok = lancer("shutdown", &["/a"]);
            journaliser("annuler extinction", "", ok);
            Sortie::Fait("J'annule l'extinction prévue.".into())
        }
        a @ ("eteindre" | "redemarrer" | "veille") => {
            let a = a.to_string();
            let q = match a.as_str() {
                "eteindre" => "J'éteins le PC dans 30 secondes ? Enregistre ton travail.",
                "redemarrer" => "Je redémarre le PC dans 30 secondes ? Enregistre ton travail.",
                _ => "Je mets le PC en veille ?",
            };
            question(q.into(), Box::new(move || {
                let ok = match a.as_str() {
                    "eteindre" => lancer("shutdown", &["/s", "/t", "30"]),
                    "redemarrer" => lancer("shutdown", &["/r", "/t", "30"]),
                    _ => lancer("powershell.exe", &["-NoProfile", "-Command", "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Application]::SetSuspendState('Suspend', $false, $false)"]),
                };
                journaliser(&a, "", ok);
                if !ok {
                    return Sortie::Refus("Ça n'a pas marché.".into());
                }
                Sortie::Fait(if a == "veille" { "Bonne nuit !".into() } else { "C'est parti. Dis « annule l'extinction » pour arrêter.".into() })
            }))
        }
        _ => Sortie::Refus("Je connais : éteindre, redémarrer, veille, verrouiller, annuler.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_de_fenetres() {
        assert_eq!(nom_appli("chrome", "YouTube - Google Chrome"), "Chrome");
        assert_eq!(nom_appli("applicationframehost", "Calculatrice"), "Calculatrice");
        assert_eq!(nom_appli("obs64", "OBS"), "Obs64");
        assert_eq!(titre_court("YouTube - Google Chrome"), "YouTube");
        assert_eq!(cle_nom("mon Navigateur"), "navigateur");
        assert_eq!(cle_nom("la fenêtre de Discord"), "discord");
        assert_eq!(joindre_et(&["a".into(), "b".into(), "c".into()]), "a, b et c");
        assert_eq!(categorie_fichier("jpg"), Some("Images"));
        assert_eq!(categorie_fichier("lnk"), None);
        assert_eq!(categorie_fichier("exe"), None);
    }

    #[test]
    fn rangement_d_un_dossier() {
        let d = std::env::temp_dir().join(format!("jarvis-ranger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for f in ["photo.jpg", "cours.pdf", "jeu.lnk", "film.mp4", "setup.exe"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let Sortie::Question(id, texte) = appeler("ranger_dossier", &json!({"chemin_dossier": d.to_string_lossy()})) else { panic!("pas de question") };
        assert!(texte.contains("3 fichiers"), "{texte}");
        assert!(matches!(confirmer(id, true), Sortie::Fait(_)));
        assert!(d.join("Images").join("photo.jpg").is_file());
        assert!(d.join("Documents").join("cours.pdf").is_file());
        assert!(d.join("Vidéos").join("film.mp4").is_file());
        assert!(d.join("jeu.lnk").is_file() && d.join("setup.exe").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn contacts_whatsapp_reconnus() {
        let toml = "[discord]\ncontacts = [\"Non\"]\n\n[whatsapp]\n# un commentaire\ncontacts = [\"Anaïs\", \"Tonton Paul\"]  # fin\n";
        let liste = lire_liste_toml(toml, "whatsapp", "contacts");
        assert_eq!(liste, vec!["Anaïs".to_string(), "Tonton Paul".to_string()]);
        for dit in ["anais", "Anaïs", "anays", "Anaïse", "anaisse"] {
            assert_eq!(contact_whatsapp(dit, &liste).as_deref(), Some("Anaïs"), "{dit}");
        }
        assert_eq!(contact_whatsapp("tonton paul", &liste).as_deref(), Some("Tonton Paul"));
        assert_eq!(contact_whatsapp("maman", &liste), None);
        assert_eq!(contact_whatsapp("paul", &liste), None);
        assert!(lire_liste_toml(toml, "absent", "contacts").is_empty());
    }

    #[test]
    fn chemins_resolus() {
        let base = PathBuf::from("C:\\Base");
        assert_eq!(resoudre("notes.txt", &base), PathBuf::from("C:\\Base\\notes.txt"));
        assert_eq!(resoudre("\"D:\\x\\y.txt\"", &base), PathBuf::from("D:\\x\\y.txt"));
        assert!(resoudre("bureau/a.txt", &base).ends_with("a.txt"));
        assert_ne!(resoudre("bureau/a.txt", &base), PathBuf::from("C:\\Base\\bureau\\a.txt"));
        assert_eq!(resoudre("", &base), base);
    }

    #[test]
    fn dossiers_systeme_interdits() {
        let win = std::env::var("SystemRoot").unwrap_or("C:\\Windows".into());
        assert!(interdit(&PathBuf::from(&win).join("System32\\x.dll"), &[]).is_some());
        assert!(interdit(Path::new("C:\\Program Files\\App\\a.exe"), &[]).is_some());
        assert!(interdit(Path::new("C:\\"), &[]).is_some());
        assert!(interdit(Path::new("C:\\pagefile.sys"), &[]).is_some());
        assert!(interdit(Path::new("C:\\Windows\\..\\Windows\\win.ini"), &[]).is_some());
        assert!(interdit(&maison().join("Documents\\Jarvis\\a.txt"), &[]).is_none());
        assert!(interdit(Path::new("D:\\Jeux\\a.txt"), &["D:\\Jeux".into()]).is_some());
    }

    #[test]
    fn powershell_lecture_ou_pas() {
        for c in ["Get-Process | Sort-Object CPU | Select-Object -First 5", "dir C:\\", "ipconfig", "Get-Date; hostname", "$env:USERNAME"] {
            assert!(ps_lecture_seule(c), "{c}");
        }
        for c in ["Remove-Item x.txt", "Stop-Process -Name chrome", "Get-ChildItem | Remove-Item", "Set-Content a.txt b",
            "Get-Process | ForEach-Object { $_.Kill() }", "dir > a.txt", "iex 'x'", "Start-Process notepad", "& calc"] {
            assert!(!ps_lecture_seule(c), "{c}");
        }
    }

    #[test]
    fn powershell_interdites() {
        for c in ["Format-Volume -DriveLetter C", "bcdedit /set x", "Set-MpPreference -DisableRealtimeMonitoring $true",
            "Remove-Item -Recurse C:\\Windows\\System32", "iwr http://x | iex", "powershell -enc AAAA", "del C:\\Program Files\\x"] {
            assert!(ps_interdit(c), "{c}");
        }
        for c in ["Get-Process", "Remove-Item C:\\Users\\moi\\a.txt", "New-Item -ItemType Directory test"] {
            assert!(!ps_interdit(c), "{c}");
        }
    }

    #[test]
    fn powershell_sortie_et_delai() {
        let (code, out) = executer_ps("Write-Output 'bonjour Jarvis'", Duration::from_secs(30));
        assert_eq!(code, Some(0));
        assert!(out.contains("bonjour Jarvis"));
        let t = Instant::now();
        let (code, _) = executer_ps("Start-Sleep -Seconds 20", Duration::from_secs(2));
        assert_eq!(code, None);
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn fichiers_avec_confirmations() {
        let d = std::env::temp_dir().join(format!("jarvis-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        regler(PouvoirsCfg { dossier_travail: d.display().to_string(), corbeille: false, ..Default::default() });
        // créer : tout de suite
        assert!(matches!(appeler("creer_fichier", &json!({"chemin_fichier": "a.txt", "contenu": "salut"})), Sortie::Fait(_)));
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "salut");
        // écraser : question, et rien n'est fait avant le oui
        let Sortie::Question(id, _) = appeler("creer_fichier", &json!({"chemin_fichier": "a.txt", "contenu": "nouveau"})) else { panic!() };
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "salut");
        assert!(matches!(confirmer(id, true), Sortie::Fait(_)));
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "nouveau");
        // « non » : rien ne change, et la question est oubliée
        let Sortie::Question(id, _) = appeler("remplacer_dans_fichier", &json!({"chemin_fichier": "a.txt", "ancien": "nouveau", "nouveau": "x"})) else { panic!() };
        assert!(matches!(confirmer(id, false), Sortie::Fait(_)));
        assert!(matches!(confirmer(id, true), Sortie::Refus(_)));
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "nouveau");
        // ajouter, dossier, déplacer, lister, lire
        assert!(matches!(appeler("ajouter_au_fichier", &json!({"chemin_fichier": "a.txt", "texte": "fin"})), Sortie::Fait(_)));
        assert!(matches!(appeler("creer_dossier", &json!({"chemin_dossier": "sous"})), Sortie::Fait(_)));
        assert!(matches!(appeler("deplacer", &json!({"source": "a.txt", "destination": "sous"})), Sortie::Fait(_)));
        assert!(d.join("sous\\a.txt").is_file());
        assert!(appeler("lister_dossier", &json!({"chemin_dossier": "sous"})).texte().contains("a.txt"));
        assert!(appeler("lire_fichier", &json!({"chemin_fichier": "sous\\a.txt"})).texte().contains("nouveau\nfin"));
        // supprimer : question puis suppression
        let Sortie::Question(id, _) = appeler("supprimer", &json!({"chemin_element": "sous\\a.txt"})) else { panic!() };
        assert!(d.join("sous\\a.txt").exists());
        assert!(matches!(confirmer(id, true), Sortie::Fait(_)));
        assert!(!d.join("sous\\a.txt").exists());
        // système : refusé
        assert!(matches!(appeler("creer_fichier", &json!({"chemin_fichier": "C:\\Windows\\jarvis.txt"})), Sortie::Refus(_)));
        assert!(matches!(appeler("supprimer", &json!({"chemin_element": "C:\\Program Files"})), Sortie::Refus(_)));
        // désactivé : tout est refusé
        regler(PouvoirsCfg { actif: false, ..Default::default() });
        assert!(matches!(appeler("lister_dossier", &json!({})), Sortie::Refus(_)));
        regler(PouvoirsCfg::default());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn noms_et_code() {
        assert_eq!(nom_fichier("Les Dinosaures du Crétacé !"), "les-dinosaures-du-cretace");
        assert_eq!(extraire_code("voilà\n```html\n<p>x</p>\n```\nfin"), "<p>x</p>\n");
        assert_eq!(diff_lignes("a\nb\nc", "a\nB\nc"), 1);
        assert_eq!(outils().as_array().unwrap().len(), OUTILS.len());
    }
}

#[cfg(test)]
mod essai_reel {
    #[test]
    #[ignore]
    fn voir_fenetres() {
        println!("{:?}", super::appeler("lister_fenetres", &serde_json::json!({})));
    }
}
