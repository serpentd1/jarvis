//! Le cerveau de Jarvis, en Rust : un seul pour la voix, le chat de l'île et le téléphone.
//!
//! 1. Réponse à un « tu confirmes ? » en attente (oui / non).
//! 2. Raccourcis instantanés, sans IA (ceux des modules Python tant qu'ils existent, sinon ceux de pouvoirs.rs).
//! 3. Sinon l'IA (ia.rs : LM Studio, Ollama, ou une clé d'API) : elle choisit des outils, on les exécute VRAIMENT, et on dit ce qui a été fait.
//!    - seuls les outils utiles à la question lui sont proposés (plus rapide, moins d'erreurs) ;
//!    - quand il n'y a pas d'action à faire, la réponse arrive phrase par phrase (Jarvis parle plus tôt) ;
//!    - elle n'a pas le droit de dire « c'est fait » sans avoir appelé d'outil.
//!
//! Tant que la voix est encore en Python, ses modules restent le « registre » des outils : on les lit
//! et on les appelle par l'API locale de la voix (127.0.0.1:47801/cerveau/…). Sans la voix, le cerveau
//! marche seul avec les outils de la barre (pouvoirs sur le PC, téléphone, minuteurs, alarme…).

use std::collections::VecDeque;
use std::sync::mpsc::channel;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{Datelike, Local};
use regex::Regex;
use serde_json::{Value, json};

use crate::api::ApiReq;
use crate::data::{Msg, Out};
use crate::ia::{self, Echec, Requete};
use crate::jarvis_link;
use crate::pouvoirs::{self, Sortie};

/// Nombre maximum d'outils proposés à l'IA pour une question.
/// Une question « tu confirmes ? » reste valable ce temps-là.
const ATTENTE_VALIDE: Duration = Duration::from_secs(180);

// ───────────────────────── réglages et état ─────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct Reglages {
    /// Petit modèle des ordres : sert seulement au service Ollama (voir ia.rs).
    pub modele_rapide: String,
    pub temperature: f32,
    pub contexte: u32,
    pub garder: String,
    pub nom: String,
    /// Nombre d'échanges gardés en mémoire.
    pub historique: usize,
    /// Le prompt système écrit par l'utilisateur (fichier prompt-systeme.txt) : vide = celui de base.
    pub consigne: String,
    /// Le modèle à utiliser chez NVIDIA (config.toml, [conversation] modele) : vide = choisi tout seul.
    pub modele_api: String,
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages { modele_rapide: String::new(), temperature: 0.4, contexte: 8192, garder: "30m".into(), nom: "Jarvis".into(), historique: 3, consigne: String::new(), modele_api: String::new() }
    }
}

impl Reglages {
    /// Réglages envoyés par la voix (section [cerveau] de config.toml). Les anciennes clés « ollama_url » et « modele »
    /// ne choisissent plus le service : c'est ia.bin (fenêtre Paramètres, IA) qui décide.
    pub fn depuis_json(v: &Value) -> Option<Reglages> {
        let d = Reglages::default();
        let s = |k: &str, def: &str| v[k].as_str().map(str::trim).filter(|x| !x.is_empty()).unwrap_or(def).to_string();
        v.is_object().then(|| Reglages {
            modele_rapide: v["modele_rapide"].as_str().unwrap_or("").trim().to_string(),
            temperature: v["temperature"].as_f64().unwrap_or(d.temperature as f64) as f32,
            contexte: v["contexte"].as_u64().unwrap_or(d.contexte as u64) as u32,
            garder: s("garder_en_memoire", &d.garder),
            nom: s("nom", &d.nom),
            historique: v["historique"].as_u64().unwrap_or(d.historique as u64) as usize,
            consigne: v["consigne"].as_str().unwrap_or("").trim().to_string(),
            modele_api: v["modele_api"].as_str().unwrap_or("").trim().to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Cote {
    /// Question posée par un outil des modules Python (numéro donné par la voix).
    Python(u64),
    /// Question posée par un pouvoir de la barre (pouvoirs.rs).
    Rust(u64),
}

struct Attente {
    cote: Cote,
    annuler: String,
    quand: Instant,
}

struct Etat {
    reg: Reglages,
    /// Réglages de la voix (config.toml), reçus à chaque question venue de la voix.
    reg_voix: Option<Reglages>,
    /// Les messages de chaque échange (question, appels d'outils, résultats, réponse).
    histo: VecDeque<Vec<Value>>,
    attente: Option<Attente>,
    out: Option<Out>,
    cache_outils: Option<(Instant, Vec<Value>)>,
    /// Outils utilisés au dernier échange (« et monte encore un peu » doit les retrouver).
    derniers: Vec<String>,
    /// Échanges faits sans outil depuis `derniers` (« exécute-la » juste après reste une suite d'action).
    sans_outil: usize,
}

static ETAT: Mutex<Option<Etat>> = Mutex::new(None);

fn avec<R>(f: impl FnOnce(&mut Etat) -> R) -> R {
    let mut g = ETAT.lock().unwrap_or_else(|e| e.into_inner());
    let e = g.get_or_insert_with(|| Etat {
        reg: Reglages::default(),
        reg_voix: None,
        histo: VecDeque::new(),
        attente: None,
        out: None,
        cache_outils: None,
        derniers: Vec::new(),
        sans_outil: 0,
    });
    f(e)
}

/// À appeler au démarrage de la barre : le cerveau peut alors lancer minuteurs, alarme, chrono…
pub fn init(out: Out) {
    avec(|e| e.out = Some(out));
}

/// Réglages de l'île (utilisés quand la voix ne tourne pas).
pub fn regler(reg: Reglages) {
    avec(|e| e.reg = reg);
}

/// « Oublie tout » : nouvelle conversation.
pub fn oublier() {
    avec(|e| {
        e.histo.clear();
        e.attente = None;
        e.derniers.clear();
        e.sans_outil = 0;
    });
}

// ───────────────────────── texte ─────────────────────────

fn re(cell: &'static OnceLock<Regex>, motif: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(motif).expect("motif du cerveau invalide"))
}

/// « Lance-moi Discord ! » → « lance moi discord »
pub fn normaliser(texte: &str) -> String {
    let t = pouvoirs::sans_accents(&texte.to_lowercase()).replace(['’', '`'], "'").replace('œ', "oe");
    let t: String = t.chars().map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '\'' { c } else { ' ' }).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn enlever_mot_activation(texte: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"(?i)^\s*(?:(?:hey|hé|he|eh|et|ok|okay|dis|bonjour|salut|yo)[\s,]*)?(?:jarvis|jarvi|jarvy|djarvis|charvis|jervis|jarvisse)\b[\s,.!?:;-]*")
        .replace(texte, "")
        .trim()
        .to_string()
}

fn est_oui(n: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    static ORDRE: OnceLock<Regex> = OnceLock::new();
    // « Exécute-la », « Lance-le » : un oui, mais seulement si la phrase s'arrête là (« lance le minuteur » est un autre ordre)
    if re(&ORDRE, r"^(?:execute|lance|envoie|fais)(?: la| le| les| ca)?(?: stp| s'il te plait)?$").is_match(n) {
        return true;
    }
    re(&R, r"^(?:(?:tu as|t'as|je te donne) (?:mon accord|mon autorisation|l'autorisation)|je t'autorise|j'autorise|autorise|j'accepte|accepte|affirmatif)\b").is_match(n)
        || est_oui_simple(n)
}

fn est_oui_simple(n: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^(oui|ouais|ouai|wesh|vas y|ok|okay|d'accord|daccord|envoie|envoie le|envoie la|confirme|je confirme|c'est bon|go|let's go|yes|yep|carrement|bien sur|exactement|exact|parfait|ca marche|fais le|fais la|tres bien|bien|super|nickel|top|cool|genial|valide|c'est ca|c'est parti|absolument|evidemment|grave|bah oui|ben oui)\b").is_match(n)
}

fn est_non(n: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^(non|nan|nope|annule|laisse tomber|oublie|stop|pas maintenant|attends|surtout pas|n'envoie pas|non merci|pas du tout|arrete|change)\b").is_match(n)
}

fn est_oubli(n: &str) -> bool {
    matches!(n, "oublie tout" | "nouvelle conversation" | "on change de sujet" | "efface la memoire" | "efface ta memoire" | "reset")
}

/// L'IA dit avoir agi… sans avoir appelé d'outil : on ne la laisse pas mentir. `texte` : sa réponse telle quelle.
/// `strict` : l'utilisateur a demandé une action, donc « c'est supprimé », « le fichier a été créé »… comptent aussi.
/// Mais une phrase qui DÉCRIT un état n'est pas un mensonge : « je peux regarder quelles fenêtres sont ouvertes »,
/// « je ne vois pas quelles applis sont installées », « Chrome est déjà lancé ? ». Les tournures « est / sont … »
/// ne comptent donc que dans une phrase affirmative, sans négation, question ni hypothèse.
fn pretend_agir(texte: &str, strict: bool) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    static S: OnceLock<Regex> = OnceLock::new();
    static P: OnceLock<Regex> = OnceLock::new();
    static DOUTE: OnceLock<Regex> = OnceLock::new();
    const FAIT: &str = "supprime|enregistre|cree|ferme|efface|deplace|copie|renomme|modifie|note|ajoute|active|desactive|coupe|eteint|allume|installe|annule|arrete|envoye|lance|ouvert|regle|programme|demarre|mis";
    let n = normaliser(texte);
    if re(&R, r"\b(j'ai (bien )?(envoye|lance|ouvert|ferme|mis|regle|active|desactive|coupe|change|cree|programme|supprime|annule|demarre|transmis)|c'est (bien )?(envoye|fait|lance|ouvert|regle|parti|programme)|message (bien )?envoye|je (l')?envoie|j'envoie|j'ouvre|je lance|je ferme|je mets|je regle|je coupe|minuteur (lance|regle|cree))\b").is_match(&n) {
        return true;
    }
    if !strict {
        return false;
    }
    if re(&S, &format!(r"\b(c'est (bien |maintenant )?({FAIT})|j'ai (bien )?({FAIT})|je (l'|le |la |les )?(supprime|cree|efface|deplace|copie|renomme|modifie|eteins|allume|arrete|annule))e?s?\b")).is_match(&n) {
        return true;
    }
    let passif = re(&P, &format!(r"\b(est|sont|a ete|ont ete) (bien |maintenant )?({FAIT})e?s?\b"));
    let doute = re(&DOUTE, r"\b(ne|n'\w+|pas|jamais|aucun|aucune|si|s'il|peux|peut|pourrai|pourrais|pourrait|veux|voudrais|quel|quelle|quels|quelles|lequel|laquelle|est ce|deja|voir|savoir|verifier|regarder|regarde)\b");
    texte.split_inclusive(['.', '!', '?', '…', ';', '\n']).any(|phrase| {
        let p = normaliser(phrase);
        !phrase.trim_end().ends_with('?') && passif.is_match(&p) && !doute.is_match(&p)
    })
}

/// L'IA dit qu'elle ne sait pas faire : peut-être que le bon outil ne lui a pas été proposé.
/// L'IA a ÉCRIT un appel d'outil au lieu de l'appeler (« ouvrir_application Chrome », « <tool_call> ») :
/// ce texte ne doit jamais être lu à voix haute, et l'action n'est pas faite.
fn ecrit_un_outil(texte: &str, connus: &[String]) -> bool {
    let bas = texte.to_lowercase();
    bas.contains("tool_call") || bas.contains("\"arguments\"") || connus.iter().any(|nom| nom.contains('_') && bas.contains(nom.as_str()))
}

fn dit_ne_pas_savoir(n: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"\b(ne sais pas (encore )?(le |la |les )?faire|sais pas encore|ne peux pas (le |la |les )?faire|je ne peux pas|pas (encore )?capable|aucun outil|pas d'outil|n'ai pas (d'|l')outil)\b").is_match(n)
}

fn plusieurs_ordres(n: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"\b(?:et|puis|ensuite|apres)\s+(?:(?:me|moi|le|la|les|un|une|aussi)\s+)?(?:mets?|lance|ouvre|ferme|baisse|monte|coupe|cherche|reveille|dis|reponds?|annule|arrete|joue|regle|active|demarre|envoie|lis|passe)\b").is_match(n)
}

/// Outils Internet (lents : une phrase d'attente est dite pendant qu'ils tournent).
const OUTILS_INTERNET: [&str; 4] = ["web_search", "read_page", "lire_onglet", "capture_onglet"];
const PHRASE_ATTENTE: &str = "Je regarde ça.";

/// Question dont la réponse a pu changer (actualité, résultats, prix…) ou qui parle d'une page web :
/// les outils Internet sont proposés à l'IA, même sans verbe d'action.
fn besoin_d_internet(n: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"\b(internet|web|en ligne|google|cherch|recherch|derni|actuel|en ce moment|aujourd'hui|ce soir|hier|cette (annee|semaine|saison)|recemment|recent|actu|news|nouveaute|resultat|score|classement|qui a gagne|a gagne|vainqueur|champion|match|prix (d|du|de)|combien (coute|vaut)|sorti|bourse|election|president|onglet|chrome|page|site|article|lien|capture|resume|20[2-3][0-9])").is_match(n)
}

/// Sans verbe d'action, c'est une question : l'IA répond avec les données, sans outils.
fn est_action(n: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"\b(execut|fais (?:le|la|les|ca)|fait (?:le|la|les|ca)|vas y|va sur|va dans|va voir|aller sur|rends toi|utilise|autoris|essaie|essaye|reessai|refais|recommence|lance|lancer|ouvr|ferm|quitt|mets|met|mettre|baiss|monte|monter|augment|diminu|coupe|couper|remet|reactiv|envoi|repond|ecri|cherch|recherch|trouve|avance|recule|pause|joue|jouer|passe a|suivant|precedent|reveill|alarme|minuteur|chrono|timer|annul|arret|stop|demarr|active|desactiv|allum|etein|verrouill|actualis|rafraich|lis|lire|volume|ecout|affich|montre|regle|dis a|dis lui|plus fort|moins fort|clique|clic|tape|appuie|presse|saisis|defile|descend|remonte|scroll|regarde|ecran|analyse|decris|voix|installe|ajoute|rajoute|note (?:que|ca|le|la|les|dans|bien|moi)|prends note|retiens|souviens|rappelle|coche|termine|j'ai fait|j'ai fini|cree|oublie|routine|habitude|verrouille|batterie|supprim|efface|renomm|deplac|copie|modifi|corrige|fichier|dossier|page web|site web|powershell|commande|redemarr|veille|tue|processus|sonner|sonne|appelle|photo|lampe|silencieux|vibreur|redui|agrandi|minimis|maximis|fenetre|restaur|plein ecran|premier plan|en petit|en grand|modele|mode leger|mode performant|ne vais pas|n y vais pas|j y vais|y vais finalement|pas cours|pas en cours|emploi du temps|pai)").is_match(n)
}

fn nettoyer(texte: &str) -> String {
    jarvis_link::strip_think(texte)
}

/// Ce qu'on peut déjà montrer d'une réponse en cours : rien de ce qui est dans <think>…</think>.
pub fn visible(texte: &str) -> String {
    let mut t = texte.to_string();
    loop {
        match (t.find("<think>"), t.find("</think>")) {
            (Some(a), Some(b)) if b > a => t.replace_range(a..b + "</think>".len(), ""),
            (Some(a), _) => {
                t.truncate(a);
                break;
            }
            _ => break,
        }
    }
    t
}

/// Phrases terminées depuis l'octet `depuis` (les très courtes sont regroupées jusqu'à 40 caractères ;
/// la toute première part dès 20 caractères, pour que Jarvis commence à parler plus tôt).
/// Renvoie les phrases et le nouvel octet de départ. `fin` : la réponse est complète, on donne le reste.
pub fn phrases_pretes(texte: &str, depuis: usize, fin: bool) -> (Vec<String>, usize) {
    let (mut sorties, mut debut) = (Vec::new(), depuis.min(texte.len()));
    let reste = &texte[debut..];
    let mut precedent: Option<char> = None;
    let base = debut;
    for (i, c) in reste.char_indices() {
        if c.is_whitespace() && precedent.is_some_and(|p| matches!(p, '.' | '!' | '?' | '…')) {
            let morceau = &texte[debut..base + i];
            let minimum = if debut == 0 { 20 } else { 40 };
            if morceau.trim().chars().count() >= minimum {
                sorties.push(morceau.trim().to_string());
                debut = base + i;
            }
        }
        precedent = Some(c);
    }
    if fin {
        let morceau = texte[debut..].trim();
        if !morceau.is_empty() {
            sorties.push(morceau.to_string());
        }
        debut = texte.len();
    }
    (sorties, debut)
}

const JOURS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const MOIS: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];

fn consigne(nom: &str, perso: &str, quelle_ia: &str) -> String {
    let m = Local::now();
    let lieu = if quelle_ia.contains("LM Studio") || quelle_ia.contains("Ollama") { "en local, sur ce PC" } else { "dans le cloud (sur Internet)" };
    let ia = if quelle_ia.is_empty() {
        String::new()
    } else {
        format!("Tu fonctionnes avec l'IA « {quelle_ia} », {lieu}. Si on te demande quel modèle ou quelle IA tu es, dis-le simplement.\n")
    };
    // Le prompt de l'utilisateur (prompt-systeme.txt) remplace la présentation et la longueur des réponses.
    let presentation = if perso.trim().is_empty() {
        format!(
            "Tu es {nom}, l'assistant vocal d'un collégien français, sur son PC Windows. Tu es efficace, poli, \
avec une pointe d'humour à la Jarvis d'Iron Man.\n- Réponds en français, en TUTOYANT l'utilisateur, en une ou deux \
phrases courtes et naturelles : elles sont LUES À VOIX HAUTE."
        )
    } else {
        perso.replace("{nom}", nom).trim().to_string()
    };
    format!(
        "{presentation}\n{ia}\
Nous sommes le {} {} {} {}, il est {}.\n\
RÈGLES :\n\
- Tout ce que tu écris est lu à voix haute : pas de markdown, pas de listes, pas d'émojis, pas de liens.\n\
- Tu ne sais pas encore envoyer de message sur Pronote ni de mail : si on te le demande, dis-le.\n\
- N'INVENTE RIEN. Pour les devoirs, les notes, les cours, les mails et les messages, ne cite que ce qui est écrit \
mot pour mot dans les DONNÉES : garde chaque devoir avec SA matière, sans mélanger deux matières. Si tu n'es pas \
sûr ou si ce n'est pas dans les données, dis « je ne sais pas » ou « ce n'est pas dans ce que je vois ».\n\
- QUESTION sur la vie de l'utilisateur (météo, devoirs, notes, cours, mails, minuteurs, Discord, \
téléphone…) : réponds directement avec les DONNÉES ci-dessous, SANS appeler d'outil. Si l'information \
n'y est pas, dis-le au lieu d'inventer. « Et demain ? » continue la question précédente.\n\
- DISCUSSION ou CULTURE GÉNÉRALE (blague, histoire, devinette, conseil, explication, question de cours…) : \
réponds toi-même avec tes connaissances, volontiers et sans outil.\n\
- ACTUALITÉ ou info qui a pu changer (résultats sportifs, « le dernier… », prix, sorties, personnes en poste…) : \
si l'outil web_search t'est proposé, appelle-le au lieu de répondre de mémoire, car ta mémoire peut être dépassée. \
Une page ouverte dans Chrome : lire_onglet pour son texte, capture_onglet pour la voir.\n\
- ACTION demandée clairement (ouvrir ou fermer une appli, un site, minuteur, alarme, chrono, vidéo, \
musique, volume, fichier, téléphone, envoyer un message Discord, ouvrir Pronote ou un mail…) : appelle \
l'outil adapté, avec des paramètres précis tirés de la demande. N'affirme JAMAIS avoir fait une action \
sans avoir appelé l'outil.\n\
- Plusieurs actions demandées d'un coup : appelle un outil par action, dans l'ordre.\n\
- Si aucun outil ne permet de faire l'action, dis en une phrase « je ne sais pas encore faire ça, je l'ai noté \
pour l'apprendre », puis propose UNE chose proche que l'un de tes outils permet vraiment (ouvrir le site ou l'appli \
concernée, chercher sur Internet, mettre un rappel…). Ne propose JAMAIS une action qu'aucun de tes outils ne \
permet. « Qu'est-ce que tu sais faire ? » : appelle ce_que_je_sais_faire s'il t'est proposé.\n",
        JOURS[m.weekday().num_days_from_monday() as usize],
        m.day(),
        MOIS[m.month0() as usize],
        m.year(),
        m.format("%H:%M"),
    )
}

// ───────────────────────── les outils ─────────────────────────

fn nom_outil(o: &Value) -> &str {
    o["function"]["name"].as_str().unwrap_or("")
}

/// Outils que la barre sait faire seule, sans la voix (minuteurs, alarme, chrono, Pronote, mails, météo).
fn outils_barre() -> Vec<Value> {
    let o = |nom: &str, desc: &str, params: &[(&str, &str, bool)]| {
        let props: serde_json::Map<String, Value> = params.iter().map(|(n, d, _)| (n.to_string(), json!({"type": "string", "description": d}))).collect();
        let req: Vec<&str> = params.iter().filter(|p| p.2).map(|p| p.0).collect();
        json!({"type": "function", "function": {"name": nom, "description": desc, "parameters": {"type": "object", "properties": props, "required": req}}})
    };
    vec![
        o("lancer_minuteur", "Lance un minuteur sur le PC (dans l'île).", &[("duree", "durée, ex. 5 minutes, 1 h 30", true), ("nom", "nom du minuteur", false)]),
        o("regler_alarme", "Règle et active l'alarme du PC.", &[("heure", "heure, ex. 07:30", true)]),
        o("chronometre", "Démarre, met en pause ou remet à zéro le chronomètre.", &[("commande", "demarrer, pause ou zero", true)]),
        o("ouvrir_page", "Ouvre Pronote ou la boîte mail dans le navigateur.", &[("quoi", "pronote ou mails", true)]),
        o("actualiser_meteo", "Actualise la météo affichée.", &[]),
    ]
}

/// Tous les outils disponibles : ceux de la voix (qui contiennent déjà ceux de la barre) si elle tourne,
/// sinon ceux de la barre seule.
fn tous_les_outils(python: bool) -> Vec<Value> {
    if python {
        if let Some(v) = avec(|e| e.cache_outils.as_ref().filter(|(t, _)| t.elapsed() < Duration::from_secs(45)).map(|(_, v)| v.clone())) {
            return v;
        }
        if let Ok(r) = jarvis_link::post("/cerveau/outils", json!({}), 5) {
            let v: Vec<Value> = r["outils"].as_array().cloned().unwrap_or_default();
            if !v.is_empty() {
                avec(|e| e.cache_outils = Some((Instant::now(), v.clone())));
                return v;
            }
        }
    }
    let mut v = outils_barre();
    v.extend(pouvoirs::outils().as_array().cloned().unwrap_or_default());
    v
}

const MOTS_VIDES: [&str; 44] = [
    "les", "des", "une", "mon", "mes", "ton", "tes", "son", "ses", "que", "qui", "quoi", "est", "pour", "sur", "dans", "avec", "moi", "toi",
    "jarvis", "stp", "plait", "peux", "veux", "fais", "faire", "fait", "aux", "par", "pas", "plus", "the", "ça", "cela", "cette", "ces", "tout",
    "merci", "alors", "bien", "juste", "aussi", "vous", "nous",
];

fn racine(mot: &str) -> &str {
    let fin = mot.char_indices().nth(5).map(|(i, _)| i).unwrap_or(mot.len());
    &mot[..fin]
}

/// Garde les outils qui ont un rapport avec la question (au plus MAX_OUTILS). Rien ne ressemble : on les
/// donne tous, pour ne jamais priver l'IA du bon outil.
pub fn choisir(tous: &[Value], n: &str, garder: &[String]) -> Vec<Value> {
    // mode ultra léger : sans les actions lourdes, et moins d'outils (voir ia::max_outils)
    let permis: Vec<Value> = tous.iter().filter(|o| crate::ia::outil_permis(nom_outil(o))).cloned().collect();
    let tous = &permis[..];
    let max = crate::ia::max_outils();
    if tous.len() <= max {
        return tous.to_vec();
    }
    let mots: Vec<&str> = n.split(' ').filter(|m| m.chars().count() >= 3 && !MOTS_VIDES.contains(m)).map(racine).collect();
    let mut notes: Vec<(i32, usize)> = tous
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let nom = normaliser(&nom_outil(o).replace('_', " "));
            let desc = normaliser(o["function"]["description"].as_str().unwrap_or(""));
            let params = normaliser(&o["function"]["parameters"]["properties"].to_string());
            let dans = |texte: &str, r: &str| texte.split(' ').any(|m| m.starts_with(r));
            let mut note = 0;
            for r in &mots {
                if dans(&nom, r) {
                    note += 3;
                } else if dans(&desc, r) {
                    note += 2;
                } else if dans(&params, r) {
                    note += 1;
                }
            }
            if garder.iter().any(|g| g == nom_outil(o)) {
                note += 2;
            }
            (note, i)
        })
        .collect();
    if notes.iter().all(|(note, _)| *note == 0) {
        return tous.to_vec();
    }
    notes.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    notes.into_iter().filter(|(note, _)| *note > 0).take(max).map(|(_, i)| tous[i].clone()).collect()
}

/// Résultat d'un outil.
#[derive(Default)]
struct Res {
    dire: String,
    confirmer: Option<Cote>,
    annuler: String,
    /// Données pour l'IA (résultats web, texte d'une page…) : elle rédige la réponse parlée (2e tour).
    pour_ia: String,
    /// Images en base64 (capture d'un onglet) montrées à l'IA au 2e tour.
    images: Vec<String>,
}

fn res_sortie(s: Sortie) -> Res {
    let confirmer = if let Sortie::Question(id, _) = &s { Some(Cote::Rust(*id)) } else { None };
    let pour_ia = if let Sortie::Donnees(_, p) = &s { p.clone() } else { String::new() };
    Res { dire: s.texte().to_string(), confirmer, annuler: "D'accord, je ne touche à rien.".into(), pour_ia, ..Default::default() }
}

fn res_python(r: &Value) -> Res {
    let ok = r["ok"].as_bool().unwrap_or(false);
    let dire = r["dire"].as_str().map(str::to_string).unwrap_or_else(|| if ok { "C'est fait.".into() } else { "Ça n'a pas marché.".into() });
    Res {
        dire,
        confirmer: r["confirmer"].as_u64().map(Cote::Python),
        annuler: r["annuler"].as_str().unwrap_or("D'accord, j'annule.").to_string(),
        pour_ia: r["pour_ia"].as_str().unwrap_or("").to_string(),
        images: r["images"].as_array().into_iter().flatten().filter_map(|i| i.as_str().map(str::to_string)).collect(),
    }
}

fn duree_parlee(secs: u32) -> String {
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    let mut p = Vec::new();
    if h > 0 {
        p.push(format!("{h} heure{}", if h > 1 { "s" } else { "" }));
    }
    if m > 0 {
        p.push(format!("{m} minute{}", if m > 1 { "s" } else { "" }));
    }
    if s > 0 && h == 0 {
        p.push(format!("{s} seconde{}", if s > 1 { "s" } else { "" }));
    }
    p.join(" ")
}

/// Demande à la fenêtre de la barre (minuteur, alarme…) et attend sa réponse.
pub(crate) fn a_la_barre(req: ApiReq) -> Option<Value> {
    let out = avec(|e| e.out.clone())?;
    let (tx, rx) = channel();
    out.send(Msg::Api(req, tx));
    let (_, corps) = rx.recv_timeout(Duration::from_secs(10)).ok()?;
    serde_json::from_str(&corps).ok()
}

fn outil_barre(nom: &str, args: &Value) -> Option<Res> {
    let s = |k: &str| args[k].as_str().unwrap_or("").trim().to_string();
    let fait = |t: String| Some(Res { dire: t, ..Default::default() });
    let rate = || fait("Désolé, ça n'a pas marché.".into());
    match nom {
        "lancer_minuteur" => {
            let Some(secs) = crate::telephone::duree(&s("duree")) else { return fait("Il me faut une durée, par exemple « 5 minutes ».".into()) };
            match a_la_barre(ApiReq::Timer { secs: secs as f64, nom: s("nom") }) {
                Some(v) if v["ok"].as_bool() == Some(true) => fait(format!("Minuteur de {} lancé.", duree_parlee(secs))),
                _ => rate(),
            }
        }
        "regler_alarme" => {
            let Some((h, m)) = crate::brain::parse_hhmm(&s("heure")) else { return fait("Il me faut une heure, par exemple « 7 h 30 ».".into()) };
            match a_la_barre(ApiReq::Alarm { heure: format!("{h:02}:{m:02}"), active: true }) {
                Some(v) if v["ok"].as_bool() == Some(true) => fait(format!("Alarme réglée à {h} h {m:02}.")),
                _ => rate(),
            }
        }
        "chronometre" => {
            let c = normaliser(&s("commande"));
            let commande = if c.contains("pause") || c.contains("arret") || c.contains("stop") {
                "pause"
            } else if c.contains("zero") || c.contains("remet") || c.contains("reinit") {
                "zero"
            } else {
                "demarrer"
            };
            match a_la_barre(ApiReq::Chrono { commande: commande.into() }) {
                Some(v) if v["ok"].as_bool() == Some(true) => fait(match commande {
                    "pause" => "Chronomètre en pause.".into(),
                    "zero" => "Chronomètre remis à zéro.".into(),
                    _ => "Chronomètre lancé.".into(),
                }),
                _ => rate(),
            }
        }
        "ouvrir_page" => {
            let q = normaliser(&s("quoi"));
            let quoi = if q.contains("mail") { "mails" } else { "pronote" };
            match a_la_barre(ApiReq::Open { quoi: quoi.into(), n: 0, section: String::new() }) {
                Some(v) if v["ok"].as_bool() == Some(true) => fait(if quoi == "mails" { "J'ouvre ta boîte mail.".into() } else { "J'ouvre Pronote.".into() }),
                _ => rate(),
            }
        }
        "actualiser_meteo" => match a_la_barre(ApiReq::WeatherRefresh) {
            Some(_) => fait("Météo actualisée.".into()),
            None => rate(),
        },
        _ => None,
    }
}

fn executer(nom: &str, args: &Value, python: bool) -> Res {
    crate::sys::log(&format!("cerveau : outil {nom} {args}"));
    if python {
        return match jarvis_link::post("/cerveau/outil", json!({"nom": nom, "args": args}), 330) {
            Ok(r) => res_python(&r),
            Err(_) => Res { dire: "Désolé, ça n'a pas marché.".into(), ..Default::default() },
        };
    }
    outil_barre(nom, args).unwrap_or_else(|| res_sortie(pouvoirs::appeler(nom, args)))
}

/// Raccourcis de pouvoirs.rs (téléphone…), utilisés quand la voix ne tourne pas.
fn raccourci_local(n: &str, texte: &str) -> Option<(String, Value)> {
    static REGLES: OnceLock<Vec<(Regex, String, Value)>> = OnceLock::new();
    let regles = REGLES.get_or_init(|| {
        pouvoirs::regles()
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| Some((Regex::new(&format!("(?i){}", r["motif"].as_str()?)).ok()?, r["outil"].as_str()?.to_string(), r["args"].clone())))
            .collect()
    });
    for (motif, outil, modele) in regles {
        let Some(c) = motif.captures(n) else { continue };
        let mut args = serde_json::Map::new();
        for (k, v) in modele.as_object().into_iter().flatten() {
            let valeur = match v.as_str() {
                Some("$$") => json!(texte),
                Some(x) if x.starts_with('$') => json!(c.name(&x[1..]).map(|m| m.as_str()).unwrap_or("")),
                _ => v.clone(),
            };
            args.insert(k.clone(), valeur);
        }
        return Some((outil.clone(), Value::Object(args)));
    }
    None
}

/// L'utilisateur a dit « sur le bureau » (ou documents, téléchargements…) mais l'IA n'a donné qu'un nom de
/// fichier : on remet le dossier devant, sinon le fichier irait dans Documents\Jarvis.
pub fn completer_chemins(appels: &mut [(String, Value)], n: &str) {
    const DOSSIERS: [(&str, &str); 6] =
        [("bureau", "bureau"), ("telechargement", "telechargements"), ("documents", "documents"), ("images", "images"), ("musique", "musique"), ("videos", "videos")];
    let Some(dossier) = DOSSIERS.iter().find(|(mot, _)| n.split(' ').any(|m| m.starts_with(mot))).map(|d| d.1) else { return };
    for (_, args) in appels.iter_mut() {
        let Some(o) = args.as_object_mut() else { continue };
        for cle in ["chemin_fichier", "chemin_dossier", "chemin_element", "source"] {
            let Some(v) = o.get(cle).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty()) else { continue };
            let nu = !v.contains(['\\', '/', ':']) && !normaliser(v).split(' ').any(|m| DOSSIERS.iter().any(|d| m.starts_with(d.0)));
            if nu {
                o.insert(cle.to_string(), json!(format!("{dossier}\\{v}")));
            }
        }
    }
}

// ───────────────────────── l'IA (ia.rs) ─────────────────────────

/// Prépare une demande à l'IA avec les réglages du cerveau.
fn requete(messages: &[Value], outils: &[Value], max_mots: u32, rapide: bool, reg: &Reglages) -> Requete {
    Requete {
        messages: messages.to_vec(),
        outils: outils.to_vec(),
        max_mots,
        temperature: reg.temperature,
        json: false,
        rapide,
        contexte: reg.contexte,
        garder: reg.garder.clone(),
    }
}

// ───────────────────────── le cerveau ─────────────────────────

pub struct Demande {
    pub texte: String,
    /// "voix", "ile", "telephone", "texte"…
    pub source: String,
    /// Ce que Jarvis disait quand on lui a coupé la parole.
    pub coupe: Option<String>,
    /// Ce qu'il n'avait pas encore eu le temps de dire quand on l'a coupé (il en fait un mini-résumé ensuite).
    pub pas_dit: Option<String>,
    /// La voix (Python) tourne : ses modules donnent les outils, les raccourcis et les données.
    pub python: bool,
    /// Données du moment (Pronote, mails, météo…) quand la voix ne tourne pas.
    pub contexte: Option<String>,
    pub reglages: Option<Reglages>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rep {
    /// La réponse complète.
    pub texte: String,
    /// Ce qui reste à dire après les phrases déjà données en direct.
    pub reste: String,
    /// Jarvis a posé une question : il réécoute tout de suite.
    pub suivi: bool,
}

fn rep(texte: impl Into<String>, suivi: bool) -> Rep {
    let t = texte.into();
    Rep { reste: t.clone(), texte: t, suivi }
}

/// Répond à ce que dit l'utilisateur. `sur_phrase` reçoit les phrases dès qu'elles sont prêtes
/// (réponses sans action seulement) ; le reste est dans `Rep::reste`.
pub fn repondre(d: Demande, sur_phrase: &mut dyn FnMut(&str) -> bool) -> Rep {
    if let Some(r) = &d.reglages {
        avec(|e| e.reg_voix = Some(r.clone()));
        ia::rapide_ollama(&r.modele_rapide);
        ia::modele_api(&r.modele_api);
    }
    if let Some(c) = d.coupe.as_deref() {
        corriger_coupe(c);
    }
    let reg = avec(|e| if d.python { e.reg_voix.clone().unwrap_or_else(|| e.reg.clone()) } else { e.reg.clone() });
    let texte = enlever_mot_activation(&d.texte);
    let n = normaliser(&texte);
    if n.is_empty() {
        return rep("Oui ?", d.source == "voix");
    }

    // 1. réponse à une question oui / non
    if let Some(a) = avec(|e| e.attente.take()).filter(|a| a.quand.elapsed() < ATTENTE_VALIDE) {
        let (oui, non) = (est_oui(&n), est_non(&n));
        if oui || non {
            let r = match (&a.cote, oui) {
                (Cote::Rust(id), _) => res_sortie(pouvoirs::confirmer(*id, oui)),
                (Cote::Python(id), true) => jarvis_link::post("/cerveau/confirmer", json!({"id": id}), 330).map(|v| res_python(&v)).unwrap_or(Res { dire: "Désolé, ça n'a pas marché.".into(), ..Default::default() }),
                (Cote::Python(_), false) => Res { dire: a.annuler.clone(), ..Default::default() },
            };
            return fin(&reg, &d, &texte, r.dire.clone(), vec![r], String::new(), &[]);
        }
        // Un ou deux mots, ni oui ni non (« Loss » au lieu de « oui ») : souvent un « oui » mal entendu.
        // On garde la question et on la repose, au lieu de l'oublier.
        if n.split(' ').count() <= 2 && !est_action(&n) && !est_oubli(&n) && !matches!(n.as_str(), "merci" | "merci beaucoup") {
            avec(|e| e.attente = Some(Attente { quand: Instant::now(), ..a }));
            return rep("Je n'ai pas bien entendu : c'est oui ou non ?", true);
        }
        if let Cote::Rust(id) = a.cote {
            let _ = pouvoirs::confirmer(id, false); // autre sujet : la question est oubliée
        }
    }

    // 2. ce qui passe avant l'IA : révision en cours, « oublie tout », raccourcis instantanés
    if d.python {
        if let Ok(v) = jarvis_link::post("/cerveau/avant", json!({"texte": texte, "source": d.source}), 330) {
            if let Some(dire) = v["dire"].as_str().filter(|s| !s.is_empty()) {
                return rep(dire, false);
            }
            if v["oubli"].as_bool() == Some(true) {
                oublier();
                return rep("C'est oublié. On repart de zéro.", false);
            }
            let appels: Vec<(String, Value)> = v["appels"].as_array().into_iter().flatten().filter_map(|a| Some((a[0].as_str()?.to_string(), a[1].clone()))).collect();
            if !appels.is_empty() {
                crate::sys::log(&format!("cerveau : raccourci {appels:?}"));
                return executer_tout(&reg, &d, &texte, appels, String::new(), sur_phrase);
            }
        }
    } else {
        if est_oubli(&n) {
            oublier();
            return rep("C'est oublié. On repart de zéro.", false);
        }
        if !plusieurs_ordres(&n) {
            if let Some(appel) = raccourci_local(&n, &texte) {
                return executer_tout(&reg, &d, &texte, vec![appel], String::new(), sur_phrase);
            }
        }
    }

    // 3. l'IA
    ia(&reg, &d, &texte, &n, sur_phrase)
}

/// Jarvis a été coupé : sa dernière réponse, dans l'historique, devient ce qu'il a VRAIMENT dit
/// (pour qu'il sache où il en était), au lieu de toute la réponse prévue.
pub fn corriger_coupe(dit: &str) {
    let dit = dit.trim().trim_matches('…').trim();
    if dit.is_empty() {
        // coupé avant d'avoir rien dit : l'utilisateur n'avait pas fini sa phrase (la voix la recolle), on oublie cet échange
        avec(|e| {
            e.histo.pop_back();
        });
        return;
    }
    let texte = format!("{dit}… (coupé ici par l'utilisateur)");
    avec(|e| {
        if let Some(m) = e.histo.back_mut().and_then(|ech| ech.iter_mut().rev().find(|m| m["role"] == "assistant" && m.get("tool_calls").is_none())) {
            m["content"] = json!(texte);
        }
    });
}

/// Consigne pour l'IA quand l'utilisateur lui a parlé par-dessus : où il en était, et ce qu'il n'a pas dit.
/// Il répond d'abord au nouveau message, puis résume en une phrase ce qui restait, si c'est encore utile.
pub fn note_coupure(dit: &str, pas_dit: &str) -> String {
    let dit = dit.trim().trim_matches('…').trim();
    if dit.is_empty() {
        return String::new();
    }
    let n = dit.chars().count();
    let fin: String = dit.chars().skip(n.saturating_sub(300)).collect();
    let mut note = format!(
        "\nTu viens d'être coupé par l'utilisateur pendant que tu disais : « {fin} ». C'est normal dans une discussion : \
ne le prends pas mal et ne t'excuse pas."
    );
    let reste = pas_dit.trim();
    if reste.is_empty() {
        note.push_str(" Réponds simplement à ce qu'il vient de dire.");
    } else {
        let reste: String = reste.chars().take(400).collect();
        note.push_str(&format!(
            " Tu n'as pas eu le temps de dire : « {reste} ». Réponds D'ABORD à ce qu'il vient de dire. Ensuite, seulement \
si c'est encore utile (même sujet, et il ne t'a pas demandé d'arrêter ou de changer de sujet), ajoute UNE phrase très \
courte qui résume ce que tu n'avais pas eu le temps de dire, par exemple « Et pour finir ce que je disais : … ». \
Sinon, n'en parle pas."
        ));
    }
    note
}

fn donnees(d: &Demande, texte: &str) -> String {
    if d.python {
        if let Ok(v) = jarvis_link::post("/cerveau/contexte", json!({"question": texte}), 20) {
            return v["texte"].as_str().unwrap_or("").to_string();
        }
    }
    match d.contexte.as_deref() {
        Some(c) if !c.is_empty() => format!("DONNÉES ACTUELLES (à jour, utilise-les pour répondre) :\n{c}"),
        _ => String::new(),
    }
}

fn quelle_ia(reg: &Reglages) -> String {
    ia::actif().unwrap_or_else(|| if reg.modele_api.is_empty() { String::new() } else { format!("NVIDIA · {}", reg.modele_api) })
}

fn ia(reg: &Reglages, d: &Demande, texte: &str, n: &str, sur_phrase: &mut dyn FnMut(&str) -> bool) -> Rep {
    let mut systeme = consigne(&reg.nom, &reg.consigne, &quelle_ia(reg));
    if let Some(c) = d.coupe.as_deref() {
        systeme.push_str(&note_coupure(c, d.pas_dit.as_deref().unwrap_or("")));
    }
    systeme.push('\n');
    systeme.push_str(&donnees(d, texte));
    let mut messages = vec![json!({"role": "system", "content": systeme})];
    messages.extend(avec(|e| e.histo.iter().flatten().cloned().collect::<Vec<_>>()));
    messages.push(json!({"role": "user", "content": texte}));

    let tous = tous_les_outils(d.python);
    let connus: Vec<String> = tous.iter().map(|o| nom_outil(o).to_string()).collect();
    let (derniers, sans_outil) = avec(|e| (e.derniers.clone(), e.sans_outil));
    // Phrase courte juste après une action (« exécute-la », « oui vas-y », « l'autre », « encore ») : sans verbe
    // d'action, mais c'est la suite. On lui redonne les outils de cette action, sinon elle ne peut que parler.
    let suite = !derniers.is_empty() && sans_outil <= 1 && n.split(' ').count() <= 5;
    let action = est_action(n) || suite;
    let mut choisis = if action { choisir(&tous, n, &derniers) } else { Vec::new() };
    if besoin_d_internet(n) && tous.iter().any(|o| OUTILS_INTERNET.contains(&nom_outil(o))) {
        for o in tous.iter().filter(|o| OUTILS_INTERNET.contains(&nom_outil(o))) {
            if !choisis.iter().any(|c| nom_outil(c) == nom_outil(o)) {
                choisis.push(o.clone());
            }
        }
        // juste après la question, sinon l'IA répond de mémoire (parfois avec une info dépassée)
        messages.push(json!({"role": "system", "content": "Si la réponse a pu changer depuis tes connaissances (actualité, \
sport, « le dernier… », prix, sorties…), appelle d'abord web_search au lieu de répondre de mémoire. Si la question porte \
sur la page ouverte dans Chrome, appelle lire_onglet (ou capture_onglet pour la voir). Même si tu l'as déjà fait plus tôt, rappelle l'outil au lieu de recopier une ancienne réponse : la page ou la situation a pu changer. Sinon, réponds normalement."}));
        crate::sys::log("cerveau : outils Internet proposés");
    }
    if choisis.iter().any(|o| nom_outil(o) == "fenetre") {
        messages.push(json!({"role": "system", "content": "L'outil fenetre marche sur TOUTES les fenêtres et pages \
affichées, quelle que soit l'application : pour réduire, mettre en petit, agrandir, mettre en plein écran, remettre ou \
fermer ce qui est à l'écran, appelle-le (nom vide = la fenêtre devant l'utilisateur). Ne dis jamais qu'il te manque \
un outil pour cette page ou cette appli."}));
    }
    let filtrer = |appels: Vec<(String, Value)>| -> Vec<(String, Value)> {
        let (bons, inconnus): (Vec<_>, Vec<_>) = appels.into_iter().partition(|a| connus.contains(&a.0));
        if !inconnus.is_empty() {
            crate::sys::log(&format!("cerveau : l'IA a inventé des outils : {inconnus:?}"));
        }
        bons
    };
    let echec = |e: Echec| match e {
        Echec::Injoignable => rep("Je n'arrive à joindre aucune IA. Ouvre LM Studio, ou ajoute une clé gratuite : clic droit sur la barre, Paramètres, IA.", false),
        Echec::Autre(t) => {
            crate::sys::log(&format!("cerveau : l'IA a répondu une erreur : {t}"));
            rep("J'ai eu un souci pour réfléchir à ça, désolé.", false)
        }
    };

    // Premier appel. Sans outil à proposer, la réponse part en direct, phrase par phrase.
    let mut dit = String::new();
    let mut menti = false;
    let mut annule = false;
    let premier = if choisis.is_empty() {
        let mut f = |p: &str| {
            if pretend_agir(p, false) || ecrit_un_outil(p, &connus) {
                menti = true;
                return false;
            }
            if !sur_phrase(p) {
                // la voix ne lit plus (on lui a coupé la parole) : on arrête l'IA tout de suite
                annule = true;
                return false;
            }
            if !dit.is_empty() {
                dit.push(' ');
            }
            dit.push_str(p);
            true
        };
        ia::discuter(&requete(&messages, &[], 300, action, reg), Some(&mut f as &mut dyn FnMut(&str) -> bool))
    } else {
        ia::discuter(&requete(&messages, &choisis, 300, action, reg), None)
    };
    let r = match premier {
        Ok(r) => r,
        Err(e) => return echec(e),
    };
    if annule {
        crate::sys::log("cerveau : coupé par l'utilisateur, l'IA est arrêtée");
        let mut r = fin(reg, d, texte, dit, Vec::new(), String::new(), &[]);
        r.reste = String::new();
        return r;
    }
    let mut contenu = nettoyer(&r.contenu);
    let mut appels = filtrer(r.appels);

    // L'IA dit qu'elle ne sait pas faire alors qu'on ne lui a pas montré tous les outils : on les lui montre tous.
    if appels.is_empty() && !choisis.is_empty() && choisis.len() < tous.len() && dit_ne_pas_savoir(&normaliser(&contenu)) {
        crate::sys::log("cerveau : outil peut-être manquant, je redemande avec tous les outils");
        if let Ok(r2) = ia::discuter(&requete(&messages, &tous, 300, action, reg), None) {
            let a2 = filtrer(r2.appels);
            if !a2.is_empty() {
                appels = a2;
                contenu = nettoyer(&r2.contenu);
            }
        }
    }
    // Rien d'utilisable : on redemande une vraie réponse, sans outils.
    if appels.is_empty() && contenu.is_empty() && !menti {
        if let Ok(r2) = ia::discuter(&requete(&messages, &[], 300, false, reg), None) {
            contenu = nettoyer(&r2.contenu);
        }
    }
    // Elle prétend avoir agi sans outil : on lui redemande, outils en main.
    if appels.is_empty() && (menti || pretend_agir(&contenu, action) || ecrit_un_outil(&contenu, &connus)) {
        crate::sys::log(&format!("cerveau : l'IA prétend avoir agi sans outil (« {contenu} ») — je lui redemande"));
        let mut relance = messages.clone();
        relance.push(json!({"role": "assistant", "content": contenu}));
        relance.push(json!({"role": "system", "content": "ATTENTION : tu n'as appelé AUCUN outil, donc rien n'a été fait. \
Si l'utilisateur demande une action, appelle maintenant l'outil adapté. Sinon, dis-lui honnêtement que tu n'as rien fait."}));
        let pour_relance = if choisis.is_empty() { choisir(&tous, n, &[]) } else { choisis.clone() };
        match ia::discuter(&requete(&relance, &pour_relance, 300, action, reg), None) {
            Ok(r2) => {
                appels = filtrer(r2.appels);
                contenu = nettoyer(&r2.contenu);
            }
            Err(_) => contenu.clear(),
        }
        if appels.is_empty() && (contenu.is_empty() || pretend_agir(&contenu, action) || ecrit_un_outil(&contenu, &connus)) {
            contenu = "En fait, je n'ai rien fait. Redis-moi précisément ce que tu veux.".into();
        }
        // ce qui a déjà été dit en direct reste dit ; la suite est la nouvelle réponse
        if appels.is_empty() {
            return fin(reg, d, texte, contenu, Vec::new(), dit, &[]);
        }
    }
    if !appels.is_empty() {
        completer_chemins(&mut appels, n);
        crate::sys::log(&format!("cerveau : outils choisis par l'IA : {appels:?}"));
        return executer_tout(reg, d, texte, appels, contenu, sur_phrase);
    }
    if !dit.is_empty() {
        // tout a été dit en direct
        let mut r = fin(reg, d, texte, dit, Vec::new(), String::new(), &[]);
        r.reste = String::new();
        return r;
    }
    fin(reg, d, texte, if contenu.is_empty() { "Je n'ai pas de réponse à ça.".into() } else { contenu }, Vec::new(), String::new(), &[])
}

fn executer_tout(reg: &Reglages, d: &Demande, texte: &str, appels: Vec<(String, Value)>, contenu: String, sur_phrase: &mut dyn FnMut(&str) -> bool) -> Rep {
    let noms: Vec<String> = appels.iter().map(|a| a.0.clone()).collect();
    let (mut attente_dite, mut coupe) = (false, false);
    let mut resultats = Vec::new();
    for (nom, args) in &appels {
        // outil lent (web, page, capture) : une phrase courte tout de suite, pour ne pas laisser de silence
        // (elle n'est pas gardée dans l'historique : l'IA la recopierait dans ses réponses suivantes)
        if !attente_dite && !coupe && OUTILS_INTERNET.contains(&nom.as_str()) {
            if sur_phrase(PHRASE_ATTENTE) {
                attente_dite = true;
            } else {
                coupe = true;
            }
        }
        resultats.push(executer(nom, args, d.python));
    }
    avec(|e| {
        e.derniers = noms;
        e.sans_outil = 0;
    });
    let pour_l_ia = resultats.iter().any(|r| !r.pour_ia.is_empty() || !r.images.is_empty());
    if pour_l_ia && !coupe && resultats.iter().all(|r| r.confirmer.is_none()) {
        return second_tour(reg, d, texte, &appels, resultats, sur_phrase);
    }
    let phrase = resultats.iter().map(|r| r.dire.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
    let phrase = if !phrase.is_empty() {
        phrase
    } else if !contenu.is_empty() {
        contenu
    } else {
        "C'est fait.".into()
    };
    fin(reg, d, texte, phrase, resultats, String::new(), &appels)
}

/// 2e tour : l'IA reçoit les résultats des outils (et les images) et rédige la réponse parlée, en direct.
fn second_tour(reg: &Reglages, d: &Demande, texte: &str, appels: &[(String, Value)], resultats: Vec<Res>, sur_phrase: &mut dyn FnMut(&str) -> bool) -> Rep {
    let mut systeme = consigne(&reg.nom, &reg.consigne, &quelle_ia(reg));
    systeme.push_str(
        "\nTu viens d'utiliser des outils : leurs résultats sont dans la conversation. Réponds maintenant à la question \
en une ou deux phrases parlées (plus seulement si on te demande un résumé ou une explication), en t'appuyant sur ces \
résultats. Ne lis ni les liens, ni les noms d'outils, ni les numéros des résultats. Si les résultats ne donnent pas \
la réponse, dis-le simplement.",
    );
    let mut messages = vec![json!({"role": "system", "content": systeme})];
    messages.extend(avec(|e| e.histo.iter().flatten().cloned().collect::<Vec<_>>()));
    messages.push(json!({"role": "user", "content": texte}));
    let liste: Vec<Value> = appels.iter().map(|(nom, args)| json!({"function": {"name": nom, "arguments": args}})).collect();
    messages.push(json!({"role": "assistant", "content": "", "tool_calls": liste}));
    for ((nom, _), r) in appels.iter().zip(&resultats) {
        let contenu = if r.pour_ia.is_empty() { &r.dire } else { &r.pour_ia };
        messages.push(json!({"role": "tool", "tool_name": nom, "content": contenu}));
    }
    let images: Vec<&String> = resultats.iter().flat_map(|r| &r.images).collect();
    if !images.is_empty() {
        messages.push(json!({"role": "user", "content": "Voici la capture de l'onglet.", "images": images}));
    }
    let connus: Vec<String> = appels.iter().map(|a| a.0.clone()).collect();
    let (mut dit, mut arrete) = (String::new(), false);
    let mut f = |p: &str| {
        if ecrit_un_outil(p, &connus) {
            return true; // jamais lu à voix haute
        }
        if !sur_phrase(p) {
            arrete = true;
            return false;
        }
        if !dit.is_empty() {
            dit.push(' ');
        }
        dit.push_str(p);
        true
    };
    let r = ia::discuter(&requete(&messages, &[], 400, false, reg), Some(&mut f as &mut dyn FnMut(&str) -> bool));
    let secours = || resultats.iter().map(|r| r.dire.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
    match r {
        Ok(_) if arrete || !dit.is_empty() => {
            // tout a été dit en direct (ou on lui a coupé la parole)
            let mut rep = fin(reg, d, texte, dit, resultats, String::new(), appels);
            rep.reste = String::new();
            rep
        }
        Ok(r2) => {
            // le service n'a rien donné en direct : on prend sa réponse entière
            let c = nettoyer(&r2.contenu);
            let reponse = if c.is_empty() || ecrit_un_outil(&c, &connus) { secours() } else { c };
            fin(reg, d, texte, reponse, resultats, String::new(), appels)
        }
        Err(_) => {
            let s = secours();
            fin(reg, d, texte, s, resultats, String::new(), appels)
        }
    }
}

/// Range l'échange en mémoire, garde la question en attente s'il y en a une, et prévient les modules.
/// `deja_dit` : phrases déjà données en direct avant `reponse`.
/// `appels` : les outils appelés pour cet échange. Ils restent dans la mémoire de la conversation : l'IA voit
/// qu'une action passe TOUJOURS par un outil, et ne répond plus « c'est fait » par simple imitation.
fn fin(reg: &Reglages, d: &Demande, texte: &str, reponse: String, resultats: Vec<Res>, deja_dit: String, appels: &[(String, Value)]) -> Rep {
    let complet = if deja_dit.is_empty() { reponse.clone() } else { format!("{deja_dit} {reponse}") };
    let mut echange = vec![json!({"role": "user", "content": texte})];
    if !appels.is_empty() {
        let liste: Vec<Value> = appels.iter().map(|(nom, args)| json!({"function": {"name": nom, "arguments": args}})).collect();
        echange.push(json!({"role": "assistant", "content": "", "tool_calls": liste}));
        for ((nom, _), r) in appels.iter().zip(&resultats) {
            let source = if r.pour_ia.is_empty() { &r.dire } else { &r.pour_ia };
            let court: String = source.chars().take(600).collect();
            echange.push(json!({"role": "tool", "tool_name": nom, "content": court}));
        }
    }
    let court: String = complet.chars().take(1200).collect();
    echange.push(json!({"role": "assistant", "content": court}));
    let question = resultats.into_iter().find(|r| r.confirmer.is_some());
    let suivi = question.is_some();
    avec(|e| {
        if let Some(q) = question {
            e.attente = Some(Attente { cote: q.confirmer.unwrap_or(Cote::Rust(0)), annuler: q.annuler, quand: Instant::now() });
        }
        if appels.is_empty() {
            e.sans_outil += 1;
        }
        e.histo.push_back(echange);
        while e.histo.len() > reg.historique.max(1) {
            e.histo.pop_front();
        }
    });
    if d.python {
        let corps = json!({"question": texte, "reponse": complet, "source": d.source});
        std::thread::spawn(move || {
            let _ = jarvis_link::post("/cerveau/apres", corps, 10);
        });
    }
    Rep { texte: complet, reste: reponse, suivi }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outil(nom: &str, desc: &str) -> Value {
        json!({"type": "function", "function": {"name": nom, "description": desc, "parameters": {"type": "object", "properties": {}, "required": []}}})
    }

    #[test]
    fn texte_normalise_et_mot_d_activation() {
        assert_eq!(normaliser("Lance-moi Discord !"), "lance moi discord");
        assert_eq!(normaliser("  Éteins   l’écran, s'il te plaît "), "eteins l'ecran s'il te plait");
        assert_eq!(enlever_mot_activation("Hey Jarvis, mets pause"), "mets pause");
        assert_eq!(enlever_mot_activation("Dis Jarvis quelle heure est-il ?"), "quelle heure est-il ?");
        assert_eq!(enlever_mot_activation("lance Discord"), "lance Discord");
    }

    #[test]
    fn oui_non_action_et_mensonge() {
        assert!(est_oui("oui vas y") && est_oui("d'accord") && !est_oui("ouvre chrome"));
        assert!(est_non("non merci") && est_non("annule") && !est_non("nouveau fichier"));
        assert!(est_action("lance discord") && est_action("fais sonner mon telephone"));
        assert!(!est_action("quel temps fait il demain"));
        assert!(pretend_agir("c'est fait", false) && pretend_agir("j'ai bien lance discord", false));
        assert!(!pretend_agir("il fait beau demain", true));
        // une action était demandée : ces phrases sont des mensonges s'il n'y a pas eu d'outil
        assert!(pretend_agir("c'est supprime essai txt", true) && pretend_agir("le fichier a ete cree", true));
        assert!(pretend_agir("les fichiers sont supprimes", true) && pretend_agir("je supprime le fichier", true));
        assert!(!pretend_agir("le fichier a ete cree en 1990", false));
        // décrire un état n'est pas mentir (vu le 2026-11-18 : ses vraies réponses étaient jetées)
        assert!(!pretend_agir("Je ne sais pas quelle application tu utilises. Je peux ouvrir Chrome si tu veux, ou regarder quelles fenêtres sont ouvertes pour voir si un navigateur est déjà lancé. Tu préfères quoi ?", true));
        assert!(!pretend_agir("Je ne peux pas scanner ton PC pour voir quelles applications sont installées.", true));
        assert!(!pretend_agir("Discord est ouvert ?", true));
        assert!(pretend_agir("Voilà. Ton micro est coupé.", true) && pretend_agir("Les fichiers sont supprimés !", true));
        assert!(pretend_agir("Je lance Chrome avec PowerShell, en tapant la commande Start-Process chrome.", true));
        // les phrases de suite et les « oui » qui donnent une autorisation
        for p in ["execute la", "vas y mets la dans powershell", "va sur vinted", "utilise powershell", "fais le", "tu as mon autorisation"] {
            assert!(est_action(p), "{p}");
        }
        for p in ["tu as mon autorisation", "je t'autorise", "execute la", "lance le", "oui vas y"] {
            assert!(est_oui(p), "{p}");
        }
        assert!(!est_oui("lance le minuteur") && !est_oui("execute la commande dir"));
        assert!(dit_ne_pas_savoir("desole je ne sais pas encore faire ca"));
        assert!(plusieurs_ordres("ouvre chrome et mets un minuteur") && !plusieurs_ordres("ouvre chrome"));
        assert!(est_oubli("oublie tout") && !est_oubli("oublie ce fichier"));
    }

    #[test]
    fn outil_ecrit_en_texte() {
        let connus = vec!["ouvrir_application".to_string(), "aller_onglet".to_string(), "alarme".to_string()];
        assert!(ecrit_un_outil("ouvrir_application Chrome", &connus));
        assert!(ecrit_un_outil("<tool_call> {\"name\": \"x\"}", &connus));
        assert!(!ecrit_un_outil("J'ouvre Chrome pour toi.", &connus));
        assert!(!ecrit_un_outil("Ton alarme sonne à 7 heures.", &connus));
    }

    #[test]
    fn phrases_en_direct() {
        let t = "Bonjour. Il fait beau aujourd'hui, avec un grand soleil sur toute la région ! Demain il pleuvra";
        let (p, vu) = phrases_pretes(t, 0, false);
        assert_eq!(p, vec!["Bonjour. Il fait beau aujourd'hui, avec un grand soleil sur toute la région !"]);
        let (p2, vu2) = phrases_pretes(t, vu, true);
        assert_eq!(p2, vec!["Demain il pleuvra"]);
        assert_eq!(vu2, t.len());
        // un nombre à virgule ou « 3.5 » ne coupe pas la phrase
        assert!(phrases_pretes("La version 3.5 est sortie", 0, false).0.is_empty());
        // rien à redire quand tout est déjà parti
        assert!(phrases_pretes(t, t.len(), true).0.is_empty());
    }

    #[test]
    fn balises_de_reflexion_cachees() {
        assert_eq!(visible("<think>je réfléchis"), "");
        assert_eq!(visible("<think>hmm</think>Bonjour"), "Bonjour");
        assert_eq!(visible("Salut <think>hm"), "Salut ");
    }

    #[test]
    fn choix_des_outils() {
        let mut tous: Vec<Value> = (0..30).map(|i| outil(&format!("truc_{i}"), "ne sert à rien ici")).collect();
        tous.push(outil("lancer_application", "Lance une application du PC (Chrome, Discord…)."));
        tous.push(outil("regler_volume", "Monte ou baisse le volume du son."));
        tous.push(outil("sonner_telephone", "Fait sonner le téléphone pour le retrouver."));
        let c = choisir(&tous, "fais sonner mon telephone", &[]);
        assert!(c.len() <= crate::ia::max_outils());
        assert_eq!(nom_outil(&c[0]), "sonner_telephone");
        let c = choisir(&tous, "monte le volume", &[]);
        assert_eq!(nom_outil(&c[0]), "regler_volume");
        // l'outil du dernier échange reste proposé (« encore un peu »)
        let c = choisir(&tous, "lance discord", &["regler_volume".to_string()]);
        assert!(c.iter().any(|o| nom_outil(o) == "regler_volume") && nom_outil(&c[0]) == "lancer_application");
        // rien ne ressemble : on donne tout
        assert_eq!(choisir(&tous, "xyzzy", &[]).len(), tous.len());
        // peu d'outils : on donne tout
        assert_eq!(choisir(&tous[..5], "monte le volume", &[]).len(), 5);
    }

    #[test]
    fn dossier_oublie_par_l_ia() {
        let mut a = vec![("creer_fichier".to_string(), json!({"chemin_fichier": "essai.txt", "contenu": "bonjour"}))];
        completer_chemins(&mut a, "cree un fichier essai txt sur le bureau avec ecrit bonjour");
        assert_eq!(a[0].1["chemin_fichier"], "bureau\\essai.txt");
        assert_eq!(a[0].1["contenu"], "bonjour");
        // un chemin déjà complet n'est pas touché, et sans dossier cité rien ne change
        let mut b = vec![("supprimer".to_string(), json!({"chemin_element": "documents\\x.txt"})), ("lire_fichier".to_string(), json!({"chemin_fichier": "C:\\a\\b.txt"}))];
        completer_chemins(&mut b, "supprime x txt du bureau");
        assert_eq!(b[0].1["chemin_element"], "documents\\x.txt");
        assert_eq!(b[1].1["chemin_fichier"], "C:\\a\\b.txt");
        let mut c = vec![("creer_fichier".to_string(), json!({"chemin_fichier": "essai.txt"}))];
        completer_chemins(&mut c, "cree un fichier essai txt");
        assert_eq!(c[0].1["chemin_fichier"], "essai.txt");
    }

    #[test]
    fn raccourcis_sans_la_voix() {
        let (o, a) = raccourci_local("fais sonner mon telephone je l'ai perdu", "Fais sonner mon téléphone, je l'ai perdu").unwrap();
        assert_eq!(o, "sonner_telephone");
        assert_eq!(a, json!({}));
        let (o, a) = raccourci_local("mets mon telephone en silencieux", "Mets mon téléphone en silencieux").unwrap();
        assert_eq!(o, "mode_telephone");
        assert_eq!(a["mode"], "silencieux");
        assert!(raccourci_local("quel temps fait il", "Quel temps fait-il ?").is_none());
        // « appelle maman avec WhatsApp » : appel WhatsApp, pas un appel normal à « maman avec whatsapp »
        // « avec WhatsApp » : l'appli WhatsApp du PC
        for phrase in ["appelle maman avec whatsapp", "appelle maman depuis what's app", "appelle maman whatsapp", "appelle maman sur whatsapp sur le pc"] {
            let (o, a) = raccourci_local(phrase, phrase).unwrap();
            assert_eq!((o.as_str(), &a), ("appeler_whatsapp_pc", &json!({"a": "maman"})), "{phrase}");
        }
        // « avec WhatsApp sur mon téléphone » : WhatsApp du téléphone
        let (o, a) = raccourci_local("appelle maman avec whatsapp sur mon telephone", "x").unwrap();
        assert_eq!((o.as_str(), &a), ("appeler_telephone", &json!({"a": "maman", "appli": "whatsapp"})));
        // un numéro dicté
        let (o, a) = raccourci_local("appelle le 06 12 34 56 78 avec whatsapp", "x").unwrap();
        assert_eq!((o.as_str(), a["a"].as_str().unwrap()), ("appeler_whatsapp_pc", "le 06 12 34 56 78"));
        assert_eq!(pouvoirs::cible_whatsapp("06 12 34 56 78"), "+33612345678");
        assert_eq!(pouvoirs::cible_whatsapp("33 7 11 22 33 44"), "+33711223344");
        assert_eq!(pouvoirs::cible_whatsapp("+33 7 11 22 33 44"), "+33711223344");
        assert_eq!(pouvoirs::cible_whatsapp("Maman"), "Maman");
        assert_eq!(pouvoirs::erreur_whatsapp("ERREUR:introuvable", "Tom"), "Je ne trouve pas Tom dans WhatsApp sur le PC.");
        assert!(pouvoirs::erreur_whatsapp("ERREUR:pas_d_appel:Moi", "x").contains("Moi"));
        let (o, a) = raccourci_local("appelle maman", "Appelle maman").unwrap();
        assert_eq!((o.as_str(), &a), ("appeler_telephone", &json!({"a": "maman"})));
        // la transcription écrit souvent « Appel Anaïs » pour « Appelle Anaïs »
        let (o, a) = raccourci_local("appel anais", "Appel Anaïs").unwrap();
        assert_eq!((o.as_str(), &a), ("appeler_telephone", &json!({"a": "anais"})));
        assert_eq!(pouvoirs::contact_whatsapp("anais", &["Anaïs".to_string()]).as_deref(), Some("Anaïs"));
    }

    #[test]
    fn fenetres_rangement_et_reessai() {
        let cas = [
            ("met mon navigateur en premier plan", "premier_plan", json!({"nom": "mon navigateur"})),
            ("mets discord au premier plan", "premier_plan", json!({"nom": "discord"})),
            ("dis moi quelles sont les applications ouvertes", "lister_fenetres", json!({})),
            ("dis moi toutes les applications ouvertes et ouvertes en arriere plan", "lister_fenetres", json!({})),
            ("fais du tri sur mon bureau", "ranger_dossier", json!({"chemin_dossier": "mon bureau"})),
            ("range mon bureau", "ranger_dossier", json!({"chemin_dossier": "mon bureau"})),
            ("reessaie l'appel", "reessayer_appel", json!({})),
            ("recommence", "reessayer_appel", json!({})),
        ];
        for (phrase, outil, args) in cas {
            let (o, a) = raccourci_local(phrase, phrase).unwrap_or_else(|| panic!("{phrase}"));
            assert_eq!((o.as_str(), &a), (outil, &args), "{phrase}");
        }
        assert!(raccourci_local("ferme les applications ouvertes", "x").map(|(o, _)| o != "lister_fenetres").unwrap_or(true));
    }

    #[test]
    fn reglages_de_la_voix() {
        // « ollama_url » et « modele » sont ignorés : le service est choisi dans ia.bin
        let r = Reglages::depuis_json(&json!({"ollama_url": "http://127.0.0.1:11434/", "modele": "qwen3-vl", "historique": 5, "temperature": 0.2, "modele_rapide": "qwen3-4b"})).unwrap();
        assert_eq!((r.historique, r.temperature, r.modele_rapide.as_str()), (5, 0.2, "qwen3-4b"));
        assert!(Reglages::depuis_json(&json!(null)).is_none());
    }

    #[test]
    fn appels_d_outils_lus() {
        let mut a = Vec::new();
        ia::lire_appels(&json!({"tool_calls": [{"function": {"name": "lancer_minuteur", "arguments": {"duree": "5 minutes"}}}, {"function": {"name": "x", "arguments": "{\"a\":1}"}}]}), &mut a);
        assert_eq!(a, vec![("lancer_minuteur".to_string(), json!({"duree": "5 minutes"})), ("x".to_string(), json!({"a": 1}))]);
        assert_eq!(duree_parlee(5400), "1 heure 30 minutes");
        assert_eq!(duree_parlee(45), "45 secondes");
    }

    #[test]
    fn questions_qui_demandent_internet() {
        assert!(besoin_d_internet(&normaliser("Qui a gagné la dernière Coupe du monde de rugby ?")));
        assert!(besoin_d_internet(&normaliser("Résume la page que j'ai ouverte dans Chrome")));
        assert!(!besoin_d_internet(&normaliser("Raconte-moi une blague")));
        assert!(!besoin_d_internet(&normaliser("Combien font 7 fois 8 ?")));
    }
}
