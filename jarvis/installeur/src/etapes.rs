//! Les étapes de l'installation (dans un fil à part) et la désinstallation.

use std::ffi::OsStr;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_YESNO};

use crate::{boite, poster, WM_ETAT, WM_FINI, WM_JOURNAL, WM_PROGRES};

pub static CODE_SORTIE: AtomicI32 = AtomicI32::new(0);
static JOURNAL: Mutex<String> = Mutex::new(String::new());
static CHARGE: &[u8] = include_bytes!(env!("JARVIS_CHARGE"));
const SANS_FENETRE: u32 = 0x0800_0000;
const DETACHE: u32 = 0x0000_0008 | 0x0000_0200;
const MODELE_DEFAUT: &str = "qwen3.5:4b";
const CLE_DESINSTALLATION: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Jarvis";
const CLE_DEMARRAGE: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Clone)]
pub struct Options {
    pub dossier: PathBuf,
    pub demarrage: bool,
    pub bureau: bool,
    pub ollama: bool,
    pub raccourcis: bool,
    pub auto: bool,
    pub relancer: bool,
}

impl Options {
    pub fn par_defaut() -> Self {
        let base = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(r"C:\Jarvis"));
        Options {
            dossier: base.join("Programs").join("Jarvis"),
            demarrage: true,
            bureau: true,
            ollama: true,
            raccourcis: true,
            auto: false,
            relancer: false,
        }
    }
}

// ── affichage ────────────────────────────────────────────────────────────────

fn journal(t: impl Into<String>) {
    let t = t.into();
    if let Ok(mut j) = JOURNAL.lock() {
        j.push_str(&t);
        j.push('\n');
    }
    poster(WM_JOURNAL, 0, Some(t));
}

fn etat(t: impl Into<String>) {
    let t = t.into();
    journal(format!("── {t}"));
    poster(WM_ETAT, 0, Some(t));
}

fn progres(p: usize) {
    poster(WM_PROGRES, p.min(1000), None);
}

// ── installation ─────────────────────────────────────────────────────────────

pub fn installer(o: &Options) {
    let debut = Instant::now();
    let resultat = etapes(o);
    let minutes = (debut.elapsed().as_secs() + 30) / 60;
    match &resultat {
        Ok(()) => {
            progres(1000);
            etat(format!("C'est installé ({minutes} min) ! Clique sur « Lancer Jarvis », puis dis « Hey Jarvis »."));
            journal("Pour Discord et les vidéos : clique sur « Extension Chrome… ».");
        }
        Err(e) => {
            etat(format!("Erreur : {e}"));
            journal("Tu peux relancer l'installation : ce qui est déjà fait ne sera pas refait.");
        }
    }
    let _ = std::fs::write(o.dossier.join("installation.log"), JOURNAL.lock().map(|j| j.clone()).unwrap_or_default());
    if o.relancer {
        lancer_jarvis(&o.dossier); // mise à jour automatique : Jarvis revient tout seul (même après une erreur)
    }
    CODE_SORTIE.store(if resultat.is_ok() { 0 } else { 1 }, Ordering::Relaxed);
    poster(WM_FINI, resultat.is_ok() as usize, None);
}

fn etapes(o: &Options) -> Result<(), String> {
    let d = o.dossier.as_path();
    etat("Préparation");
    progres(10);
    std::fs::create_dir_all(d).map_err(|e| format!("impossible de créer {} : {e}", d.display()))?;
    arreter_programmes(d);

    etat("Copie de Jarvis et de sa barre");
    progres(30);
    let ancienne_config = std::fs::read(d.join("config.toml")).ok();
    let zip = std::env::temp_dir().join(format!("jarvis-installation-{}.zip", std::process::id()));
    std::fs::write(&zip, CHARGE).map_err(|e| format!("impossible d'écrire {} : {e}", zip.display()))?;
    let copie = powershell(&format!("Expand-Archive -LiteralPath {} -DestinationPath {} -Force", ps(&zip), ps(d)));
    let _ = std::fs::remove_file(&zip);
    copie?;
    if let Some(ancienne) = &ancienne_config {
        let _ = std::fs::copy(d.join("config.toml"), d.join("config.defaut.toml"));
        std::fs::write(d.join("config.toml"), ancienne).map_err(|e| e.to_string())?;
        journal("Tes réglages (config.toml) sont gardés ; les nouveaux réglages par défaut sont dans config.defaut.toml.");
    }
    journal(format!("Fichiers installés dans {}", d.display()));

    let gpu = carte_nvidia();
    if gpu {
        journal("Carte graphique NVIDIA trouvée : ta voix sera transcrite par la carte graphique.");
    } else {
        journal("Pas de carte NVIDIA : la transcription se fera sur le processeur, avec un modèle plus petit.");
        if ancienne_config.is_none() {
            adapter_config_sans_gpu(d)?;
        }
    }

    etat("Outil Python (uv)");
    progres(60);
    let uv = trouver_ou_installer_uv(d)?;

    etat("Python et ses bibliothèques (≈ 1,5 Go : quelques minutes)");
    executer(&uv, &["venv", "--allow-existing", "--python", "3.12", ".venv"], d, 70, 100)?;
    let exigences = if gpu { "requirements.txt".to_string() } else { exigences_sans_gpu(d)? };
    executer(&uv, &["pip", "install", "--python", r".venv\Scripts\python.exe", "-r", &exigences], d, 100, 450)?;

    etat("Modèles de voix et d'écoute (≈ 1,7 Go)");
    executer(d.join(r".venv\Scripts\python.exe"), &[r"outils\telecharger_modeles.py"], d, 450, 700)?;

    etat("Moteur d'IA (Ollama)");
    progres(700);
    let ollama = match trouver_ollama() {
        Some(o) => Some(o),
        None if o.ollama => Some(installer_ollama()?),
        None => None,
    };
    match ollama {
        Some(exe) => {
            attendre_ollama(&exe)?;
            let modele = lire_modele(d).unwrap_or_else(|| MODELE_DEFAUT.to_string());
            etat(format!("Modèle d'IA {modele} (≈ 3,4 Go)"));
            executer(&exe, &["pull", &modele], d, 760, 960)?;
        }
        None => journal("Ollama n'est pas installé : installe-le depuis https://ollama.com/download, puis relance l'installeur."),
    }

    if o.raccourcis {
        etat("Raccourcis et menu Démarrer");
        progres(970);
        raccourcis(d, o)?;
        inscrire_desinstallation(d)?;
    }
    Ok(())
}

// ── outils ───────────────────────────────────────────────────────────────────

/// Chemin entre apostrophes pour PowerShell.
fn ps(p: impl AsRef<OsStr>) -> String {
    format!("'{}'", p.as_ref().to_string_lossy().replace('\'', "''"))
}

fn powershell(script: &str) -> Result<String, String> {
    let sortie = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(format!("$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; {script}"))
        .stdin(Stdio::null())
        .creation_flags(SANS_FENETRE)
        .output()
        .map_err(|e| format!("PowerShell introuvable : {e}"))?;
    if sortie.status.success() {
        Ok(String::from_utf8_lossy(&sortie.stdout).into_owned())
    } else {
        Err(format!("PowerShell : {}", String::from_utf8_lossy(&sortie.stderr).trim()))
    }
}

fn reussit(programme: &str, args: &[&str]) -> bool {
    Command::new(programme)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(SANS_FENETRE)
        .status()
        .is_ok_and(|s| s.success())
}

fn sans_ansi(t: &str) -> String {
    let mut r = String::with_capacity(t.len());
    let mut c = t.chars().peekable();
    while let Some(x) = c.next() {
        if x == '\u{1b}' {
            if c.peek() == Some(&'[') {
                c.next();
                while let Some(y) = c.next() {
                    if ('@'..='~').contains(&y) {
                        break;
                    }
                }
            }
            continue;
        }
        r.push(x);
    }
    r
}

/// Lance un programme sans fenêtre et recopie ce qu'il affiche dans le journal.
fn executer(programme: impl AsRef<OsStr>, args: &[&str], dossier: &Path, debut: usize, fin: usize) -> Result<(), String> {
    let nom = Path::new(programme.as_ref()).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    progres(debut);
    let mut enfant = Command::new(programme.as_ref())
        .args(args)
        .current_dir(dossier)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .env("NO_COLOR", "1")
        .env("UV_NO_PROGRESS", "1")
        .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(SANS_FENETRE)
        .spawn()
        .map_err(|e| format!("impossible de lancer {nom} : {e}"))?;
    let lignes = Arc::new(AtomicUsize::new(0));
    let fils: Vec<_> = [
        enfant.stdout.take().map(|f| Box::new(f) as Box<dyn Read + Send>),
        enfant.stderr.take().map(|f| Box::new(f) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|flux| {
        let lignes = lignes.clone();
        std::thread::spawn(move || lire_lignes(flux, &lignes, debut, fin))
    })
    .collect();
    let statut = enfant.wait().map_err(|e| e.to_string())?;
    for f in fils {
        let _ = f.join();
    }
    progres(fin);
    if statut.success() {
        Ok(())
    } else {
        Err(format!("« {nom} » a échoué (code {}) : regarde le journal ci-dessus", statut.code().unwrap_or(-1)))
    }
}

fn lire_lignes(mut flux: Box<dyn Read + Send>, lignes: &AtomicUsize, debut: usize, fin: usize) {
    let mut tampon: Vec<u8> = Vec::new();
    let mut bloc = [0u8; 4096];
    let mut dernier_progres = Instant::now() - Duration::from_secs(5);
    let envoyer = |brut: &[u8], dernier: &mut Instant| {
        let ligne = sans_ansi(&String::from_utf8_lossy(brut)).trim().to_string();
        if ligne.is_empty() {
            return;
        }
        let barre = ligne.contains('%') || ligne.contains('━') || ligne.contains('█') || ligne.contains('▕');
        if barre && dernier.elapsed() < Duration::from_millis(1500) {
            return; // une barre de progression toutes les 1,5 s suffit
        }
        *dernier = Instant::now();
        journal(ligne);
        let n = lignes.fetch_add(1, Ordering::Relaxed) + 1;
        progres(debut + (n * 2).min(fin.saturating_sub(debut + 1)));
    };
    loop {
        let n = match flux.read(&mut bloc) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &o in &bloc[..n] {
            if o == b'\n' || o == b'\r' {
                envoyer(&tampon, &mut dernier_progres);
                tampon.clear();
            } else {
                tampon.push(o);
            }
        }
    }
    envoyer(&tampon, &mut dernier_progres);
}

fn telecharger(url: &str, cible: &Path) -> Result<(), String> {
    journal(format!("Téléchargement : {url}"));
    if reussit("curl.exe", &["--version"]) {
        let ok = Command::new("curl.exe")
            .args(["-L", "--fail", "--silent", "--show-error", "-o"])
            .arg(cible)
            .arg(url)
            .creation_flags(SANS_FENETRE)
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return Ok(());
        }
    }
    powershell(&format!("Invoke-WebRequest -UseBasicParsing -Uri '{url}' -OutFile {}", ps(cible))).map(|_| ())
}

/// Arrête Jarvis et sa barre s'ils tournent depuis ce dossier (mise à jour), sans toucher aux autres.
fn arreter_programmes(d: &Path) {
    let _ = powershell(&format!(
        "Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -and $_.ExecutablePath.StartsWith({}, 'OrdinalIgnoreCase') -and $_.ProcessId -ne {} }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force }}",
        ps(format!("{}\\", d.display())),
        std::process::id()
    ));
}

fn carte_nvidia() -> bool {
    reussit("nvidia-smi", &["-L"]) || reussit(r"C:\Windows\System32\nvidia-smi.exe", &["-L"])
}

fn adapter_config_sans_gpu(d: &Path) -> Result<(), String> {
    let chemin = d.join("config.toml");
    let texte = std::fs::read_to_string(&chemin).map_err(|e| e.to_string())?;
    let texte = texte
        .replacen("modele = \"large-v3-turbo\"", "modele = \"small\"", 1)
        .replacen("appareil = \"auto\"", "appareil = \"cpu\"", 1)
        .replacen("precision = \"float16\"", "precision = \"int8\"", 1);
    std::fs::write(&chemin, texte).map_err(|e| e.to_string())
}

fn exigences_sans_gpu(d: &Path) -> Result<String, String> {
    let texte = std::fs::read_to_string(d.join("requirements.txt")).map_err(|e| e.to_string())?;
    let filtre: String = texte.lines().filter(|l| !l.trim_start().starts_with("nvidia-")).map(|l| format!("{l}\n")).collect();
    std::fs::write(d.join("requirements-cpu.txt"), filtre).map_err(|e| e.to_string())?;
    Ok("requirements-cpu.txt".into())
}

fn trouver_ou_installer_uv(d: &Path) -> Result<PathBuf, String> {
    if reussit("uv", &["--version"]) {
        return Ok(PathBuf::from("uv"));
    }
    let candidats = [
        std::env::var("USERPROFILE").map(|u| PathBuf::from(u).join(r".local\bin\uv.exe")).ok(),
        Some(d.join(r"outils\uv\uv.exe")),
    ];
    if let Some(uv) = candidats.into_iter().flatten().find(|p| p.is_file()) {
        return Ok(uv);
    }
    let zip = std::env::temp_dir().join("uv-jarvis.zip");
    telecharger("https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip", &zip)?;
    powershell(&format!("Expand-Archive -LiteralPath {} -DestinationPath {} -Force", ps(&zip), ps(d.join(r"outils\uv"))))?;
    let _ = std::fs::remove_file(&zip);
    let uv = d.join(r"outils\uv\uv.exe");
    if uv.is_file() { Ok(uv) } else { Err("uv introuvable après téléchargement".into()) }
}

fn trouver_ollama() -> Option<PathBuf> {
    if reussit("ollama", &["--version"]) {
        return Some(PathBuf::from("ollama"));
    }
    let exe = PathBuf::from(std::env::var("LOCALAPPDATA").ok()?).join(r"Programs\Ollama\ollama.exe");
    exe.is_file().then_some(exe)
}

fn installer_ollama() -> Result<PathBuf, String> {
    etat("Installation d'Ollama (≈ 1 Go à télécharger)");
    let setup = std::env::temp_dir().join("OllamaSetup.exe");
    telecharger("https://ollama.com/download/OllamaSetup.exe", &setup)?;
    journal("Installation d'Ollama : une petite fenêtre d'Ollama peut s'afficher, c'est normal.");
    let ok = Command::new(&setup)
        .args(["/SILENT", "/NORESTART", "/SUPPRESSMSGBOXES"])
        .status()
        .is_ok_and(|s| s.success());
    let _ = std::fs::remove_file(&setup);
    if !ok {
        return Err("l'installation d'Ollama a échoué : installe-le depuis https://ollama.com/download".into());
    }
    trouver_ollama().ok_or_else(|| "Ollama introuvable après installation".into())
}

fn attendre_ollama(exe: &Path) -> Result<(), String> {
    let pret = || reussit(&exe.to_string_lossy(), &["list"]);
    if pret() {
        return Ok(());
    }
    let appli = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join(r"Programs\Ollama\ollama app.exe");
    if appli.is_file() {
        let _ = Command::new(&appli).creation_flags(DETACHE).spawn();
    } else {
        let _ = Command::new(exe).arg("serve").creation_flags(SANS_FENETRE | DETACHE).spawn();
    }
    for _ in 0..40 {
        std::thread::sleep(Duration::from_secs(1));
        if pret() {
            return Ok(());
        }
    }
    Err("Ollama ne démarre pas : lance-le à la main (icône du lama), puis relance l'installeur".into())
}

/// Le modèle d'IA choisi dans config.toml (section [cerveau]).
fn lire_modele(d: &Path) -> Option<String> {
    let texte = std::fs::read_to_string(d.join("config.toml")).ok()?;
    let mut dans_cerveau = false;
    for ligne in texte.lines().map(str::trim) {
        if ligne.starts_with('[') {
            dans_cerveau = ligne == "[cerveau]";
        } else if dans_cerveau && ligne.starts_with("modele") {
            return ligne.split('"').nth(1).map(str::to_string);
        }
    }
    None
}

fn raccourcis(d: &Path, o: &Options) -> Result<(), String> {
    let exe = d.join("Jarvis.exe");
    let creer = |lien: String| {
        format!(
            "$r=(New-Object -ComObject WScript.Shell).CreateShortcut({lien}); $r.TargetPath={}; $r.WorkingDirectory={}; \
             $r.IconLocation={}; $r.Description='Assistant vocal Jarvis'; $r.Save();",
            ps(&exe), ps(d), ps(format!("{},0", exe.display()))
        )
    };
    let mut script = creer("(Join-Path ([Environment]::GetFolderPath('Programs')) 'Jarvis.lnk')".into());
    if o.bureau {
        script.push_str(&creer("(Join-Path ([Environment]::GetFolderPath('Desktop')) 'Jarvis.lnk')".into()));
    }
    powershell(&script)?;
    if o.demarrage {
        reg(&["add", CLE_DEMARRAGE, "/v", "Jarvis", "/t", "REG_SZ", "/d", &format!("\"{}\"", exe.display()), "/f"])?;
        journal("Jarvis démarrera avec Windows (modifiable dans le menu de son icône).");
    }
    Ok(())
}

fn reg(args: &[&str]) -> Result<(), String> {
    if reussit("reg", args) { Ok(()) } else { Err(format!("registre : reg {}", args.join(" "))) }
}

fn inscrire_desinstallation(d: &Path) -> Result<(), String> {
    let desinstalleur = d.join("Desinstaller-Jarvis.exe");
    if let Ok(moi) = std::env::current_exe() {
        if moi != desinstalleur {
            let _ = std::fs::copy(&moi, &desinstalleur);
        }
    }
    let commande = format!("\"{}\" --desinstaller", desinstalleur.display());
    let version = std::fs::read_to_string(d.join("VERSION")).map(|v| v.trim().to_string()).unwrap_or_else(|_| "1.0.0".into());
    let icone = format!("{},0", d.join("Jarvis.exe").display());
    let dossier = d.display().to_string();
    for (nom, valeur) in [
        ("DisplayName", "Jarvis (assistant vocal + sa barre)"),
        ("DisplayVersion", version.as_str()),
        ("Publisher", "Jarvis"),
        ("UninstallString", commande.as_str()),
        ("DisplayIcon", icone.as_str()),
        ("InstallLocation", dossier.as_str()),
    ] {
        reg(&["add", CLE_DESINSTALLATION, "/v", nom, "/t", "REG_SZ", "/d", valeur, "/f"])?;
    }
    for nom in ["NoModify", "NoRepair"] {
        reg(&["add", CLE_DESINSTALLATION, "/v", nom, "/t", "REG_DWORD", "/d", "1", "/f"])?;
    }
    Ok(())
}

// ── après l'installation ─────────────────────────────────────────────────────

pub fn lancer_jarvis(d: &Path) {
    let _ = Command::new(d.join("Jarvis.exe")).current_dir(d).creation_flags(DETACHE).spawn();
}

pub fn aide_extension(d: &Path) {
    let dossier = d.join("extension-chrome");
    let _ = Command::new("explorer.exe").arg(&dossier).spawn();
    let chrome = [
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramFiles(x86)").ok(),
        std::env::var("LOCALAPPDATA").ok(),
    ]
    .into_iter()
    .flatten()
    .map(|b| PathBuf::from(b).join(r"Google\Chrome\Application\chrome.exe"))
    .find(|p| p.is_file());
    if let Some(chrome) = chrome {
        let _ = Command::new(chrome).arg("chrome://extensions/").spawn();
    }
    boite(
        &format!(
            "Pour relier Chrome à Jarvis (vidéos + messages privés Discord) :\n\n\
             1. Dans la page « Extensions » de Chrome, active le « Mode développeur » (en haut à droite).\n\
             2. Clique sur « Charger l'extension non empaquetée ».\n\
             3. Choisis ce dossier (il vient de s'ouvrir) :\n   {}\n\
             4. Recharge ton onglet Discord (F5) et garde-le ouvert.\n\n\
             Attention : automatiser son compte Discord est contraire à ses conditions d'utilisation. \
             Jarvis te demande toujours confirmation avant d'envoyer un message.",
            dossier.display()
        ),
        MB_OK | MB_ICONINFORMATION,
    );
}

// ── désinstallation ──────────────────────────────────────────────────────────

pub fn desinstaller() {
    let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) else { return };
    // Sécurité : on ne supprime qu'un vrai dossier d'installation de Jarvis.
    let vrai = d.file_name().is_some_and(|n| n.eq_ignore_ascii_case("Jarvis"))
        && d.join("Jarvis.exe").is_file()
        && d.join("main.py").is_file();
    if !vrai {
        boite("Ce dossier ne ressemble pas à une installation de Jarvis : rien n'a été supprimé.", MB_OK | MB_ICONWARNING);
        return;
    }
    if boite(
        "Désinstaller Jarvis et sa barre ?\n\n(Ollama, ses modèles et tes réglages sont gardés.)",
        MB_YESNO | MB_ICONQUESTION,
    ) != IDYES
    {
        return;
    }
    arreter_programmes(&d);
    let _ = powershell(
        "foreach ($l in @((Join-Path ([Environment]::GetFolderPath('Programs')) 'Jarvis.lnk'), \
         (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Jarvis.lnk'))) { Remove-Item -LiteralPath $l -ErrorAction SilentlyContinue }",
    );
    let _ = reg(&["delete", CLE_DEMARRAGE, "/v", "Jarvis", "/f"]);
    let _ = reg(&["delete", CLE_DESINSTALLATION, "/f"]);
    if let Ok(appdata) = std::env::var("APPDATA") {
        let _ = std::fs::remove_dir_all(PathBuf::from(appdata).join("Jarvis"));
    }
    // Le désinstalleur est dans le dossier : on le supprime juste après s'être fermé.
    let _ = Command::new("cmd")
        .arg("/c")
        .raw_arg(format!("ping 127.0.0.1 -n 3 >nul & rmdir /s /q \"{}\"", d.display()))
        .creation_flags(SANS_FENETRE | DETACHE)
        .spawn();
    boite("Jarvis est désinstallé.", MB_OK | MB_ICONINFORMATION);
}
