//! Profils matériel tout prêts : chaque PC reçoit le bon petit modèle local (et sa quantification),
//! la bonne transcription de la voix et le bon nombre d'outils pour le petit modèle.
//!
//! Ce fichier n'utilise que la bibliothèque standard : l'installeur l'inclut aussi
//! (`#[path = "../../barre/src/profils.rs"]`), pour choisir quoi télécharger.
//!
//! Deux modes de modèle (réglés dans la grande application › Paramètres) :
//! - ultra performant (clé NVIDIA) : `NVIDIA_GROS` pour le texte, `NVIDIA_VISION` pour les images, tout est permis ;
//! - ultra léger (sur ce PC) : le modèle local du profil, moins d'outils, sans les actions lourdes.
#![allow(dead_code)]

use std::os::windows::process::CommandExt;
use std::process::Command;

const SANS_FENETRE: u32 = 0x0800_0000;

/// Gros modèle NVIDIA du mode ultra performant (mesuré le 2026-11-21 : 3/3 outils, 1,25 s ; ne lit pas les images).
pub const NVIDIA_GROS: &str = "nvidia/nemotron-3-ultra-550b-a55b";
/// Modèle NVIDIA qui voit les images (captures d'écran) : 3/3 outils, 1,05 s.
pub const NVIDIA_VISION: &str = "nvidia/nemotron-3.5-super-vl-120b-a12b";
/// Le plus rapide (337 ms) : plus petit, pour les ordres simples.
pub const NVIDIA_RAPIDE: &str = "nvidia/nemotron-3.5-lightning-30b-a3b";

/// Le choix du modèle, dans l'ordre des flèches ‹ › de la grande application : (n° de mode, ce qui est affiché).
pub const CHOIX_MODELE: [(u8, &str); 5] = [
    (0, "Auto : le plus fort s'il y a une clé NVIDIA"),
    (1, "NVIDIA, le plus fort (550B)"),
    (3, "NVIDIA, rapide et voit les images (120B)"),
    (4, "NVIDIA, très rapide (30B)"),
    (2, "Sur ce PC, gratuit (LM Studio)"),
];

/// « passe sur le plus rapide », « mets le modèle local »… → n° de mode (None = pas compris).
pub fn mode_depuis(texte: &str) -> Option<u8> {
    let t = texte.to_lowercase();
    let a = |mots: &[&str]| mots.iter().any(|m| t.contains(m));
    if a(&["auto"]) {
        Some(0)
    } else if a(&["très rapide", "tres rapide", "30b", "lightning", "le plus rapide"]) {
        Some(4)
    } else if a(&["local", "ce pc", "lm studio", "léger", "leger", "gratuit", "hors ligne"]) {
        Some(2)
    } else if a(&["image", "vision", "120", "rapide"]) {
        Some(3)
    } else if a(&["fort", "550", "puissant", "performant", "intelligent", "gros", "meilleur"]) {
        Some(1)
    } else {
        None
    }
}

pub struct Profil {
    pub id: &'static str,
    pub nom: &'static str,
    /// Pour quel PC (affiché dans les paramètres).
    pub pour: &'static str,
    /// Modèle LM Studio, avec sa quantification (« qwen/qwen3.5-9b@q6_k »).
    pub modele_local: &'static str,
    /// Taille du téléchargement du modèle local, en Go.
    pub taille_go: f32,
    /// Transcription de la voix ([transcription] de config.toml).
    pub whisper: &'static str,
    pub appareil: &'static str,
    pub precision: &'static str,
    /// Outils donnés au petit modèle en mode léger.
    pub max_outils: usize,
}

pub const PROFILS: [Profil; 5] = [
    Profil {
        id: "petit",
        nom: "Petite config",
        pour: "sans carte graphique (ou moins de 4 Go)",
        modele_local: "qwen/qwen3.5-2b@q4_k_m",
        taille_go: 1.5,
        whisper: "small",
        appareil: "cpu",
        precision: "int8",
        max_outils: 8,
    },
    Profil {
        id: "portable",
        nom: "PC portable ou petite carte",
        pour: "PC portable, ou carte graphique de 4 à 6 Go",
        modele_local: "qwen/qwen3.5-4b@q4_k_m",
        taille_go: 2.8,
        whisper: "small",
        appareil: "auto",
        precision: "int8_float16",
        max_outils: 10,
    },
    Profil {
        id: "moyen",
        nom: "Config moyenne",
        pour: "carte graphique de 8 Go (RTX 3060, 4060, 5060…)",
        modele_local: "qwen/qwen3.5-9b@q4_k_m",
        taille_go: 5.7,
        whisper: "large-v3-turbo",
        appareil: "auto",
        precision: "int8_float16",
        max_outils: 12,
    },
    Profil {
        id: "gros",
        nom: "Grosse config",
        pour: "carte graphique de 10 à 12 Go (RTX 3080, 4070, 5070…)",
        modele_local: "qwen/qwen3.5-9b@q6_k",
        taille_go: 7.5,
        whisper: "large-v3-turbo",
        appareil: "auto",
        precision: "int8_float16",
        max_outils: 12,
    },
    Profil {
        id: "tres_gros",
        nom: "Très grosse config",
        pour: "carte graphique de 16 Go ou plus (RTX 4080, 4090, 5080, 5090…)",
        modele_local: "qwen/qwen3.5-9b@q8_0",
        taille_go: 9.5,
        whisper: "large-v3-turbo",
        appareil: "auto",
        precision: "float16",
        max_outils: 14,
    },
];

pub fn par_id(id: &str) -> Option<&'static Profil> {
    PROFILS.iter().find(|p| p.id == id)
}

/// Le modèle local sans sa quantification (« qwen/qwen3.5-9b »), tel que LM Studio le liste.
pub fn base_modele(p: &Profil) -> &'static str {
    p.modele_local.split('@').next().unwrap_or(p.modele_local)
}

/// Ce que Jarvis sait de ce PC.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Materiel {
    pub gpu: String,
    pub vram_mo: u64,
    pub nvidia: bool,
    pub ram_mo: u64,
    pub cpu: String,
    pub coeurs: u32,
    pub portable: bool,
}

/// Dernière détection connue (pour l'affichage des paramètres).
static CONNU: std::sync::Mutex<Option<Materiel>> = std::sync::Mutex::new(None);

pub fn retenir(m: Materiel) {
    *CONNU.lock().unwrap_or_else(|e| e.into_inner()) = Some(m);
}

pub fn materiel_connu() -> Option<Materiel> {
    CONNU.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Le bon profil pour ce PC.
pub fn choisir(m: &Materiel) -> &'static Profil {
    let v = m.vram_mo;
    let id = if v < 3500 {
        "petit"
    } else if v < 7000 || (m.portable && v < 7500) {
        "portable"
    } else if v < 9500 {
        "moyen"
    } else if v < 15000 {
        "gros"
    } else {
        "tres_gros"
    };
    par_id(id).unwrap_or(&PROFILS[2])
}

/// Transcription de la voix pour ce profil et ce PC : sans carte NVIDIA, Whisper « small » sur le processeur.
pub fn voix(p: &Profil, m: &Materiel) -> (&'static str, &'static str, &'static str) {
    if m.nvidia {
        (p.whisper, p.appareil, p.precision)
    } else {
        ("small", "cpu", "int8")
    }
}

/// « RTX 5070 (12 Go) · 32 Go de RAM · 16 cœurs · portable »
pub fn resume(m: &Materiel) -> String {
    let gpu = if m.gpu.is_empty() || m.vram_mo < 1000 {
        "pas de vraie carte graphique".to_string()
    } else {
        let nom = m.gpu.replace("NVIDIA ", "").replace("GeForce ", "").replace("AMD ", "").replace("Radeon ", "Radeon ");
        format!("{nom} ({} Go)", (m.vram_mo + 512) / 1024)
    };
    let mut s = format!("{gpu} · {} Go de RAM · {} cœurs", (m.ram_mo + 1023) / 1024, m.coeurs);
    if m.portable {
        s.push_str(" · portable");
    }
    s
}

fn lancer(prog: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(prog).args(args).creation_flags(SANS_FENETRE).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Regarde la carte graphique (et sa vraie mémoire), la RAM, le processeur, et s'il y a une batterie.
/// Environ 1 à 2 secondes (PowerShell et nvidia-smi) : à faire hors du fil de l'interface.
pub fn detecter() -> Materiel {
    let script = "$ErrorActionPreference='SilentlyContinue';\
        $c=Get-CimInstance Win32_ComputerSystem; 'ram='+$c.TotalPhysicalMemory;\
        $p=Get-CimInstance Win32_Processor | Select-Object -First 1; 'cpu='+$p.Name; 'coeurs='+$p.NumberOfLogicalProcessors;\
        'batterie='+@(Get-CimInstance Win32_Battery).Count;\
        Get-ItemProperty 'HKLM:\\SYSTEM\\ControlSet001\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0*' |\
        ForEach-Object { 'gpu='+$_.DriverDesc+'|'+$_.'HardwareInformation.qwMemorySize' }";
    let texte = lancer("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]).unwrap_or_default();
    let mut m = lire_detection(&texte);
    // nvidia-smi donne la vraie mémoire des cartes NVIDIA
    let smi = lancer("nvidia-smi", &["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .or_else(|| lancer(r"C:\Windows\System32\nvidia-smi.exe", &["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]));
    if let Some(s) = smi {
        for l in s.lines() {
            if let Some((nom, mo)) = l.rsplit_once(',') {
                let mo: u64 = mo.trim().parse().unwrap_or(0);
                if mo >= m.vram_mo {
                    m.gpu = nom.trim().to_string();
                    m.vram_mo = mo;
                    m.nvidia = true;
                }
            }
        }
    }
    m
}

/// Lit la sortie du script de `detecter` (séparé pour les tests).
pub fn lire_detection(texte: &str) -> Materiel {
    let mut m = Materiel::default();
    for l in texte.lines() {
        let Some((k, v)) = l.trim().split_once('=') else { continue };
        let v = v.trim();
        match k {
            "ram" => m.ram_mo = v.parse::<u64>().unwrap_or(0) / (1024 * 1024),
            "cpu" => m.cpu = v.to_string(),
            "coeurs" => m.coeurs = v.parse().unwrap_or(0),
            "batterie" => m.portable = v.parse::<u32>().unwrap_or(0) > 0,
            "gpu" => {
                let (nom, mem) = v.split_once('|').unwrap_or((v, ""));
                let bas = nom.to_lowercase();
                if ["basic", "virtual", "parsec", "remote", "meta ", "dummy"].iter().any(|x| bas.contains(x)) {
                    continue;
                }
                let mo = mem.trim().parse::<u64>().unwrap_or(0) / (1024 * 1024);
                if mo >= m.vram_mo {
                    m.gpu = nom.trim().to_string();
                    m.vram_mo = mo;
                    m.nvidia = bas.contains("nvidia");
                }
            }
            _ => {}
        }
    }
    m
}

/// Pour garder la détection dans un petit fichier texte (une ligne « clé=valeur » par info).
pub fn vers_texte(m: &Materiel) -> String {
    format!(
        "gpu={}\nvram_mo={}\nnvidia={}\nram_mo={}\ncpu={}\ncoeurs={}\nportable={}\n",
        m.gpu, m.vram_mo, m.nvidia, m.ram_mo, m.cpu, m.coeurs, m.portable
    )
}

pub fn depuis_texte(t: &str) -> Option<Materiel> {
    let mut m = Materiel::default();
    let mut vu = false;
    for l in t.lines() {
        let Some((k, v)) = l.split_once('=') else { continue };
        vu = true;
        match k {
            "gpu" => m.gpu = v.to_string(),
            "vram_mo" => m.vram_mo = v.parse().unwrap_or(0),
            "nvidia" => m.nvidia = v == "true",
            "ram_mo" => m.ram_mo = v.parse().unwrap_or(0),
            "cpu" => m.cpu = v.to_string(),
            "coeurs" => m.coeurs = v.parse().unwrap_or(0),
            "portable" => m.portable = v == "true",
            _ => {}
        }
    }
    vu.then_some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choix_du_profil() {
        let pc = |vram: u64, portable: bool| Materiel { vram_mo: vram, portable, nvidia: true, ..Default::default() };
        assert_eq!(choisir(&pc(0, false)).id, "petit");
        assert_eq!(choisir(&pc(6144, false)).id, "portable");
        assert_eq!(choisir(&pc(8192, false)).id, "moyen");
        assert_eq!(choisir(&pc(8192, true)).id, "moyen");
        assert_eq!(choisir(&pc(12227, false)).id, "gros");
        assert_eq!(choisir(&pc(24564, false)).id, "tres_gros");
        assert_eq!(base_modele(par_id("gros").unwrap()), "qwen/qwen3.5-9b");
        assert_eq!(voix(par_id("gros").unwrap(), &Materiel::default()).0, "small");
        assert_eq!(mode_depuis("passe sur le plus rapide"), Some(4));
        assert_eq!(mode_depuis("mets le modèle local"), Some(2));
        assert_eq!(mode_depuis("le plus intelligent"), Some(1));
        assert_eq!(mode_depuis("celui qui voit les images"), Some(3));
    }

    #[test]
    fn lecture_de_la_detection() {
        let t = "ram=34359738368\ncpu=AMD Ryzen 7\ncoeurs=16\nbatterie=0\n\
                 gpu=Microsoft Basic Display Adapter|0\ngpu=AMD Radeon(TM) Graphics|536870912\ngpu=NVIDIA GeForce RTX 5070|4293918720\n";
        let m = lire_detection(t);
        assert_eq!(m.ram_mo, 32768);
        assert_eq!(m.coeurs, 16);
        assert!(m.nvidia && m.gpu.contains("5070") && !m.portable);
        assert_eq!(depuis_texte(&vers_texte(&m)), Some(m.clone()));
        assert!(resume(&m).contains("RTX 5070"));
    }
}
