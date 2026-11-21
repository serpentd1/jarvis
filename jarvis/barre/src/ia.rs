//! Les fournisseurs d'IA de Jarvis : LM Studio, Ollama, OpenAI, Anthropic, Groq, Gemini, OpenRouter…
//!
//! Tout le programme passe par `discuter` : on essaie les services dans l'ordre, le GRATUIT d'abord
//! (locaux gratuits, puis clés gratuites, puis payants). Le cerveau garde le format « Ollama » en interne ;
//! ici on le convertit pour chaque service.
//!
//! Les réglages et les clés sont dans ia.bin (chiffré avec DPAPI, comme le coffre des comptes).
//! Une clé n'est jamais écrite dans le journal.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::sys;

/// Pas de fenêtre pour un programme lancé en arrière-plan.
const SANS_FENETRE: u32 = 0x0800_0000;
/// Un fournisseur injoignable est sauté pendant ce temps.
const HORS_LIGNE: Duration = Duration::from_secs(60);
/// Le modèle trouvé tout seul est gardé ce temps.
const CACHE_MODELE: Duration = Duration::from_secs(600);

/// Le genre de protocole : presque tout est « compatible OpenAI ».
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Genre {
    #[default]
    OpenAi,
    Anthropic,
    Ollama,
}

/// Un service d'IA. La clé n'est jamais affichée (l'interface ne montre que si elle existe).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Fournisseur {
    pub nom: String,
    pub genre: Genre,
    pub url: String,
    pub cle: String,
    /// Vide = choisi tout seul avec la liste des modèles du service.
    pub modele: String,
    /// Petit modèle pour les ordres (vide = le modèle ci-dessus).
    pub modele_rapide: String,
    pub gratuit: bool,
    pub local: bool,
    pub on: bool,
    /// Choisi par l'utilisateur : reste en tête de la liste.
    pub choisi_en_premier: bool,
}

/// Ce qui est écrit dans ia.bin (une fois chiffré).
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Fichier {
    fournisseurs: Vec<Fournisseur>,
    /// Adresse saisie pour une clé d'un service inconnu.
    adresse: String,
}

/// Une demande au cerveau. Les messages gardent le format « Ollama » (rôle, contenu, images, appels d'outils).
pub struct Requete {
    pub messages: Vec<Value>,
    pub outils: Vec<Value>,
    pub max_mots: u32,
    pub temperature: f32,
    pub json: bool,
    /// Ordres simples : le petit modèle du fournisseur, s'il y en a un.
    pub rapide: bool,
    pub contexte: u32,
    /// Durée de garde en mémoire (Ollama seulement, ex. « 30m » ou « 0 »).
    pub garder: String,
}

pub struct Retour {
    pub contenu: String,
    pub appels: Vec<(String, Value)>,
}

pub enum Echec {
    /// Aucun service ne répond.
    Injoignable,
    /// Un service a répondu une erreur.
    Autre(String),
}

/// Erreur d'un seul fournisseur : faut-il essayer le suivant ?
struct Pas {
    suivant: bool,
    echec: Echec,
}

struct Etat {
    fournisseurs: Vec<Fournisseur>,
    adresse: String,
    /// Le dernier service qui a répondu (« LM Studio · qwen3-4b »).
    actif: Option<String>,
    hors_ligne: HashMap<String, Instant>,
    modeles: HashMap<String, (Instant, String)>,
}

static ETAT: Mutex<Option<Etat>> = Mutex::new(None);

fn avec<R>(f: impl FnOnce(&mut Etat) -> R) -> R {
    let mut g = ETAT.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.get_or_insert_with(|| Etat {
        fournisseurs: defaut(),
        adresse: String::new(),
        actif: None,
        hors_ligne: HashMap::new(),
        modeles: HashMap::new(),
    });
    f(e)
}

/// Les deux services de départ : LM Studio, puis Ollama (tous deux gratuits, sur ce PC).
fn defaut() -> Vec<Fournisseur> {
    vec![
        Fournisseur {
            nom: "LM Studio".into(),
            genre: Genre::OpenAi,
            url: "http://127.0.0.1:1234/v1".into(),
            gratuit: true,
            local: true,
            on: true,
            ..Default::default()
        },
        Fournisseur {
            nom: "Ollama".into(),
            genre: Genre::Ollama,
            url: "http://127.0.0.1:11434".into(),
            gratuit: true,
            local: true,
            on: true,
            ..Default::default()
        },
    ]
}

fn chemin() -> PathBuf {
    crate::config::dir().join("ia.bin")
}

/// Lit ia.bin. Fichier absent : les services de départ, et l'ancienne adresse d'Ollama (réglages mails) si elle existe.
pub fn charger(ancienne_url_ollama: &str) {
    let lu = std::fs::read(chemin()).ok().and_then(|b| sys::unprotect(&b)).and_then(|b| serde_json::from_slice::<Fichier>(&b).ok());
    let (mut fournisseurs, adresse) = match lu {
        Some(f) => (f.fournisseurs, f.adresse),
        None => {
            let mut d = defaut();
            let ancienne = adresse_normale(ancienne_url_ollama);
            if !ancienne.is_empty() {
                if let Some(o) = d.iter_mut().find(|f| f.genre == Genre::Ollama) {
                    o.url = ancienne;
                }
            }
            (d, String::new())
        }
    };
    // une clé NVIDIA enregistrée avec l'adresse du site (build.nvidia.com) : on remet la bonne adresse
    for f in fournisseurs.iter_mut().filter(|f| est_nvidia(f)) {
        f.url = NVIDIA_URL.to_string();
        f.gratuit = true; // build.nvidia.com : des crédits gratuits
        if f.nom.to_lowercase().contains("nvidia.com") {
            f.nom = "NVIDIA".into();
        }
    }
    cle_nvidia_env(&mut fournisseurs);
    trier(&mut fournisseurs);
    avec(|e| {
        e.fournisseurs = fournisseurs;
        e.adresse = adresse;
    });
}

/// Écrit ia.bin, chiffré. Une erreur est juste journalisée.
pub fn enregistrer() {
    let f = avec(|e| Fichier { fournisseurs: e.fournisseurs.clone(), adresse: e.adresse.clone() });
    match serde_json::to_vec(&f).ok().and_then(|j| sys::protect(&j)) {
        Some(b) => {
            if std::fs::write(chemin(), b).is_err() {
                sys::log("ia : impossible d'écrire ia.bin");
            }
        }
        None => sys::log("ia : chiffrement impossible, les réglages de l'IA ne sont pas enregistrés"),
    }
}

/// La liste dans l'ordre d'essai (la clé est incluse : ne jamais l'afficher).
pub fn liste() -> Vec<Fournisseur> {
    avec(|e| e.fournisseurs.clone())
}

/// Le dernier service qui a répondu, par exemple « LM Studio · qwen3-4b ».
pub fn actif() -> Option<String> {
    avec(|e| e.actif.clone())
}

fn rang(f: &Fournisseur) -> u8 {
    if f.choisi_en_premier {
        0
    } else if f.gratuit && f.local {
        1
    } else if f.gratuit {
        2
    } else {
        3
    }
}

/// Gratuit d'abord : locaux gratuits, puis clés gratuites, puis payants. Le tri garde l'ordre d'arrivée dans chaque groupe.
fn trier(v: &mut [Fournisseur]) {
    v.sort_by_key(rang);
}

/// Met le service `i` tout en tête (et lui seul est « choisi »).
fn placer_en_premier(v: &mut [Fournisseur], i: usize) {
    for (j, f) in v.iter_mut().enumerate() {
        f.choisi_en_premier = j == i;
    }
    trier(v);
}

pub fn mettre_en_premier(i: usize) {
    avec(|e| placer_en_premier(&mut e.fournisseurs, i));
    enregistrer();
}

pub fn basculer(i: usize) {
    avec(|e| {
        if let Some(f) = e.fournisseurs.get_mut(i) {
            f.on = !f.on;
        }
    });
    enregistrer();
}

pub fn retirer(i: usize) {
    avec(|e| {
        if i < e.fournisseurs.len() {
            e.fournisseurs.remove(i);
        }
        trier(&mut e.fournisseurs);
    });
    enregistrer();
}

/// L'adresse saisie pour une clé d'un service inconnu.
pub fn adresse_saisie() -> String {
    avec(|e| e.adresse.clone())
}

pub fn regler_adresse(s: &str) {
    avec(|e| e.adresse = s.trim().to_string());
    enregistrer();
}

/// Le modèle du premier service (celui qui sert en premier).
pub fn modele_premier() -> String {
    avec(|e| e.fournisseurs.first().map(|f| f.modele.clone()).unwrap_or_default())
}

pub fn regler_modele_premier(s: &str) {
    avec(|e| {
        if let Some(f) = e.fournisseurs.first_mut() {
            f.modele = s.trim().to_string();
        }
    });
    enregistrer();
}

/// Le modèle NVIDIA choisi dans config.toml ([conversation] modele). Vide = choisi tout seul.
static MODELE_API: Mutex<String> = Mutex::new(String::new());

thread_local! {
    /// Modèle imposé pour une seule demande (POST /ia/discuter avec « modele » : mesurer un modèle sans sa clé).
    static MODELE_ICI: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Fait `f` avec ce modèle chez NVIDIA (vide = le modèle habituel).
pub fn avec_modele<R>(modele: &str, f: impl FnOnce() -> R) -> R {
    MODELE_ICI.with(|m| *m.borrow_mut() = modele.trim().to_string());
    let r = f();
    MODELE_ICI.with(|m| m.borrow_mut().clear());
    r
}

pub fn modele_api(modele: &str) {
    if let Ok(mut m) = MODELE_API.lock() {
        *m = modele.trim().to_string();
    }
}

const NVIDIA_URL: &str = "https://integrate.api.nvidia.com/v1";

// ── Deux modes : ultra performant (clé NVIDIA) ou ultra léger (sur ce PC) ──────────────────────────
/// 0 = auto (ultra performant s'il y a une clé NVIDIA allumée), 1 = ultra performant, 2 = ultra léger,
/// 3 = NVIDIA rapide qui voit les images, 4 = NVIDIA très rapide (voir profils::CHOIX_MODELE).
static MODE: AtomicU8 = AtomicU8::new(0);
/// Profil matériel en service (voir profils.rs).
static PROFIL: Mutex<String> = Mutex::new(String::new());

pub fn regler_mode(mode: u8, profil: &str) {
    MODE.store(mode.min(4), Ordering::Relaxed);
    if let Ok(mut p) = PROFIL.lock() {
        *p = profil.to_string();
    }
}

pub fn mode_choisi() -> u8 {
    MODE.load(Ordering::Relaxed)
}

pub fn profil() -> &'static crate::profils::Profil {
    let id = PROFIL.lock().map(|p| p.clone()).unwrap_or_default();
    crate::profils::par_id(&id).unwrap_or(&crate::profils::PROFILS[2])
}

/// Une clé NVIDIA est enregistrée et le service est allumé.
pub fn cle_nvidia() -> bool {
    liste().iter().any(|f| est_nvidia(f) && f.on && !f.cle.trim().is_empty())
}

/// Mode ultra performant en service : choisi (ou auto) ET une clé NVIDIA. Sans clé, c'est toujours le mode léger.
pub fn puissant() -> bool {
    mode_choisi() != 2 && cle_nvidia()
}

pub fn nom_mode() -> &'static str {
    match (puissant(), mode_choisi()) {
        (true, 3) => "NVIDIA rapide qui voit les images (120B)",
        (true, 4) => "NVIDIA très rapide (30B)",
        (true, _) => "ultra performant (NVIDIA 550B)",
        (false, _) => "ultra léger (sur ce PC)",
    }
}

/// Outils donnés à l'IA à chaque question : beaucoup pour le gros modèle, peu pour le petit.
pub fn max_outils() -> usize {
    if puissant() { 24 } else { profil().max_outils }
}

/// Actions trop lourdes pour le petit modèle : seulement en mode ultra performant
/// (les noms qui finissent par « _ » valent pour tous les outils qui commencent ainsi).
const OUTILS_LOURDS: [&str; 10] = [
    "executer_powershell", "creer_page_web", "modifier_page_web", "remplacer_dans_fichier", "recherche_approfondie",
    "agent_", "cliquer_sur", "action_ecran", "zone_ecran", "whatsapp_",
];

pub fn outil_permis(nom: &str) -> bool {
    puissant() || !OUTILS_LOURDS.iter().any(|x| if x.ends_with('_') { nom.starts_with(x) } else { nom == *x })
}

/// L'ordre des services selon le mode : NVIDIA d'abord en ultra performant, ceux du PC d'abord en ultra léger.
fn ordre_du_mode(mut v: Vec<Fournisseur>) -> Vec<Fournisseur> {
    let p = puissant();
    v.sort_by_key(|f| if p { !est_nvidia(f) } else { !f.local });
    v
}

fn a_des_images(r: &Requete) -> bool {
    r.messages.iter().any(|m| !images(m).is_empty())
}

fn est_nvidia(f: &Fournisseur) -> bool {
    f.url.to_lowercase().contains("nvidia.com")
}

/// La clé NVIDIA de la variable d'environnement NVIDIA_API_KEY : ajoute le service s'il manque
/// (ou lui donne la clé s'il n'en a pas). Rien n'est affiché ni journalisé.
fn cle_nvidia_env(fournisseurs: &mut Vec<Fournisseur>) {
    let Ok(cle) = std::env::var("NVIDIA_API_KEY") else { return };
    let cle = cle.trim().to_string();
    if cle.is_empty() {
        return;
    }
    match fournisseurs.iter_mut().find(|f| est_nvidia(f)) {
        Some(f) if f.cle.trim().is_empty() => f.cle = cle,
        Some(_) => {}
        None => fournisseurs.push(Fournisseur {
            nom: "NVIDIA".into(),
            genre: Genre::OpenAi,
            url: NVIDIA_URL.into(),
            cle,
            gratuit: true,
            on: true,
            ..Default::default()
        }),
    }
}

/// Le petit modèle des ordres, réglé dans config.toml (section cerveau), pour le service Ollama.
pub fn rapide_ollama(modele: &str) {
    let m = modele.trim();
    if m.is_empty() {
        return;
    }
    avec(|e| {
        for f in e.fournisseurs.iter_mut().filter(|f| f.genre == Genre::Ollama) {
            f.modele_rapide = m.to_string();
        }
    });
}

/// Adresse normale : sans « / » final, avec « http:// » si rien n'est écrit.
fn adresse_normale(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    // « build.nvidia.com » est le site web : l'IA répond à une autre adresse
    if u.to_lowercase().contains("nvidia.com") {
        return NVIDIA_URL.to_string();
    }
    if u.is_empty() {
        String::new()
    } else if u.starts_with("http://") || u.starts_with("https://") {
        u.to_string()
    } else {
        format!("http://{u}")
    }
}

/// Racine d'un service (LM Studio : « http://127.0.0.1:1234/v1 » reste tel quel).
fn adresse(url: &str) -> String {
    adresse_normale(url)
}

fn court(t: &str) -> String {
    t.chars().take(300).collect()
}

fn raison(e: &Echec) -> String {
    match e {
        Echec::Injoignable => "injoignable".into(),
        Echec::Autre(t) => court(t),
    }
}

fn suivant_si(code: u16) -> bool {
    matches!(code, 401 | 402 | 403 | 404 | 429) || code >= 500
}

fn pas_de_ureq(e: ureq::Error) -> Pas {
    match e {
        ureq::Error::Transport(_) => Pas { suivant: true, echec: Echec::Injoignable },
        ureq::Error::Status(code, r) => {
            let t = r.into_string().unwrap_or_default();
            Pas { suivant: suivant_si(code), echec: Echec::Autre(format!("{code} {}", court(&t))) }
        }
    }
}

/// Le service répond-il à cette adresse de test ?
fn repond(test: &str) -> bool {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(2)).build().get(test).call().is_ok()
}

/// Attend 20 s au plus que le service réponde.
fn attendre(test: impl Fn() -> bool) -> bool {
    let fin = Instant::now() + Duration::from_secs(20);
    while Instant::now() < fin {
        std::thread::sleep(Duration::from_secs(1));
        if test() {
            return true;
        }
    }
    false
}

/// Ollama ne répond pas : on le lance (une fois) et on attend.
fn lancer_ollama(url: &str) -> bool {
    let test = format!("{url}/api/version");
    if repond(&test) {
        return true;
    }
    let appli = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("Programs").join("Ollama").join("ollama app.exe");
    if !appli.is_file() {
        return false;
    }
    sys::log("ia : Ollama ne répond pas, je le lance");
    if Command::new(&appli).spawn().is_err() {
        return false;
    }
    attendre(|| repond(&test))
}

/// LM Studio ne répond pas : on lance son serveur (`lms server start`) en fenêtre cachée.
fn lancer_lmstudio(url: &str) -> bool {
    let racine = url.trim_end_matches("/v1");
    let test = format!("{racine}/v1/models");
    if repond(&test) {
        return true;
    }
    let Some(maison) = std::env::var_os("USERPROFILE") else { return false };
    let exe = PathBuf::from(maison).join(".lmstudio").join("bin").join("lms.exe");
    if !exe.is_file() {
        return false;
    }
    sys::log("ia : LM Studio ne répond pas, je le lance");
    if Command::new(&exe).args(["server", "start"]).creation_flags(SANS_FENETRE).spawn().is_err() {
        return false;
    }
    attendre(|| repond(&test))
}

/// Tente de lancer le service s'il est sur ce PC. `true` = il répond maintenant.
fn lancer(f: &Fournisseur) -> bool {
    let url = adresse(&f.url);
    if !(url.contains("127.0.0.1") || url.contains("localhost")) {
        return false;
    }
    if url.contains(":11434") {
        lancer_ollama(&url)
    } else if url.contains(":1234") {
        lancer_lmstudio(&url)
    } else {
        false
    }
}

fn agent(f: &Fournisseur) -> ureq::Agent {
    let connexion = if f.local { 2 } else { 6 };
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(connexion)).timeout(Duration::from_secs(120)).build()
}

fn entetes(f: &Fournisseur) -> Vec<(String, String)> {
    match f.genre {
        Genre::Anthropic => vec![("x-api-key".into(), f.cle.clone()), ("anthropic-version".into(), "2023-06-01".into())],
        Genre::OpenAi if !f.cle.is_empty() => vec![("Authorization".into(), format!("Bearer {}", f.cle))],
        _ => Vec::new(),
    }
}

/// Envoie une requête ; si le service est injoignable et qu'on peut le lancer, on le lance et on réessaie une fois.
fn avec_relance(f: &Fournisseur, mut envoi: impl FnMut() -> Result<ureq::Response, ureq::Error>) -> Result<ureq::Response, ureq::Error> {
    let mut relance = false;
    loop {
        match envoi() {
            Err(ureq::Error::Transport(e)) if !relance => {
                if lancer(f) {
                    relance = true;
                    continue;
                }
                return Err(ureq::Error::Transport(e));
            }
            autre => return autre,
        }
    }
}

fn post(f: &Fournisseur, url: &str, corps: &Value) -> Result<ureq::Response, ureq::Error> {
    let entetes = entetes(f);
    avec_relance(f, || {
        let mut req = agent(f).post(url);
        for (k, v) in &entetes {
            req = req.set(k, v);
        }
        req.send_json(corps)
    })
}

fn get(f: &Fournisseur, url: &str) -> Result<ureq::Response, ureq::Error> {
    avec_relance(f, || requete_get(f, url))
}

/// Une lecture simple, sans lancer le service.
fn requete_get(f: &Fournisseur, url: &str) -> Result<ureq::Response, ureq::Error> {
    let mut req = ureq::AgentBuilder::new().timeout(Duration::from_secs(6)).build().get(url);
    for (k, v) in &entetes(f) {
        req = req.set(k, v);
    }
    req.call()
}

fn lire_json(r: Result<ureq::Response, ureq::Error>) -> Result<Value, Pas> {
    let rep = r.map_err(pas_de_ureq)?;
    rep.into_json().map_err(|e| Pas { suivant: false, echec: Echec::Autre(e.to_string()) })
}

/// Noms des modèles d'une réponse : « data[].id » (OpenAI, Anthropic) ou « models[].name » (Ollama).
fn noms_modeles(v: &Value) -> Vec<String> {
    let liste = if v["data"].is_array() { &v["data"] } else { &v["models"] };
    liste
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["id"].as_str().or_else(|| m["name"].as_str()))
        .map(|s| s.trim_start_matches("models/").to_string())
        .collect()
}

/// Premier nom de la liste qui vérifie `f` (dans l'ordre de la liste).
/// Choisit le modèle à utiliser dans une liste : on écarte les modèles qui ne parlent pas (embedding, voix, image…),
/// puis on prend le modèle préféré de chaque service, sinon le premier.
pub fn choisir_dans(noms: &[String], url: &str, genre: Genre) -> Option<String> {
    modeles_possibles(noms, url, genre).into_iter().next()
}

/// Tous les modèles qui savent discuter, le meilleur d'abord. On écarte ceux qui ne parlent pas (embedding,
/// voix, image, code, filtres de sécurité…). Sur ce PC, l'ordre donné (modèles déjà chargés d'abord) est gardé.
pub fn modeles_possibles(noms: &[String], url: &str, genre: Genre) -> Vec<String> {
    let mauvais = [
        "embed", "whisper", "tts", "audio", "image", "dall-e", "moderation", "guard", "rerank", "starcoder", "retriever",
        "safety", "detector", "calibration", "clip", "translate", "reward", "ocr", "parse", "cosmos", "synthetic",
    ];
    let mut bons: Vec<String> = noms
        .iter()
        .map(|n| n.trim().trim_start_matches("models/").to_string())
        .filter(|n| !n.is_empty() && !mauvais.iter().any(|m| n.to_lowercase().contains(m)))
        .collect();
    let u = url.to_lowercase();
    if u.contains("127.0.0.1") || u.contains("localhost") {
        return bons;
    }
    let rang = |n: &String| -> usize {
        let n = n.to_lowercase();
        let oui = |c: bool| if c { 0 } else { 9 };
        if u.contains("openrouter") {
            oui(n.ends_with(":free"))
        } else if u.contains("groq") {
            if n.contains("llama-3.3-70b") { 0 } else { oui(n.contains("70b")) }
        } else if u.contains("googleapis") {
            oui(n.contains("flash") && !n.contains("lite"))
        } else if u.contains("openai.com") {
            oui(n.contains("mini"))
        } else if genre == Genre::Anthropic {
            oui(n.contains("haiku"))
        } else {
            // service inconnu (NVIDIA, Mistral…) : les modèles rapides et faits pour discuter d'abord
            ["lightning", "flash", "instruct", "-it", "chat", "gpt-oss"].iter().position(|g| n.contains(g)).unwrap_or(9)
        }
    };
    bons.sort_by_key(rang);
    bons
}

/// Les modèles disponibles chez un service. LM Studio : d'abord les modèles déjà chargés.
fn trouver_modeles(f: &Fournisseur) -> Result<Vec<String>, Pas> {
    let url = adresse(&f.url);
    if f.genre == Genre::Ollama {
        return Ok(noms_modeles(&lire_json(get(f, &format!("{url}/api/tags")))?));
    }
    if url.contains(":1234") {
        let racine = url.trim_end_matches("/v1");
        // sans relance : si LM Studio ne répond pas, la lecture normale ci-dessous le lancera
        if let Ok(v) = lire_json(requete_get(f, &format!("{racine}/api/v0/models"))) {
            let charges: Vec<String> = v["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|m| matches!(m["type"].as_str(), Some("llm") | Some("vlm")) && m["state"].as_str() == Some("loaded"))
                .filter_map(|m| m["id"].as_str().map(String::from))
                .collect();
            if !charges.is_empty() {
                return Ok(charges);
            }
        }
    }
    Ok(noms_modeles(&lire_json(get(f, &format!("{url}/models")))?))
}

fn cache_lire(nom: &str) -> Option<String> {
    avec(|e| e.modeles.get(nom).filter(|(t, _)| t.elapsed() < CACHE_MODELE).map(|(_, m)| m.clone()))
}

/// Le modèle à utiliser pour ce service : réglé à la main, sinon choisi tout seul (gardé 10 min).
fn modele_de(f: &Fournisseur, rapide: bool, images: bool) -> Result<String, Pas> {
    if rapide && !f.modele_rapide.trim().is_empty() {
        return Ok(f.modele_rapide.trim().to_string());
    }
    if est_nvidia(f) {
        let ici = MODELE_ICI.with(|m| m.borrow().clone());
        if !ici.is_empty() {
            return Ok(ici);
        }
        // ultra performant : le gros modèle pour le texte, celui qui voit les images pour les captures
        if puissant() {
            let m = match (mode_choisi(), images) {
                (_, true) | (3, _) => crate::profils::NVIDIA_VISION,
                (4, false) => crate::profils::NVIDIA_RAPIDE,
                _ => crate::profils::NVIDIA_GROS,
            };
            return Ok(m.to_string());
        }
        let force = MODELE_API.lock().map(|m| m.clone()).unwrap_or_default();
        if !force.is_empty() {
            return Ok(force);
        }
    }
    if !f.modele.trim().is_empty() {
        return Ok(f.modele.trim().to_string());
    }
    if let Some(m) = cache_lire(&f.nom) {
        return Ok(m);
    }
    let liste = trouver_modeles(f)?;
    // sur ce PC : le modèle du profil matériel s'il est installé
    let voulu = crate::profils::base_modele(profil()).to_lowercase();
    let du_profil = if f.local { liste.iter().find(|m| m.to_lowercase().starts_with(&voulu)).cloned() } else { None };
    let m = du_profil.or_else(|| choisir_dans(&liste, &f.url, f.genre)).ok_or_else(|| Pas {
        suivant: true,
        echec: Echec::Autre(format!("{} : aucun modèle trouvé", f.nom)),
    })?;
    avec(|e| {
        e.modeles.insert(f.nom.clone(), (Instant::now(), m.clone()));
    });
    Ok(m)
}

/// Ce qui reste d'une réponse : sans les balises de réflexion, et sans les ``` autour d'un JSON.
fn finir(texte: &str, json: bool) -> String {
    let t = crate::jarvis_link::strip_think(texte);
    if json { sans_balises(&t) } else { t }
}

/// Enlève les ``` (et le nom du langage) autour d'une réponse JSON.
fn sans_balises(t: &str) -> String {
    let t = t.trim();
    let Some(a) = t.find("```") else { return t.to_string() };
    let apres = &t[a + 3..];
    // la ligne du langage (« json ») est sautée si elle est courte
    let apres = match apres.find('\n') {
        Some(i) if apres[..i].trim().len() <= 12 && !apres[..i].contains(|c: char| c == '{' || c == '[') => &apres[i + 1..],
        _ => apres,
    };
    let fin = apres.find("```").unwrap_or(apres.len());
    apres[..fin].trim().to_string()
}

/// Donne à `cb` les phrases prêtes du texte (sans <think>). `false` = arrêter.
fn diffuser(brut: &str, vu: &mut usize, fini: bool, cb: &mut dyn FnMut(&str) -> bool) -> bool {
    let montrable = crate::cerveau::visible(brut);
    let (phrases, suite) = crate::cerveau::phrases_pretes(&montrable, *vu, fini);
    *vu = suite;
    phrases.iter().all(|p| cb(p.as_str()))
}

fn images(m: &Value) -> Vec<&str> {
    m["images"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect()
}

/// Type d'une image en base64 : les JPEG commencent par « /9j/ », le reste est traité comme du PNG.
fn mime(b64: &str) -> &'static str {
    if b64.starts_with("/9j/") { "image/jpeg" } else { "image/png" }
}

fn contenu_openai(m: &Value) -> Value {
    let texte = m["content"].as_str().unwrap_or("");
    let imgs = images(m);
    if imgs.is_empty() {
        return json!(texte);
    }
    let mut morceaux = vec![json!({"type": "text", "text": texte})];
    for b in imgs {
        morceaux.push(json!({"type": "image_url", "image_url": {"url": format!("data:{};base64,{b}", mime(b))}}));
    }
    Value::Array(morceaux)
}

/// Messages au format OpenAI : chaque appel d'outil reçoit un id, et chaque résultat reprend l'id du bon appel.
fn vers_openai(messages: &[Value]) -> Vec<Value> {
    let mut sortie = Vec::new();
    let (mut n, mut ids) = (0usize, VecDeque::new());
    for m in messages {
        match m["role"].as_str().unwrap_or("user") {
            "assistant" => {
                let texte = m["content"].as_str().unwrap_or("");
                let mut o = json!({"role": "assistant", "content": texte});
                let appels = m["tool_calls"].as_array().cloned().unwrap_or_default();
                if !appels.is_empty() {
                    o["content"] = if texte.is_empty() { Value::Null } else { json!(texte) };
                    let mut liste = Vec::new();
                    for c in &appels {
                        n += 1;
                        let id = format!("appel_{n}");
                        ids.push_back(id.clone());
                        // les arguments doivent être du texte JSON
                        let args = match &c["function"]["arguments"] {
                            Value::String(s) => s.clone(),
                            a => a.to_string(),
                        };
                        liste.push(json!({"id": id, "type": "function", "function": {"name": c["function"]["name"], "arguments": args}}));
                    }
                    o["tool_calls"] = Value::Array(liste);
                }
                sortie.push(o);
            }
            "tool" => {
                let id = ids.pop_front().unwrap_or_else(|| "appel_inconnu".to_string());
                sortie.push(json!({"role": "tool", "tool_call_id": id, "content": m["content"].as_str().unwrap_or("")}));
            }
            role => sortie.push(json!({"role": role, "content": contenu_openai(m)})),
        }
    }
    sortie
}

fn contenu_anthropic(m: &Value) -> Value {
    let texte = m["content"].as_str().unwrap_or("");
    let imgs = images(m);
    if imgs.is_empty() {
        return json!(texte);
    }
    let mut blocs: Vec<Value> = imgs
        .iter()
        .map(|b| json!({"type": "image", "source": {"type": "base64", "media_type": mime(b), "data": b}}))
        .collect();
    if !texte.is_empty() {
        blocs.push(json!({"type": "text", "text": texte}));
    }
    Value::Array(blocs)
}

/// Messages au format Anthropic : les consignes du début vont dans « system », les résultats d'outils
/// qui se suivent sont regroupés dans un seul message « user ».
fn vers_anthropic(messages: &[Value]) -> (Option<String>, Vec<Value>) {
    let mut systeme: Vec<String> = Vec::new();
    let mut sortie: Vec<Value> = Vec::new();
    let (mut n, mut ids) = (0usize, VecDeque::new());
    let mut debut = true;
    for m in messages {
        let role = m["role"].as_str().unwrap_or("user");
        let texte = m["content"].as_str().unwrap_or("");
        if role == "system" {
            if debut {
                systeme.push(texte.to_string());
            } else {
                sortie.push(json!({"role": "user", "content": format!("[Consigne] {texte}")}));
            }
            continue;
        }
        debut = false;
        match role {
            "assistant" => {
                let appels = m["tool_calls"].as_array().cloned().unwrap_or_default();
                if appels.is_empty() {
                    if !texte.trim().is_empty() {
                        sortie.push(json!({"role": "assistant", "content": texte}));
                    }
                    continue;
                }
                let mut blocs = Vec::new();
                if !texte.trim().is_empty() {
                    blocs.push(json!({"type": "text", "text": texte}));
                }
                for c in &appels {
                    n += 1;
                    let id = format!("toolu_{n}");
                    ids.push_back(id.clone());
                    let input = match &c["function"]["arguments"] {
                        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
                        Value::Object(o) => Value::Object(o.clone()),
                        _ => json!({}),
                    };
                    blocs.push(json!({"type": "tool_use", "id": id, "name": c["function"]["name"], "input": input}));
                }
                sortie.push(json!({"role": "assistant", "content": blocs}));
            }
            "tool" => {
                let id = ids.pop_front().unwrap_or_else(|| "toolu_inconnu".to_string());
                let bloc = json!({"type": "tool_result", "tool_use_id": id, "content": texte});
                let suite = matches!(sortie.last(), Some(d) if d["role"] == "user" && d["content"][0]["type"] == "tool_result");
                match sortie.last_mut().filter(|_| suite).and_then(|d| d["content"].as_array_mut()) {
                    Some(c) => c.push(bloc),
                    None => sortie.push(json!({"role": "user", "content": [bloc]})),
                }
            }
            _ => sortie.push(json!({"role": "user", "content": contenu_anthropic(m)})),
        }
    }
    let systeme = if systeme.is_empty() { None } else { Some(systeme.join("\n\n")) };
    (systeme, sortie)
}

/// Les outils au format Anthropic (« input_schema » au lieu de « parameters »).
fn outils_anthropic(outils: &[Value]) -> Vec<Value> {
    outils
        .iter()
        .map(|o| json!({"name": o["function"]["name"], "description": o["function"]["description"], "input_schema": o["function"]["parameters"]}))
        .collect()
}

/// Lit les appels d'outils d'un message (format Ollama, et OpenAI qui a la même forme).
pub fn lire_appels(message: &Value, appels: &mut Vec<(String, Value)>) {
    for c in message["tool_calls"].as_array().into_iter().flatten() {
        let nom = c["function"]["name"].as_str().unwrap_or("").to_string();
        let args = match &c["function"]["arguments"] {
            Value::Object(o) => Value::Object(o.clone()),
            Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
            _ => json!({}),
        };
        if !nom.is_empty() {
            appels.push((nom, args));
        }
    }
}

fn pas_ok(e: impl Into<String>) -> Pas {
    Pas { suivant: false, echec: Echec::Autre(e.into()) }
}

fn openai(f: &Fournisseur, modele: &str, r: &Requete, flux: Option<&mut dyn FnMut(&str) -> bool>) -> Result<Retour, Pas> {
    let url = format!("{}/chat/completions", adresse(&f.url));
    let diffuse = flux.is_some() && r.outils.is_empty();
    let mut corps = json!({
        "model": modele, "messages": vers_openai(&r.messages), "temperature": r.temperature,
        "max_tokens": r.max_mots, "stream": diffuse,
    });
    if !r.outils.is_empty() {
        corps["tools"] = Value::Array(r.outils.clone());
    }
    // Les modèles qui « réfléchissent » (Qwen 3.5, Gemini…) useraient tous les jetons à réfléchir et la réponse
    // serait coupée : on leur demande de répondre tout de suite. Un service qui refuse ce réglage est retenu.
    static SANS_RAISONNEMENT: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let refuse = |nom: &str| SANS_RAISONNEMENT.lock().map(|v| v.iter().any(|n| n == nom)).unwrap_or(false);
    if !refuse(&f.nom) {
        corps["reasoning_effort"] = json!("none");
    }
    let mut essais = 0;
    let rep = loop {
        essais += 1;
        match post(f, &url, &corps) {
            Ok(x) => break x,
            // réglage inconnu de ce service : on réessaie sans (outils ou « reasoning_effort »)
            Err(ureq::Error::Status(c @ (400 | 422), x)) if essais < 3 => {
                let t = x.into_string().unwrap_or_default();
                let bas = t.to_lowercase();
                let o = corps.as_object_mut().expect("objet");
                if o.contains_key("reasoning_effort") && bas.contains("reasoning") {
                    o.remove("reasoning_effort");
                    if let Ok(mut v) = SANS_RAISONNEMENT.lock() {
                        v.push(f.nom.clone());
                    }
                } else if o.contains_key("tools") && bas.contains("tool") {
                    o.remove("tools");
                } else if o.contains_key("reasoning_effort") && (c == 422 || bas.contains("extra") || bas.contains("unrecognized")) {
                    o.remove("reasoning_effort");
                } else {
                    return Err(pas_ok(court(&t)));
                }
            }
            Err(e) => return Err(pas_de_ureq(e)),
        }
    };
    if let (true, Some(cb)) = (diffuse, flux) {
        let mut brut = String::new();
        let mut vu = 0usize;
        let mut continuer = true;
        for ligne in BufReader::new(rep.into_reader()).lines() {
            let Ok(ligne) = ligne else { break };
            let Some(d) = ligne.trim().strip_prefix("data:") else { continue };
            let d = d.trim();
            if d == "[DONE]" {
                break;
            }
            let Ok(v) = serde_json::from_str::<Value>(d) else { continue };
            let choix = &v["choices"][0];
            brut.push_str(choix["delta"]["content"].as_str().unwrap_or(""));
            let fini = choix["finish_reason"].is_string();
            if !diffuser(&brut, &mut vu, fini, cb) {
                continuer = false;
                break;
            }
            if fini {
                break;
            }
        }
        if continuer {
            diffuser(&brut, &mut vu, true, cb);
        }
        return Ok(Retour { contenu: crate::jarvis_link::strip_think(&brut), appels: Vec::new() });
    }
    let v: Value = rep.into_json().map_err(|e| pas_ok(e.to_string()))?;
    let msg = &v["choices"][0]["message"];
    let mut appels = Vec::new();
    lire_appels(msg, &mut appels);
    Ok(Retour { contenu: finir(msg["content"].as_str().unwrap_or(""), r.json), appels })
}

fn anthropic(f: &Fournisseur, modele: &str, r: &Requete, flux: Option<&mut dyn FnMut(&str) -> bool>) -> Result<Retour, Pas> {
    let url = format!("{}/messages", adresse(&f.url));
    let (systeme, messages) = vers_anthropic(&r.messages);
    let mut corps = json!({"model": modele, "max_tokens": r.max_mots, "temperature": r.temperature, "messages": messages});
    if let Some(s) = systeme {
        corps["system"] = json!(s);
    }
    if !r.outils.is_empty() {
        corps["tools"] = Value::Array(outils_anthropic(&r.outils));
    }
    let v: Value = post(f, &url, &corps).map_err(pas_de_ureq)?.into_json().map_err(|e| pas_ok(e.to_string()))?;
    let (mut texte, mut appels) = (String::new(), Vec::new());
    for b in v["content"].as_array().into_iter().flatten() {
        match b["type"].as_str() {
            Some("text") => texte.push_str(b["text"].as_str().unwrap_or("")),
            Some("tool_use") => {
                let nom = b["name"].as_str().unwrap_or("").to_string();
                if !nom.is_empty() {
                    appels.push((nom, b["input"].clone()));
                }
            }
            _ => {}
        }
    }
    let texte = finir(&texte, r.json);
    // pas de flux chez Anthropic ici : la réponse entière est découpée en phrases
    if let Some(cb) = flux.filter(|_| r.outils.is_empty()) {
        let mut vu = 0usize;
        diffuser(&texte, &mut vu, true, cb);
    }
    Ok(Retour { contenu: texte, appels })
}

fn ollama(f: &Fournisseur, modele: &str, r: &Requete, flux: Option<&mut dyn FnMut(&str) -> bool>) -> Result<Retour, Pas> {
    let url = format!("{}/api/chat", adresse(&f.url));
    let diffuse = flux.is_some() && r.outils.is_empty();
    let mut corps = json!({
        "model": modele, "messages": r.messages, "stream": diffuse, "think": false, "keep_alive": r.garder,
        "options": {"temperature": r.temperature, "num_ctx": r.contexte, "num_predict": r.max_mots},
    });
    if !r.outils.is_empty() {
        corps["tools"] = Value::Array(r.outils.clone());
    }
    if r.json {
        corps["format"] = json!("json");
    }
    let mut rep = None;
    for _ in 0..4 {
        match post(f, &url, &corps) {
            Ok(x) => {
                rep = Some(x);
                break;
            }
            Err(ureq::Error::Status(400, x)) => {
                let t = x.into_string().unwrap_or_default().to_lowercase();
                if corps.get("tools").is_some() && t.contains("tool") {
                    if let Some(o) = corps.as_object_mut() {
                        o.remove("tools");
                    }
                    continue;
                }
                if corps.get("think").is_some() && t.contains("think") {
                    if let Some(o) = corps.as_object_mut() {
                        o.remove("think");
                    }
                    continue;
                }
                return Err(pas_ok(court(&t)));
            }
            Err(e) => return Err(pas_de_ureq(e)),
        }
    }
    let rep = rep.ok_or_else(|| pas_ok("Ollama refuse la demande"))?;
    if let (true, Some(cb)) = (diffuse, flux) {
        let mut brut = String::new();
        let mut vu = 0usize;
        let mut appels = Vec::new();
        let mut continuer = true;
        for ligne in BufReader::new(rep.into_reader()).lines() {
            let Ok(ligne) = ligne else { break };
            let Ok(v) = serde_json::from_str::<Value>(&ligne) else { continue };
            brut.push_str(v["message"]["content"].as_str().unwrap_or(""));
            lire_appels(&v["message"], &mut appels);
            let fini = v["done"].as_bool() == Some(true);
            if !diffuser(&brut, &mut vu, fini, cb) {
                continuer = false;
                break;
            }
            if fini {
                break;
            }
        }
        if continuer {
            diffuser(&brut, &mut vu, true, cb);
        }
        return Ok(Retour { contenu: crate::jarvis_link::strip_think(&brut), appels });
    }
    let v: Value = rep.into_json().map_err(|e| pas_ok(e.to_string()))?;
    let mut appels = Vec::new();
    lire_appels(&v["message"], &mut appels);
    Ok(Retour { contenu: finir(v["message"]["content"].as_str().unwrap_or(""), r.json), appels })
}

/// Un seul service : son modèle, puis l'appel au bon protocole. Renvoie aussi le modèle utilisé.
fn un(f: &Fournisseur, r: &Requete, mut flux: Option<&mut dyn FnMut(&str) -> bool>) -> Result<(Retour, String), Pas> {
    let modele = modele_de(f, r.rapide, a_des_images(r))?;
    let p = match appeler(f, &modele, r, reprise(&mut flux)) {
        Ok(retour) => return Ok((retour, modele)),
        Err(p) if mauvais_modele(&p) => p,
        Err(p) => return Err(p),
    };
    // Ce modèle n'existe pas ou ne sait pas discuter : on en essaie d'autres du même service, et on garde le bon.
    sys::log(&format!("ia : le modèle {modele} de {} ne marche pas ({}), j'en essaie un autre", f.nom, raison(&p.echec)));
    let Ok(liste) = trouver_modeles(f) else { return Err(p) };
    for m in modeles_possibles(&liste, &f.url, f.genre).into_iter().filter(|m| *m != modele).take(4) {
        if let Ok(retour) = appeler(f, &m, r, reprise(&mut flux)) {
            sys::log(&format!("ia : {} marche avec le modèle {m}", f.nom));
            retenir_modele(&f.nom, &m);
            return Ok((retour, m));
        }
    }
    Err(p)
}

/// Prête à nouveau le « direct » (phrase par phrase) pour un autre essai.
fn reprise<'b>(flux: &'b mut Option<&mut dyn FnMut(&str) -> bool>) -> Option<&'b mut dyn FnMut(&str) -> bool> {
    match flux {
        Some(f) => Some(&mut **f),
        None => None,
    }
}

fn appeler(f: &Fournisseur, modele: &str, r: &Requete, flux: Option<&mut dyn FnMut(&str) -> bool>) -> Result<Retour, Pas> {
    match f.genre {
        Genre::OpenAi => openai(f, modele, r, flux),
        Genre::Anthropic => anthropic(f, modele, r, flux),
        Genre::Ollama => ollama(f, modele, r, flux),
    }
}

/// L'erreur vient du modèle (introuvable, pas fait pour discuter…), pas du service ni de la clé.
fn mauvais_modele(p: &Pas) -> bool {
    let Echec::Autre(t) = &p.echec else { return false };
    let t = t.to_lowercase();
    t.starts_with("404")
        || (t.contains("model") && ["not found", "does not exist", "not supported", "unknown", "invalid", "not a chat"].iter().any(|m| t.contains(m)))
}

/// Retient le modèle qui marche : remplace un modèle enregistré qui ne marchait pas, sinon le garde en cache.
fn retenir_modele(nom: &str, m: &str) {
    let change = avec(|e| {
        e.modeles.insert(nom.to_string(), (Instant::now(), m.to_string()));
        match e.fournisseurs.iter_mut().find(|x| x.nom == nom && !x.modele.trim().is_empty()) {
            Some(x) => {
                x.modele = m.to_string();
                true
            }
            None => false,
        }
    });
    if change {
        enregistrer();
    }
}

/// Parle à l'IA : essaie les services allumés, dans l'ordre (le gratuit d'abord).
/// `flux` reçoit les phrases dès qu'elles sont prêtes (seulement sans outils).
pub fn discuter(r: &Requete, mut flux: Option<&mut dyn FnMut(&str) -> bool>) -> Result<Retour, Echec> {
    let mut essayes = 0usize;
    let mut derniere = Echec::Injoignable;
    for f in ordre_du_mode(liste()).iter().filter(|f| f.on) {
        if avec(|e| e.hors_ligne.get(&f.nom).is_some_and(|t| t.elapsed() < HORS_LIGNE)) {
            continue;
        }
        essayes += 1;
        // le direct (phrase par phrase) ne vaut que pour le premier service essayé : après un échec, le texte entier suffit
        match un(f, r, flux.take()) {
            Ok((retour, modele)) => {
                let nom = if modele.is_empty() { f.nom.clone() } else { format!("{} · {modele}", f.nom) };
                avec(|e| e.actif = Some(nom));
                return Ok(retour);
            }
            Err(p) => {
                if matches!(p.echec, Echec::Injoignable) {
                    avec(|e| {
                        e.hors_ligne.insert(f.nom.clone(), Instant::now());
                    });
                }
                sys::log(&format!("ia : {} n'a pas répondu ({})", f.nom, raison(&p.echec)));
                derniere = p.echec;
                if !p.suivant {
                    avec(|e| e.actif = None);
                    return Err(derniere);
                }
            }
        }
    }
    avec(|e| e.actif = None);
    Err(if essayes == 0 { Echec::Injoignable } else { derniere })
}

/// Texte d'état pour les comptes et l'île (« En service : … » ou « L'IA ne répond pas… »).
pub fn texte_etat() -> String {
    match actif() {
        Some(a) => format!("En service : {a}."),
        None => "L'IA ne répond pas : ouvre LM Studio ou ajoute une clé gratuite (Paramètres, IA).".into(),
    }
}

/// Ce que la fenêtre des paramètres affiche pour l'IA.
pub struct Vue {
    pub en_service: String,
    pub lignes: Vec<Ligne>,
}

pub struct Ligne {
    /// Position dans la liste (sert à désigner le service dans les boutons).
    pub index: u8,
    pub nom: String,
    pub detail: String,
    pub gratuit: bool,
    pub on: bool,
    pub premier: bool,
}

pub fn vue() -> Vue {
    let lignes = liste()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let ou = if f.local {
                "sur ce PC"
            } else if !f.cle.is_empty() {
                "clé enregistrée"
            } else {
                "en ligne"
            };
            let modele = if f.modele.trim().is_empty() { "modèle auto".to_string() } else { f.modele.trim().to_string() };
            Ligne { index: i as u8, nom: f.nom.clone(), detail: format!("{ou} · {modele}"), gratuit: f.gratuit, on: f.on, premier: f.choisi_en_premier }
        })
        .collect();
    Vue { en_service: texte_etat(), lignes }
}

/// État pour l'API (GET /ia/etat). Les clés ne sortent jamais : seulement « cle » : vrai ou faux.
pub fn etat_json() -> Value {
    let (liste, actif) = avec(|e| (e.fournisseurs.clone(), e.actif.clone()));
    let fournisseurs: Vec<Value> = liste
        .iter()
        .map(|f| {
            json!({
                "nom": f.nom, "genre": f.genre, "url": f.url, "modele": f.modele, "modele_rapide": f.modele_rapide,
                "gratuit": f.gratuit, "local": f.local, "on": f.on, "premier": f.choisi_en_premier, "cle": !f.cle.is_empty(),
            })
        })
        .collect();
    let p = profil();
    json!({"ok": true, "actif": actif, "fournisseurs": fournisseurs, "mode": nom_mode(), "puissant": puissant(),
           "profil": {"id": p.id, "nom": p.nom, "modele_local": p.modele_local, "max_outils": max_outils()}})
}

/// Un service que la clé peut correspondre.
struct Candidat {
    nom: String,
    genre: Genre,
    url: String,
    gratuit: bool,
}

fn candidat(nom: &str, genre: Genre, url: &str, gratuit: bool) -> Candidat {
    Candidat { nom: nom.into(), genre, url: url.into(), gratuit }
}

/// Devine les services d'une clé d'après son début. Une clé inconnue essaie plusieurs services.
fn candidats(cle: &str) -> Vec<Candidat> {
    if cle.starts_with("sk-ant-") {
        return vec![candidat("Anthropic", Genre::Anthropic, "https://api.anthropic.com/v1", false)];
    }
    if cle.starts_with("sk-or-") {
        return vec![candidat("OpenRouter", Genre::OpenAi, "https://openrouter.ai/api/v1", true)];
    }
    let prefixes: [(&str, &str, &str, bool); 9] = [
        ("gsk_", "Groq", "https://api.groq.com/openai/v1", true),
        ("AIza", "Google Gemini", "https://generativelanguage.googleapis.com/v1beta/openai", true),
        ("csk-", "Cerebras", "https://api.cerebras.ai/v1", true),
        ("hf_", "Hugging Face", "https://router.huggingface.co/v1", true),
        ("nvapi-", "NVIDIA", "https://integrate.api.nvidia.com/v1", true),
        ("xai-", "xAI (Grok)", "https://api.x.ai/v1", false),
        ("pplx-", "Perplexity", "https://api.perplexity.ai", false),
        ("fw_", "Fireworks", "https://api.fireworks.ai/inference/v1", false),
        ("tgp_", "Together", "https://api.together.xyz/v1", false),
    ];
    if let Some((_, nom, url, gratuit)) = prefixes.iter().find(|(p, ..)| cle.starts_with(p)) {
        return vec![candidat(nom, Genre::OpenAi, url, *gratuit)];
    }
    if ["sk-proj-", "sk-svcacct-", "sk-admin-"].iter().any(|p| cle.starts_with(p)) {
        return vec![candidat("OpenAI", Genre::OpenAi, "https://api.openai.com/v1", false)];
    }
    if cle.starts_with("sk-") {
        return vec![
            candidat("OpenAI", Genre::OpenAi, "https://api.openai.com/v1", false),
            candidat("DeepSeek", Genre::OpenAi, "https://api.deepseek.com/v1", false),
            candidat("Moonshot", Genre::OpenAi, "https://api.moonshot.ai/v1", false),
        ];
    }
    vec![
        candidat("Mistral", Genre::OpenAi, "https://api.mistral.ai/v1", true),
        candidat("DeepSeek", Genre::OpenAi, "https://api.deepseek.com/v1", false),
        candidat("Together", Genre::OpenAi, "https://api.together.xyz/v1", false),
        candidat("DeepInfra", Genre::OpenAi, "https://api.deepinfra.com/v1/openai", false),
        candidat("Cohere", Genre::OpenAi, "https://api.cohere.ai/compatibility/v1", false),
        candidat("OpenAI", Genre::OpenAi, "https://api.openai.com/v1", false),
        candidat("Anthropic", Genre::Anthropic, "https://api.anthropic.com/v1", false),
    ]
}

/// Le nom de domaine d'une adresse (« https://api.exemple.fr/v1 » → « api.exemple.fr »).
fn domaine(url: &str) -> String {
    let sans = url.split("://").last().unwrap_or(url);
    sans.split('/').next().unwrap_or("").to_string()
}

/// Vérifie une clé auprès d'un service. `Ok(modèle)` si elle marche, sinon le code HTTP (0 = pas de réponse).
fn tester(genre: Genre, base: &str, cle: &str) -> Result<String, u16> {
    let f = Fournisseur { genre, url: base.into(), cle: cle.into(), ..Default::default() };
    // Perplexity n'a pas de liste de modèles : on envoie une toute petite question
    if base.contains("perplexity") {
        let corps = json!({"model": "sonar", "messages": [{"role": "user", "content": "ok"}], "max_tokens": 1});
        return match post(&f, &format!("{base}/chat/completions"), &corps) {
            Ok(_) => Ok("sonar".into()),
            Err(ureq::Error::Status(c, _)) => Err(c),
            Err(_) => Err(0),
        };
    }
    match get(&f, &format!("{base}/models")) {
        Ok(r) => {
            let v: Value = r.into_json().unwrap_or(json!({}));
            Ok(choisir_dans(&noms_modeles(&v), base, genre).unwrap_or_default())
        }
        Err(ureq::Error::Status(c, _)) => Err(c),
        Err(_) => Err(0),
    }
}

/// Ajoute un service à partir d'une clé d'API (appelée dans un thread : elle attend le réseau).
/// `adresse` : pour une clé d'un service inconnu. Renvoie le message à afficher.
pub fn ajouter_cle(cle: &str, adresse: &str) -> Result<String, String> {
    let cle: String = cle.split_whitespace().collect::<String>().trim_matches(|c: char| c == '"' || c == '\'').to_string();
    if cle.is_empty() {
        return Err("Colle une clé d'API dans la case.".into());
    }
    if avec(|e| e.fournisseurs.iter().any(|f| f.cle == cle)) {
        return Ok("Cette clé est déjà enregistrée.".into());
    }
    let adresse = adresse.trim().trim_matches(|c: char| c == '"' || c == '\'').trim_end_matches('/').to_string();
    let perso = !adresse.is_empty();
    let cands = if perso {
        let genre = if adresse.to_lowercase().contains("anthropic") { Genre::Anthropic } else { Genre::OpenAi };
        vec![candidat(&domaine(&adresse), genre, &adresse, false)]
    } else {
        candidats(&cle)
    };
    let mut refusee = false;
    for c in &cands {
        let mut bases = vec![c.url.clone()];
        if perso && !c.url.ends_with("/v1") {
            bases.push(format!("{}/v1", c.url));
        }
        for b in bases {
            match tester(c.genre, &b, &cle) {
                Ok(modele) => {
                    let f = Fournisseur {
                        nom: c.nom.clone(),
                        genre: c.genre,
                        url: b.clone(),
                        cle: cle.clone(),
                        modele: modele.clone(),
                        modele_rapide: String::new(),
                        gratuit: c.gratuit,
                        local: false,
                        on: true,
                        choisi_en_premier: false,
                    };
                    avec(|e| {
                        e.fournisseurs.push(f);
                        trier(&mut e.fournisseurs);
                    });
                    enregistrer();
                    let m = if modele.is_empty() { "choisi automatiquement".to_string() } else { modele };
                    return Ok(format!("{} ajouté : modèle {m}, {}.", c.nom, if c.gratuit { "gratuit" } else { "payant" }));
                }
                Err(401) | Err(403) => {
                    refusee = true;
                    break;
                }
                Err(_) => continue,
            }
        }
    }
    if refusee {
        Err("Cette clé est refusée : vérifie-la.".into())
    } else {
        Err("Je ne reconnais pas cette clé. Pour un service inconnu, indique son adresse (ex. https://…/v1) dans « Adresse du service ».".into())
    }
}

/// Cherche les IA locales qui tournent sur ce PC et ajoute celles qui manquent. Renvoie une phrase.
pub fn chercher_locaux() -> String {
    let sondes: [(&str, &str, Genre, &str); 6] = [
        ("LM Studio", "http://127.0.0.1:1234/v1", Genre::OpenAi, "/models"),
        ("Jan", "http://127.0.0.1:1337/v1", Genre::OpenAi, "/models"),
        ("llama.cpp ou LocalAI", "http://127.0.0.1:8080/v1", Genre::OpenAi, "/models"),
        ("KoboldCpp", "http://127.0.0.1:5001/v1", Genre::OpenAi, "/models"),
        ("GPT4All", "http://127.0.0.1:4891/v1", Genre::OpenAi, "/models"),
        ("Ollama", "http://127.0.0.1:11434", Genre::Ollama, "/api/tags"),
    ];
    let mut trouves = Vec::new();
    for (nom, url, genre, chemin) in sondes {
        let test = format!("{url}{chemin}");
        let ouvert = ureq::AgentBuilder::new().timeout(Duration::from_millis(1500)).build().get(&test).call().is_ok();
        if !ouvert {
            continue;
        }
        trouves.push(nom.to_string());
        let deja = avec(|e| e.fournisseurs.iter().any(|f| adresse(&f.url) == url));
        if !deja {
            avec(|e| {
                e.fournisseurs.push(Fournisseur {
                    nom: nom.into(),
                    genre,
                    url: url.into(),
                    gratuit: true,
                    local: true,
                    on: true,
                    ..Default::default()
                })
            });
        }
    }
    avec(|e| trier(&mut e.fournisseurs));
    enregistrer();
    if trouves.is_empty() {
        "Rien trouvé sur ce PC. Ouvre LM Studio (ou une autre IA locale), puis relance la recherche.".into()
    } else {
        format!("Trouvé sur ce PC : {}.", trouves.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(nom: &str, gratuit: bool, local: bool) -> Fournisseur {
        Fournisseur { nom: nom.into(), gratuit, local, on: true, ..Default::default() }
    }

    #[test]
    fn devine_le_service_d_une_cle() {
        // des faux exemples : aucune vraie clé dans le code
        assert_eq!(candidats("sk-ant-EXEMPLE")[0].nom, "Anthropic");
        assert_eq!(candidats("sk-or-EXEMPLE")[0].nom, "OpenRouter");
        assert!(candidats("sk-or-EXEMPLE")[0].gratuit);
        assert_eq!(candidats("gsk_EXEMPLE")[0].nom, "Groq");
        assert_eq!(candidats("AIzaEXEMPLE")[0].nom, "Google Gemini");
        assert_eq!(candidats("sk-proj-EXEMPLE")[0].nom, "OpenAI");
        assert_eq!(candidats("pplx-EXEMPLE")[0].nom, "Perplexity");
        // « sk- » sans préfixe connu : OpenAI d'abord, puis DeepSeek, puis Moonshot
        let c = candidats("sk-EXEMPLE");
        assert_eq!(c.iter().map(|x| x.nom.as_str()).collect::<Vec<_>>(), ["OpenAI", "DeepSeek", "Moonshot"]);
        // clé inconnue : Mistral (gratuit) en premier
        let c = candidats("EXEMPLE");
        assert_eq!(c[0].nom, "Mistral");
        assert!(c[0].gratuit && c.last().is_some_and(|x| x.nom == "Anthropic"));
    }

    #[test]
    fn gratuit_d_abord_et_choix_de_l_utilisateur() {
        let mut v = vec![service("payant", false, false), service("cle gratuite", true, false), service("local", true, true)];
        trier(&mut v);
        assert_eq!(v.iter().map(|f| f.nom.as_str()).collect::<Vec<_>>(), ["local", "cle gratuite", "payant"]);
        // l'utilisateur met le payant en premier : il reste en tête, même après un nouveau tri
        placer_en_premier(&mut v, 2);
        assert_eq!(v[0].nom, "payant");
        v.push(service("nouveau local", true, true));
        trier(&mut v);
        assert_eq!(v[0].nom, "payant");
        assert_eq!(v[1].nom, "local");
    }

    #[test]
    fn messages_vers_openai() {
        let messages = vec![
            json!({"role": "system", "content": "Consigne"}),
            json!({"role": "user", "content": "Lance Chrome"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"function": {"name": "lancer", "arguments": {"app": "chrome"}}},
                {"function": {"name": "monter", "arguments": {"n": 2}}}
            ]}),
            json!({"role": "tool", "tool_name": "lancer", "content": "Chrome lancé."}),
            json!({"role": "tool", "tool_name": "monter", "content": "Son monté."}),
            json!({"role": "user", "content": "Et ça ?", "images": ["/9j/AAA"]}),
        ];
        let s = vers_openai(&messages);
        assert_eq!(s[2]["content"], Value::Null);
        assert_eq!(s[2]["tool_calls"][0]["id"], "appel_1");
        assert_eq!(s[2]["tool_calls"][1]["id"], "appel_2");
        // les arguments sont du texte JSON
        assert_eq!(s[2]["tool_calls"][0]["function"]["arguments"], "{\"app\":\"chrome\"}");
        assert_eq!(s[3]["tool_call_id"], "appel_1");
        assert_eq!(s[4]["tool_call_id"], "appel_2");
        assert!(s[3].get("tool_name").is_none());
        // l'image part en URL de données JPEG
        assert_eq!(s[5]["content"][1]["image_url"]["url"], "data:image/jpeg;base64,/9j/AAA");
    }

    #[test]
    fn messages_vers_anthropic() {
        let messages = vec![
            json!({"role": "system", "content": "Consigne 1"}),
            json!({"role": "system", "content": "Consigne 2"}),
            json!({"role": "user", "content": "Lance Chrome et monte le son"}),
            json!({"role": "assistant", "content": "", "tool_calls": [
                {"function": {"name": "lancer", "arguments": {"app": "chrome"}}},
                {"function": {"name": "monter", "arguments": "{\"n\":2}"}}
            ]}),
            json!({"role": "tool", "tool_name": "lancer", "content": "Chrome lancé."}),
            json!({"role": "tool", "tool_name": "monter", "content": "Son monté."}),
            json!({"role": "system", "content": "Rappel"}),
        ];
        let (systeme, s) = vers_anthropic(&messages);
        assert_eq!(systeme.as_deref(), Some("Consigne 1\n\nConsigne 2"));
        assert_eq!(s[1]["content"][0]["type"], "tool_use");
        assert_eq!(s[1]["content"][1]["input"]["n"], 2);
        // les deux résultats forment UN seul message utilisateur
        assert_eq!(s[2]["role"], "user");
        assert_eq!(s[2]["content"].as_array().map(|a| a.len()), Some(2));
        assert_eq!(s[2]["content"][0]["tool_use_id"], "toolu_1");
        assert_eq!(s[2]["content"][1]["tool_use_id"], "toolu_2");
        // un système au milieu devient un message de consigne
        assert_eq!(s[3]["content"], "[Consigne] Rappel");
    }

    #[test]
    fn choix_du_modele() {
        let noms: Vec<String> = ["text-embedding-3", "gpt-4o", "gpt-4o-mini", "whisper-1"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choisir_dans(&noms, "https://api.openai.com/v1", Genre::OpenAi).as_deref(), Some("gpt-4o-mini"));
        let noms: Vec<String> = ["a/modele", "b:free", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choisir_dans(&noms, "https://openrouter.ai/api/v1", Genre::OpenAi).as_deref(), Some("b:free"));
        // NVIDIA : jamais un modèle de code ou d'embedding, un modèle rapide fait pour discuter d'abord
        let nv: Vec<String> = ["bigcode/starcoder2-15b", "google/gemma-3-12b-it", "nvidia/embed-qa-4", "nvidia/nemotron-3.5-lightning-30b-a3b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let c = modeles_possibles(&nv, "https://integrate.api.nvidia.com/v1", Genre::OpenAi);
        assert_eq!(c, vec!["nvidia/nemotron-3.5-lightning-30b-a3b".to_string(), "google/gemma-3-12b-it".to_string()]);
        assert!(mauvais_modele(&Pas { suivant: true, echec: Echec::Autre("404 404 page not found".into()) }));
        assert!(!mauvais_modele(&Pas { suivant: true, echec: Echec::Autre("401 unauthorized".into()) }));
        let noms: Vec<String> = ["models/gemini-2.5-flash-lite", "models/gemini-2.5-flash"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choisir_dans(&noms, "https://generativelanguage.googleapis.com/v1beta/openai", Genre::OpenAi).as_deref(), Some("gemini-2.5-flash"));
        // sans préférence : le premier qui n'est pas un modèle d'embedding
        let noms: Vec<String> = ["bge-embed", "qwen3-4b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(choisir_dans(&noms, "http://127.0.0.1:1234/v1", Genre::OpenAi).as_deref(), Some("qwen3-4b"));
        assert_eq!(choisir_dans(&[], "http://x", Genre::OpenAi), None);
    }

    #[test]
    fn nettoyage_du_json() {
        assert_eq!(sans_balises("```json\n{\"a\": 1}\n```"), "{\"a\": 1}");
        assert_eq!(sans_balises("{\"a\": 1}"), "{\"a\": 1}");
        assert_eq!(sans_balises("Voici :\n```\n[1, 2]\n```"), "[1, 2]");
    }

    #[test]
    fn adresses_normalisees() {
        assert_eq!(adresse_normale("localhost:11434/ "), "http://localhost:11434");
        assert_eq!(adresse_normale("https://x.fr/v1/"), "https://x.fr/v1");
        assert_eq!(adresse_normale("  "), "");
        assert_eq!(adresse_normale("build.nvidia.com"), "https://integrate.api.nvidia.com/v1");
        assert_eq!(adresse_normale("https://build.nvidia.com/"), "https://integrate.api.nvidia.com/v1");
        assert_eq!(domaine("https://api.exemple.fr/v1"), "api.exemple.fr");
    }

    #[test]
    fn premier_service_est_la_liste_par_defaut() {
        let d = defaut();
        assert_eq!(d[0].nom, "LM Studio");
        assert_eq!(d[1].nom, "Ollama");
        assert!(d.iter().all(|f| f.gratuit && f.local && f.on));
    }
}
