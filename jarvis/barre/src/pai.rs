//! PAI : l'emploi du temps aménagé de l'utilisateur, en plus de celui de Pronote.
//!
//! Pronote donne l'emploi du temps de toute la classe. Avec un PAI, l'élève ne va pas à tous
//! ces cours. Ici on marque les cours où il ne va pas (`Lesson::manque`), on ajoute ses propres
//! créneaux (`Lesson::perso`), et on range cours et devoirs dans l'ordre de la journée
//! (le cours de 8 h d'abord), jamais par ordre alphabétique.
//!
//! Réglages : Paramètres › Agenda › Mon emploi du temps.
//! - Un texte par jour, du lundi au dimanche :
//!   « 8h-12h30 » = je suis en cours seulement sur ces heures ;
//!   « 8h-12h ; 16h-17h Orthophoniste » = pareil, plus un créneau à moi ;
//!   « aucun » = pas de cours ce jour-là ; vide = comme Pronote.
//! - Les matières où je ne vais jamais : « Espagnol, Latin ».
//! - Les cours précis décochés ou recochés à la voix (« je ne vais pas en maths demain »).
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::pronote::{Lesson, School};

pub const JOURS: [&str; 7] = ["Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche"];

#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Debug)]
#[serde(default)]
pub struct Pai {
    /// Lundi → dimanche (voir l'en-tête du fichier).
    pub jours: [String; 7],
    /// Matières où je ne vais jamais, séparées par des virgules.
    pub sans: String,
    /// Cours précis décochés à la main (clé `cle`).
    pub sautes: Vec<String>,
    /// Cours précis recochés à la main alors que le PAI les enlevait.
    pub remis: Vec<String>,
}

impl Pai {
    pub fn vide(&self) -> bool {
        self.jours.iter().all(|j| j.trim().is_empty()) && self.sans.trim().is_empty() && self.sautes.is_empty()
    }
}

/// Copie des réglages pour le fil de Pronote (notifications, Google Agenda).
static ACTUEL: Mutex<Option<Pai>> = Mutex::new(None);

pub fn regler(p: &Pai) {
    *ACTUEL.lock().unwrap_or_else(|e| e.into_inner()) = Some(p.clone());
}

pub fn actuel() -> Pai {
    ACTUEL.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// « 8h », « 8h30 », « 08:30 », « 8 » → 08:00 / 08:30.
pub fn lire_heure(s: &str) -> Option<NaiveTime> {
    heure(s)
}

/// « aujourd'hui », « demain », « après-demain », « lundi » (le prochain, aujourd'hui compris), « 24/11 », « 2026-11-24 ».
pub fn lire_date(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let t = simple(s);
    if t.is_empty() || t.contains("aujourd") || t == "ce matin" || t.contains("cet apres") || t == "ce soir" {
        return Some(today);
    }
    if t.contains("apres demain") {
        return Some(today + Duration::days(2));
    }
    if t.contains("demain") {
        return Some(today + Duration::days(1));
    }
    if let Some(i) = JOURS.iter().position(|j| t.contains(&simple(j))) {
        let ecart = (i as i64 - today.weekday().num_days_from_monday() as i64).rem_euclid(7);
        return Some(today + Duration::days(ecart));
    }
    if let Ok(d) = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
        return Some(d);
    }
    let n: Vec<u32> = s.split(|c: char| !c.is_ascii_digit()).filter_map(|x| x.parse().ok()).collect();
    if n.len() >= 2 {
        let an = n.get(2).map(|a| if *a < 100 { 2000 + *a as i32 } else { *a as i32 }).unwrap_or(today.year());
        return NaiveDate::from_ymd_opt(an, n[1], n[0]);
    }
    None
}

fn heure(s: &str) -> Option<NaiveTime> {
    let s = s.trim().to_lowercase().replace(' ', "");
    let (h, m) = match s.find(['h', ':']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s.as_str(), ""),
    };
    let h: u32 = h.parse().ok()?;
    let m: u32 = if m.is_empty() { 0 } else { m.parse().ok()? };
    NaiveTime::from_hms_opt(h, m, 0)
}

/// Ce que dit le texte d'un jour.
#[derive(Debug, PartialEq, Default)]
pub struct Jour {
    /// Heures où je suis en cours. Vide = tous les cours de Pronote.
    pub plages: Vec<(NaiveTime, NaiveTime)>,
    /// Mes créneaux à moi : (début, fin, nom).
    pub perso: Vec<(NaiveTime, NaiveTime, String)>,
    /// « aucun » : pas de cours du tout ce jour-là.
    pub aucun: bool,
}

/// None = texte vide : ce jour-là, c'est comme Pronote.
pub fn lire_jour(s: &str) -> Option<Jour> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let low = simple(t);
    let mut j = Jour::default();
    if ["aucun", "rien", "pas cours", "pas de cours", "personne", "jamais"].iter().any(|k| low.starts_with(k)) {
        j.aucun = true;
        return Some(j);
    }
    for morceau in t.split([';', ',', '\n']) {
        let m = morceau.trim();
        if m.is_empty() {
            continue;
        }
        let norm = m.replace(['–', '—'], "-").replace(" à ", "-").replace(" - ", "-").replace("- ", "-").replace(" -", "-");
        let (plage, nom) = match norm.split_once(char::is_whitespace) {
            Some((a, b)) => (a.to_string(), b.trim().to_string()),
            None => (norm.clone(), String::new()),
        };
        let Some((a, b)) = plage.split_once('-') else { continue };
        let (Some(a), Some(b)) = (heure(a), heure(b)) else { continue };
        if b <= a {
            continue;
        }
        if nom.is_empty() {
            j.plages.push((a, b));
        } else {
            j.perso.push((a, b, nom));
        }
    }
    Some(j)
}

/// Minuscules, sans accents ni ponctuation : « Mathématiques » → « mathematiques ».
pub fn simple(s: &str) -> String {
    let t: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        })
        .collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// « Maths » et « MATHEMATIQUES », « Anglais » et « ANGLAIS LV1 » : même matière.
pub fn meme_matiere(a: &str, b: &str) -> bool {
    let (a, b) = (simple(a), simple(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let commun = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    a == b || a.starts_with(&b) || b.starts_with(&a) || commun >= 4
}

/// Pour comparer malgré les fautes : « phisic » et « physique » → « fisic », « fisique ».
fn flou(mot: &str) -> String {
    simple(mot).replace("ph", "f").replace('y', "i").replace("qu", "c").replace('k', "c")
}

/// Les mots tapés dans « matières où je ne vais pas » : séparés par des virgules, ou sinon par des espaces.
fn mots_sans(p: &Pai) -> Vec<String> {
    let t = p.sans.trim();
    let morceaux: Vec<&str> = if t.contains(',') { t.split(',').collect() } else { t.split_whitespace().collect() };
    morceaux.iter().map(|m| flou(m)).filter(|m| m.chars().count() >= 3).collect()
}

/// La matière est dans « matières où je ne vais pas ».
pub fn sans_touche(p: &Pai, matiere: &str) -> bool {
    let mots_cours: Vec<String> = flou(matiere).split_whitespace().map(String::from).collect();
    mots_sans(p).iter().any(|m| {
        let mots_tape: Vec<&str> = m.split_whitespace().collect();
        mots_tape.iter().all(|k| {
            mots_cours.iter().any(|w| {
                let commun = k.chars().zip(w.chars()).take_while(|(a, b)| a == b).count();
                w.starts_with(k) || (w.len() >= 4 && k.starts_with(w.as_str())) || commun >= 4
            })
        })
    })
}

/// Les matières de Pronote que « matières où je ne vais pas » enlève (pour les montrer dans les paramètres).
pub fn matieres_sans(p: &Pai, sc: &School) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for l in sc.lessons.iter().filter(|l| !l.perso) {
        if sans_touche(p, &l.subject) && !v.contains(&l.subject) {
            v.push(l.subject.clone());
        }
    }
    v
}

/// Clé d'un cours précis : « 2026-11-23 14:00 mathematiques ».
pub fn cle(l: &Lesson) -> String {
    format!("{} {}", l.start.format("%Y-%m-%d %H:%M"), simple(&l.subject))
}

/// Pourquoi je ne vais pas à ce cours (None = j'y vais).
pub fn pourquoi(p: &Pai, l: &Lesson) -> Option<&'static str> {
    if l.perso {
        return None;
    }
    let k = cle(l);
    if p.remis.contains(&k) {
        return None;
    }
    if p.sautes.contains(&k) {
        return Some("décoché à la main");
    }
    if sans_touche(p, &l.subject) {
        return Some("matière où tu ne vas pas");
    }
    let jour = l.start.weekday().num_days_from_monday() as usize;
    if let Some(j) = lire_jour(&p.jours[jour]) {
        if j.aucun {
            return Some("PAI : pas de cours ce jour-là");
        }
        if !j.plages.is_empty() {
            let milieu = (l.start + (l.end - l.start) / 2).time();
            if !j.plages.iter().any(|(a, b)| *a <= milieu && milieu < *b) {
                return Some("PAI : en dehors de tes heures de cours");
            }
        }
    }
    None
}

/// Marque les cours manqués, ajoute mes créneaux (15 jours) et range tout dans l'ordre horaire.
/// Peut être rappelé autant de fois qu'on veut (après un changement de réglage).
pub fn appliquer(p: &Pai, sc: &mut School, today: NaiveDate) {
    sc.lessons.retain(|l| !l.perso);
    for l in sc.lessons.iter_mut() {
        l.manque = pourquoi(p, l).unwrap_or("").to_string();
    }
    for d in 0..15 {
        let date = today + Duration::days(d);
        let Some(j) = lire_jour(&p.jours[date.weekday().num_days_from_monday() as usize]) else { continue };
        for (a, b, nom) in j.perso {
            sc.lessons.push(Lesson {
                id: format!("perso:{date} {a}"),
                start: date.and_time(a),
                end: date.and_time(b),
                subject: nom,
                color: [142, 142, 147],
                perso: true,
                ..Default::default()
            });
        }
    }
    sc.lessons.sort_by_key(|l| (l.start, l.bad(), !l.manque.is_empty()));
    ranger_devoirs(sc);
}

/// Heure du premier cours de cette matière ce jour-là.
fn heure_du_cours(lessons: &[Lesson], jour: NaiveDate, matiere: &str) -> Option<NaiveTime> {
    lessons.iter().filter(|l| !l.perso && l.start.date() == jour && meme_matiere(&l.subject, matiere)).map(|l| l.start.time()).min()
}

/// Devoirs d'un même jour : dans l'ordre des cours de ce jour-là (8 h d'abord).
/// Un devoir sans cours ce jour-là passe en dernier.
pub fn ranger_devoirs(sc: &mut School) {
    let lessons = &sc.lessons;
    let fin = NaiveTime::from_hms_opt(23, 59, 0).unwrap_or_default();
    let mut v = std::mem::take(&mut sc.homework);
    v.sort_by_cached_key(|h| (h.due, heure_du_cours(lessons, h.due, &h.subject).unwrap_or(fin)));
    sc.homework = v;
}

/// Pour un devoir : Some(false) si je ne vais à aucun cours de cette matière ce jour-là,
/// Some(true) si j'y vais, None s'il n'y a pas de cours de cette matière.
pub fn va_au_cours(sc: &School, jour: NaiveDate, matiere: &str) -> Option<bool> {
    let v: Vec<&Lesson> = sc.lessons.iter().filter(|l| !l.perso && l.start.date() == jour && meme_matiere(&l.subject, matiere)).collect();
    if v.is_empty() {
        None
    } else {
        Some(v.iter().any(|l| l.manque.is_empty()))
    }
}

/// « Je ne vais pas en maths demain » (sauter = true) ou « finalement j'y vais » (false).
/// Matière vide = tous les cours du jour ; heure = seulement le cours de cette heure-là.
/// Rend les cours touchés (« MATHEMATIQUES à 14:00 »).
pub fn basculer(p: &mut Pai, sc: &School, jour: NaiveDate, matiere: &str, h: Option<NaiveTime>, sauter: bool) -> Vec<String> {
    let mut faits = Vec::new();
    for l in sc.lessons.iter().filter(|l| {
        !l.perso
            && l.start.date() == jour
            && (matiere.trim().is_empty() || meme_matiere(&l.subject, matiere))
            && h.map_or(true, |h| l.start.time() <= h && h < l.end.time())
    }) {
        let k = cle(l);
        p.sautes.retain(|x| x != &k);
        p.remis.retain(|x| x != &k);
        if sauter {
            p.sautes.push(k);
        } else {
            p.remis.push(k);
        }
        faits.push(format!("{} à {}", l.subject, l.start.format("%H:%M")));
    }
    // les cours de plus d'un mois sont oubliés
    let limite = (Local::now().date_naive() - Duration::days(31)).format("%Y-%m-%d").to_string();
    p.sautes.retain(|k| k.get(..10).unwrap_or("") >= limite.as_str());
    p.remis.retain(|k| k.get(..10).unwrap_or("") >= limite.as_str());
    faits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pronote::Homework;

    fn cours(j: u32, h: u32, sub: &str) -> Lesson {
        let d = NaiveDate::from_ymd_opt(2026, 11, 23 + j).unwrap(); // lundi 23
        Lesson { id: format!("{j}{h}{sub}"), start: d.and_hms_opt(h, 0, 0).unwrap(), end: d.and_hms_opt(h, 55, 0).unwrap(), subject: sub.into(), ..Default::default() }
    }

    #[test]
    fn lire_un_jour() {
        assert_eq!(lire_jour("  "), None);
        assert!(lire_jour("aucun").unwrap().aucun);
        let j = lire_jour("8h-12h30 ; 16h - 17h Orthophoniste").unwrap();
        assert_eq!(j.plages, vec![(heure("8h").unwrap(), heure("12h30").unwrap())]);
        assert_eq!(j.perso.len(), 1);
        assert_eq!(j.perso[0].2, "Orthophoniste");
        assert_eq!(lire_jour("8:00 à 12:00").unwrap().plages.len(), 1);
    }

    #[test]
    fn pai_du_matin() {
        let mut p = Pai::default();
        p.jours[0] = "8h-12h".into();
        p.jours[2] = "aucun".into();
        p.sans = "Espagnol".into();
        assert_eq!(pourquoi(&p, &cours(0, 9, "MATHEMATIQUES")), None);
        assert!(pourquoi(&p, &cours(0, 14, "MATHEMATIQUES")).is_some());
        assert!(pourquoi(&p, &cours(1, 14, "ESPAGNOL LV2")).is_some());
        p.sans = "music art phisic".into();
        assert!(sans_touche(&p, "EDUCATION MUSICALE") && sans_touche(&p, "ARTS PLASTIQUES") && sans_touche(&p, "PHYSIQUE-CHIMIE"));
        assert!(!sans_touche(&p, "MATHEMATIQUES") && !sans_touche(&p, "FRANCAIS"));
        p.sans = "Espagnol".into();
        assert_eq!(pourquoi(&p, &cours(1, 14, "FRANCAIS")), None);
        assert!(pourquoi(&p, &cours(2, 8, "FRANCAIS")).is_some());
    }

    #[test]
    fn ordre_horaire_et_devoirs() {
        let mut sc = School {
            lessons: vec![cours(0, 14, "SVT"), cours(0, 8, "MATHEMATIQUES"), cours(0, 10, "ANGLAIS LV1")],
            ..Default::default()
        };
        let lundi = sc.lessons[0].start.date();
        let hw = |s: &str| Homework { id: s.into(), subject: s.into(), due: lundi, ..Default::default() };
        sc.homework = vec![hw("ANGLAIS LV1"), hw("HISTOIRE"), hw("MATHEMATIQUES"), hw("SVT")];
        let mut p = Pai::default();
        p.jours[0] = "8h-12h ; 16h-17h Kiné".into();
        appliquer(&p, &mut sc, lundi);
        let noms: Vec<&str> = sc.lessons_on(lundi).iter().map(|l| l.subject.as_str()).collect();
        assert_eq!(noms, ["MATHEMATIQUES", "ANGLAIS LV1", "SVT", "Kiné"]);
        let dv: Vec<&str> = sc.homework.iter().map(|h| h.subject.as_str()).collect();
        assert_eq!(dv, ["MATHEMATIQUES", "ANGLAIS LV1", "SVT", "HISTOIRE"]);
        assert_eq!(va_au_cours(&sc, lundi, "SVT"), Some(false));
        assert_eq!(va_au_cours(&sc, lundi, "Maths"), Some(true));
        // appliquer deux fois ne double pas mes créneaux
        appliquer(&p, &mut sc, lundi);
        assert_eq!(sc.lessons_on(lundi).len(), 4);
        // « finalement je vais en SVT lundi »
        let faits = basculer(&mut p, &sc, lundi, "svt", None, false);
        assert_eq!(faits.len(), 1);
        appliquer(&p, &mut sc, lundi);
        assert_eq!(va_au_cours(&sc, lundi, "SVT"), Some(true));
    }
}
