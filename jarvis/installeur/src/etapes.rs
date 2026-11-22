//! Les étapes de l'installation (dans un fil à part) et la désinstallation.

use std::ffi::OsStr;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_YESNO};

use crate::{boite, poster, profils, WM_ETAT, WM_FINI, WM_JOURNAL, WM_PROGRES};

pub static CODE_SORTIE: AtomicI32 = AtomicI32::new(0);
static JOURNAL: Mutex<String> = Mutex::new(String::new());
static CHARGE: &[u8] = include_bytes!(env!("JARVIS_CHARGE"));
const SANS_FENETRE: u32 = 0x0800_0000;
const DETACHE: u32 = 0x0000_0008 | 0x0000_0200;
/// Jarvis a déjà été lancé avant la fin (le modèle local finissait de se télécharger) : pas de 2e lancement.
static LANCE: AtomicBool = AtomicBool::new(false);
/// Pour le temps restant : début de l'installation, étape affichée, dernière estimation (en minutes).
static DEBUT: Mutex<Option<Instant>> = Mutex::new(None);
static ETAPE: Mutex<String> = Mutex::new(String::new());
static RESTE: AtomicU64 = AtomicU64::new(u64::MAX);
const PAS_LENT: &str = "PC trop lent ou pas de place ? Tu peux aussi coller une clé gratuite (Google Gemini ou Groq) \
dans les réglages de Jarvis : clic droit sur la barre, Paramètres, IA.";
const CLE_DESINSTALLATION: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Jarvis";
const CLE_DEMARRAGE: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Clone)]
pub struct Options {
    pub dossier: PathBuf,
    pub demarrage: bool,
    pub bureau: bool,
    pub lmstudio: bool,
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
            lmstudio: true,
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
    if let Ok(mut e) = ETAPE.lock() {
        *e = t.clone();
    }
    RESTE.store(u64::MAX, Ordering::Relaxed);
    poster(WM_ETAT, 0, Some(t));
}

/// La barre de progression, et le temps restant à côté de l'étape (« — encore ≈ 4 min »).
fn progres(p: usize) {
    let p = p.min(1000);
    poster(WM_PROGRES, p, None);
    let Some(debut) = DEBUT.lock().ok().and_then(|d| *d) else { return };
    let ecoule = debut.elapsed().as_secs();
    if p < 40 || p >= 1000 || ecoule < 15 {
        return;
    }
    let minutes = (ecoule * (1000 - p as u64) / p as u64 + 30) / 60;
    if RESTE.swap(minutes, Ordering::Relaxed) != minutes {
        let etape = ETAPE.lock().map(|e| e.clone()).unwrap_or_default();
        let reste = if minutes == 0 { "moins d'une minute".to_string() } else { format!("≈ {minutes} min") };
        poster(WM_ETAT, 0, Some(format!("{etape} — encore {reste}")));
    }
}

// ── installation ─────────────────────────────────────────────────────────────

pub fn installer(o: &Options) {
    let debut = Instant::now();
    if let Ok(mut d) = DEBUT.lock() {
        *d = Some(debut);
    }
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
    if o.relancer && !LANCE.load(Ordering::Relaxed) {
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

    etat("Je regarde ton PC");
    let materiel = profils::detecter();
    let profil = profils::choisir(&materiel);
    journal(format!("Ton PC : {} → profil « {} » ({}).", profils::resume(&materiel), profil.nom, profil.pour));
    // la barre de Jarvis reprendra cette détection (sans refaire le test)
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dossier = Path::new(&appdata).join(r"Jarvis\barre");
        let _ = std::fs::create_dir_all(&dossier);
        let _ = std::fs::write(dossier.join("materiel.txt"), profils::vers_texte(&materiel));
    }
    if ancienne_config.is_none() {
        adapter_config(d, profil, &materiel)?;
    }
    // L'IA sur ce PC (LM Studio et le modèle du profil) se télécharge EN MÊME TEMPS que le reste.
    let ia_locale = if o.lmstudio {
        let (d2, modele) = (d.to_path_buf(), profil.modele_local.to_string());
        journal(format!("Modèle d'IA {modele} (≈ {:.1} Go) : téléchargement en arrière-plan, pendant le reste.", profil.taille_go));
        Some(std::thread::spawn(move || modele_local(&d2, &modele)))
    } else {
        journal("IA sur ce PC : pas installée (tu utiliseras ta clé NVIDIA : mode ultra performant).");
        None
    };
    let gpu = materiel.nvidia || carte_nvidia();
    if gpu {
        journal("Carte graphique NVIDIA trouvée : ta voix sera transcrite par la carte graphique.");
    } else {
        journal("Pas de carte NVIDIA : la transcription se fera sur le processeur, avec un modèle plus petit.");
    }

    etat("Outil Python (uv)");
    progres(60);
    let uv = trouver_ou_installer_uv(d)?;

    etat("Python et ses bibliothèques (≈ 1,5 Go : quelques minutes)");
    executer(&uv, &["venv", "--allow-existing", "--python", "3.12", ".venv"], d, 70, 100)?;
    let exigences = if gpu { "requirements.txt".to_string() } else { exigences_sans_gpu(d)? };
    // déjà à jour (même liste que la dernière fois) : on saute. Sinon uv garde ses téléchargements en cache.
    let marque = d.join(r".venv\jarvis-exigences.txt");
    let liste = std::fs::read_to_string(d.join(&exigences)).unwrap_or_default();
    if !liste.is_empty() && std::fs::read_to_string(&marque).ok().as_deref() == Some(liste.as_str()) {
        journal("Bibliothèques Python : déjà à jour, je saute cette étape.");
        progres(450);
    } else {
        executer(&uv, &["pip", "install", "--python", r".venv\Scripts\python.exe", "-r", &exigences], d, 100, 450)?;
        let _ = std::fs::write(&marque, &liste);
    }

    etat("Modèles de voix et d'écoute (≈ 1,7 Go)");
    executer(d.join(r".venv\Scripts\python.exe"), &[r"outils\telecharger_modeles.py"], d, 450, 700)?;

    if o.raccourcis {
        etat("Raccourcis et menu Démarrer");
        progres(720);
        raccourcis(d, o)?;
        inscrire_desinstallation(d)?;
    }

    if let Some(fil) = ia_locale {
        if !fil.is_finished() {
            // tout le reste est prêt : Jarvis marche déjà (avec une clé d'IA), le modèle finit en arrière-plan
            lancer_jarvis(d);
            LANCE.store(true, Ordering::Relaxed);
            journal("Jarvis est lancé ! Le modèle sur ce PC finit de se télécharger : en attendant, Jarvis utilise ta clé d'IA si tu en as une.");
            etat("Modèle d'IA : je finis le téléchargement en arrière-plan");
            progres(760);
        }
        let _ = fil.join();
    }
    journal(PAS_LENT);
    Ok(())
}

/// LM Studio puis le modèle du profil, dans un fil à part (sans toucher à la barre de progression).
/// Déjà là : on saute. La quantification demandée n'existe pas : on prend la version conseillée par LM Studio.
fn modele_local(d: &Path, modele: &str) {
    if trouver_lms().is_none() {
        installer_lmstudio(d);
    }
    let Some(lms) = trouver_lms() else {
        journal("LM Studio n'est pas encore prêt. Ouvre-le une fois : Jarvis prendra le modèle de ton profil.");
        return;
    };
    let base = modele.split('@').next().unwrap_or(modele);
    if lms_a(&lms, base) {
        journal(format!("Modèle {base} : déjà sur ce PC, je saute."));
        return;
    }
    for essai in [modele, base] {
        if executer(&lms, &["get", essai, "--yes"], d, 0, 0).is_ok() || lms_a(&lms, base) {
            journal(format!("Modèle {essai} : prêt."));
            return;
        }
    }
    journal(format!("Le modèle {modele} n'a pas pu être téléchargé. Ouvre LM Studio et télécharge-le à la main."));
}

/// Le modèle est déjà dans LM Studio (`lms ls`).
fn lms_a(lms: &Path, base: &str) -> bool {
    Command::new(lms)
        .arg("ls")
        .creation_flags(SANS_FENETRE)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains(&base.to_lowercase()))
        .unwrap_or(false)
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
    let barre = fin > 0;
    if barre {
        progres(debut);
    }
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
    if barre {
        progres(fin);
    }
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
        if fin > 0 {
            progres(debut + (n * 2).min(fin.saturating_sub(debut + 1)));
        }
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

/// Nouvelle installation : la transcription de la voix du profil (Whisper « small » sur les petits PC).
fn adapter_config(d: &Path, p: &profils::Profil, m: &profils::Materiel) -> Result<(), String> {
    let (modele, appareil, precision) = profils::voix(p, m);
    let chemin = d.join("config.toml");
    let texte = std::fs::read_to_string(&chemin).map_err(|e| e.to_string())?;
    let texte = texte
        .replacen("modele = \"large-v3-turbo\"", &format!("modele = \"{modele}\""), 1)
        .replacen("appareil = \"auto\"", &format!("appareil = \"{appareil}\""), 1)
        .replacen("precision = \"int8_float16\"", &format!("precision = \"{precision}\""), 1);
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

/// lms.exe de LM Studio : il apparaît quand LM Studio a été ouvert une fois.
fn trouver_lms() -> Option<PathBuf> {
    let exe = PathBuf::from(std::env::var("USERPROFILE").ok()?).join(r".lmstudio\bin\lms.exe");
    exe.is_file().then_some(exe)
}

/// Installe LM Studio avec winget. Si ça échoue, on ouvre la page de téléchargement et on explique quoi faire.
fn installer_lmstudio(d: &Path) {
    journal("── Installation de LM Studio (le moteur d'IA, gratuit), en arrière-plan");
    let ok = executer(
        "winget",
        &["install", "--id", "ElementLabs.LMStudio", "-e", "--silent",
          "--accept-package-agreements", "--accept-source-agreements"],
        d,
        0,
        0,
    );
    if ok.is_err() {
        journal("L'installation automatique de LM Studio n'a pas marché. Je t'ouvre la page de téléchargement : \
installe LM Studio, puis ouvre-le une fois.");
        let _ = Command::new("explorer.exe")
            .arg("https://lmstudio.ai/download")
            .creation_flags(SANS_FENETRE)
            .spawn();
    } else {
        journal("LM Studio est installé. Il faut l'ouvrir une fois pour que son outil `lms` soit prêt.");
    }
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
        // --demarrage : Jarvis.exe sait que c'est Windows qui le lance (réglage « voix au démarrage du PC »)
        reg(&["add", CLE_DEMARRAGE, "/v", "Jarvis", "/t", "REG_SZ", "/d", &format!("\"{}\" --demarrage", exe.display()), "/f"])?;
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
        "Désinstaller Jarvis et sa barre ?\n\n(LM Studio, ses modèles et tes réglages sont gardés.)",
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
