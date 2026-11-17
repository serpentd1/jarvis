//! Le « cerveau » de l'île : l'IA (ia.rs : LM Studio, Ollama ou une clé d'API) avec un historique court, un résumé à jour
//! des données (Pronote, mails, météo, minuteurs) et des actions qu'elle peut déclencher.

use serde_json::Value;

/// Ce que l'IA peut faire.
#[derive(Debug, Clone, PartialEq)]
pub enum AiAction {
    Timer(i64, String),
    Alarm(u32, u32),
    Chrono(String),
    OpenMail(usize),
    OpenPronote,
    OpenInbox,
    Weather,
    Notif(String, String),
    /// Pouvoir sur le PC (voir pouvoirs.rs) : nom de l'outil, arguments.
    Pc(String, Value),
}

/// Requête envoyée au fil réseau.
pub struct AskReq {
    pub id: u64,
    pub system: String,
    pub history: Vec<(String, String)>,
    pub question: String,
    pub temp: f32,
    pub json: bool,
}

pub const MODELS: [&str; 6] = ["qwen3.5:4b", "(modèle des réglages mails)", "qwen2.5:7b", "llama3.2", "gemma3:4b", "mistral"];
pub const STYLES: [&str; 5] = ["Normal", "Très court", "Détaillé", "Pirate", "Majordome (Jarvis)"];

/// Consigne système : rôle + style + données du moment + format des actions.
/// La phrase de style choisie dans les réglages de l'île.
pub fn style_text(style: usize) -> &'static str {
    match style {
        1 => "Réponds en une seule phrase très courte.",
        2 => "Réponds de façon complète et claire, en expliquant simplement (6 phrases au maximum).",
        3 => "Parle comme un pirate sympathique (« moussaillon », « mille sabords »), en 3 phrases courtes au maximum.",
        4 => "Tu es Jarvis, un majordome numérique poli et un peu pince-sans-rire ; appelle l'utilisateur « Monsieur » ; 3 phrases courtes au maximum.",
        _ => "Réponds simplement, en 3 phrases courtes au maximum.",
    }
}

pub fn system_prompt(style: usize, context: &str, actions: bool) -> String {
    let style_txt = style_text(style);
    let mut s = format!(
        "Tu es l'assistant personnel d'un collégien français, intégré à l'île Jarvis. Réponds toujours en français. {style_txt} \
Tes réponses peuvent être lues à voix haute : pas de listes à puces, pas de Markdown, pas d'émojis. \
Si on te demande une information que tu n'as pas dans les données ci-dessous, dis-le franchement au lieu d'inventer."
    );
    if !context.is_empty() {
        s.push_str("\n\nDONNÉES ACTUELLES (à jour, utilise-les pour répondre) :\n");
        s.push_str(context);
    }
    if actions {
        s.push_str(
            "\n\nRéponds UNIQUEMENT avec un objet JSON de la forme {\"reponse\": \"ce que tu dis\", \"actions\": [...]}.\n\
Actions possibles (mets \"actions\": [] si l'utilisateur ne demande rien de concret) :\n\
- {\"type\": \"minuteur\", \"minutes\": 5, \"nom\": \"pâtes\"} : lance un minuteur (minutes peut être décimal, 0.5 = 30 s)\n\
- {\"type\": \"alarme\", \"heure\": \"07:30\"} : règle et active l'alarme\n\
- {\"type\": \"chrono\", \"commande\": \"demarrer\"} : commande = demarrer, pause ou zero\n\
- {\"type\": \"ouvrir_mail\", \"n\": 0} : ouvre le mail numéro n de la liste des données\n\
- {\"type\": \"ouvrir_pronote\"} ; {\"type\": \"ouvrir_mails\"} ; {\"type\": \"meteo\"} (actualiser la météo)\n\
- {\"type\": \"notif\", \"titre\": \"…\", \"texte\": \"…\"} : affiche un rappel dans la barre\n\
- {\"type\": \"pc\", \"outil\": \"nom\", \"args\": {…}} : agit sur le PC avec un de ces outils (les actions risquées \
demandent une confirmation à l'utilisateur ; dis seulement ce que tu lances, pas que c'est fait) :\n",
        );
        s.push_str(&crate::pouvoirs::resume_outils());
        s.push_str("\nNe déclenche une action que si on te la demande clairement.");
    }
    s
}

/// Lit la réponse du modèle : texte à afficher + actions. Tolère un texte simple (pas de JSON).
pub fn parse(content: &str, json: bool) -> (String, Vec<AiAction>) {
    let c = content.trim();
    if !json {
        return (c.to_string(), Vec::new());
    }
    // le JSON peut être entouré de texte ou de ```json … ```
    let j: Option<Value> = serde_json::from_str(c).ok().or_else(|| {
        let (a, b) = (c.find('{')?, c.rfind('}')?);
        serde_json::from_str(&c[a..=b]).ok()
    });
    let Some(j) = j else { return (c.to_string(), Vec::new()) };
    let text = ["reponse", "réponse", "response", "texte"]
        .iter()
        .find_map(|k| j[*k].as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mut acts = Vec::new();
    for a in j["actions"].as_array().into_iter().flatten() {
        let num = |k: &str| a[k].as_f64().or_else(|| a[k].as_str().and_then(|s| s.trim().replace(',', ".").parse().ok()));
        let typ = a["type"].as_str().unwrap_or("").to_lowercase();
        let act = match typ.as_str() {
            "minuteur" | "timer" => num("minutes").filter(|m| *m > 0.0 && *m <= 24.0 * 60.0).map(|m| {
                AiAction::Timer((m * 60.0).round() as i64, a["nom"].as_str().unwrap_or("").trim().to_string())
            }),
            "alarme" | "alarm" => a["heure"].as_str().and_then(parse_hhmm).map(|(h, m)| AiAction::Alarm(h, m)),
            "chrono" => Some(AiAction::Chrono(a["commande"].as_str().unwrap_or("demarrer").to_lowercase())),
            "ouvrir_mail" => num("n").map(|n| AiAction::OpenMail(n.max(0.0) as usize)),
            "ouvrir_pronote" => Some(AiAction::OpenPronote),
            "ouvrir_mails" => Some(AiAction::OpenInbox),
            "meteo" | "météo" => Some(AiAction::Weather),
            "notif" => Some(AiAction::Notif(
                a["titre"].as_str().unwrap_or("Rappel").to_string(),
                a["texte"].as_str().unwrap_or("").to_string(),
            )),
            "pc" => a["outil"].as_str().filter(|o| !o.is_empty()).map(|o| AiAction::Pc(o.to_string(), a["args"].clone())),
            _ => None,
        };
        acts.extend(act);
    }
    let text = if text.is_empty() && !acts.is_empty() { "C'est fait.".to_string() } else { text };
    (text, acts)
}

/// « 7h30 », « 07:30 », « 7 h » → (7, 30).
pub fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    let s = s.trim().to_lowercase().replace(' ', "").replace('h', ":");
    let (h, m) = s.split_once(':').unwrap_or((&s, "0"));
    let h: u32 = h.parse().ok()?;
    let m: u32 = if m.is_empty() { 0 } else { m.parse().ok()? };
    (h < 24 && m < 60).then_some((h, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reponse_avec_actions() {
        let (t, a) = parse(r#"{"reponse":"Minuteur lancé.","actions":[{"type":"minuteur","minutes":5,"nom":"pâtes"},{"type":"alarme","heure":"7h30"}]}"#, true);
        assert_eq!(t, "Minuteur lancé.");
        assert_eq!(a, vec![AiAction::Timer(300, "pâtes".into()), AiAction::Alarm(7, 30)]);
    }

    #[test]
    fn texte_simple_et_json_entoure() {
        assert_eq!(parse("Bonjour !", true).0, "Bonjour !");
        let (t, a) = parse("```json\n{\"reponse\":\"Ok\",\"actions\":[{\"type\":\"chrono\",\"commande\":\"pause\"}]}\n```", true);
        assert_eq!(t, "Ok");
        assert_eq!(a, vec![AiAction::Chrono("pause".into())]);
    }

    #[test]
    fn heures() {
        assert_eq!(parse_hhmm("07:05"), Some((7, 5)));
        assert_eq!(parse_hhmm("18h"), Some((18, 0)));
        assert_eq!(parse_hhmm("25:00"), None);
    }
}
