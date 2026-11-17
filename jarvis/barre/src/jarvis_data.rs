//! Données des sections de Jarvis (tâches, rappels, habitudes, copiés, routines, mémoire, musique,
//! appareils, PC, paramètres) lues sur son API locale http://127.0.0.1:47801/api/<nom>.
//! Chaque lecture ou action se fait dans un fil à part ; le résultat revient par Msg::JData.

use std::cell::Cell;
use std::collections::HashMap;
use std::time::Instant;

use serde_json::Value;

use crate::data::{Msg, Out};
use crate::island::View;

/// Dernière réponse de chaque API, ou l'erreur.
#[derive(Default)]
pub struct JData {
    pub v: HashMap<String, Value>,
    pub err: HashMap<String, String>,
    pub at: HashMap<String, Instant>,
}

impl JData {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.v.get(name)
    }
    /// Texte à afficher si la section n'a pas (encore) de données.
    pub fn missing(&self, name: &str) -> Option<&'static str> {
        if self.v.contains_key(name) {
            None
        } else if self.err.contains_key(name) {
            Some("Jarvis n'est pas lancé.")
        } else {
            Some("Chargement…")
        }
    }
    pub fn set(&mut self, name: &str, r: Result<Value, String>) {
        match r {
            Ok(v) if v["ok"].as_bool() != Some(false) || !self.v.contains_key(name) => {
                self.err.remove(name);
                self.v.insert(name.to_string(), v);
            }
            Ok(_) => {}
            Err(e) => {
                self.err.insert(name.to_string(), e);
            }
        }
        self.at.insert(name.to_string(), Instant::now());
    }
}

/// Nom de l'API de Jarvis pour une page.
pub fn api_name(v: View) -> Option<&'static str> {
    Some(match v {
        View::Tasks => "taches",
        View::Reminders => "rappels",
        View::Habits => "habitudes",
        View::Clips => "copies",
        View::Routines => "routines",
        View::Memory => "memoire",
        View::Music => "musique",
        View::Batteries => "appareils",
        View::Pc => "pc",
        View::JSettings => "parametres",
        _ => return None,
    })
}

/// Section de la « grande île » d'après le nom donné à /ouvrir (inconnue = accueil).
pub fn section(name: &str) -> View {
    match name.trim().to_lowercase().as_str() {
        "taches" | "tâches" => View::Tasks,
        "rappels" => View::Reminders,
        "habitudes" => View::Habits,
        "copies" | "copiés" => View::Clips,
        "routines" => View::Routines,
        "memoire" | "mémoire" => View::Memory,
        "musique" => View::Music,
        "appareils" | "batteries" => View::Batteries,
        "pc" => View::Pc,
        "telephone" | "téléphone" => View::Phone,
        "pronote" => View::School,
        "mails" => View::Mails,
        "discord" => View::Discord,
        "minuteurs" => View::Timers,
        "parametres" | "paramètres" => View::JSettings,
        "comptes" => View::Accounts,
        "jarvis" | "chat" => View::Chat,
        _ => View::Main,
    }
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(std::time::Duration::from_secs(2)).timeout(std::time::Duration::from_secs(secs)).build()
}

/// Lit une API (GET) dans un fil à part.
pub fn fetch(out: &Out, name: &str) {
    let (out, name) = (out.clone(), name.to_string());
    std::thread::spawn(move || {
        let r = agent(8)
            .get(&format!("{}/api/{name}", crate::jarvis_link::BASE))
            .call()
            .map_err(|e| e.to_string())
            .and_then(|x| x.into_json::<Value>().map_err(|e| e.to_string()));
        out.send(Msg::JData(name, r));
    });
}

/// Action (POST JSON) dans un fil à part ; la réponse remplace les données (sinon on relit).
pub fn act(out: &Out, name: &str, body: Value) {
    let (out, name) = (out.clone(), name.to_string());
    std::thread::spawn(move || {
        let r = crate::jarvis_link::post(&format!("/api/{name}"), body, 30);
        let full = matches!(&r, Ok(v) if v.as_object().is_some_and(|o| o.len() > 2));
        if full {
            out.send(Msg::JData(name, r));
        } else {
            if let Err(e) = &r {
                crate::sys::log(&format!("Jarvis /api/{name} : {e}"));
            }
            let g = agent(8)
                .get(&format!("{}/api/{name}", crate::jarvis_link::BASE))
                .call()
                .map_err(|e| e.to_string())
                .and_then(|x| x.into_json::<Value>().map_err(|e| e.to_string()));
            out.send(Msg::JData(name, g));
        }
    });
}

pub fn arr<'a>(v: Option<&'a Value>, key: &str) -> Vec<&'a Value> {
    v.and_then(|v| v[key].as_array()).map(|a| a.iter().collect()).unwrap_or_default()
}

pub fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

// ---------- Carte mentale (mémoire) ----------

/// Positions des souvenirs (placement par forces), vue (décalage, zoom) et sélection.
pub struct MemGraph {
    pub pos: HashMap<String, [f32; 4]>,
    pub sel: Option<String>,
    pub pan: (f32, f32),
    pub zoom: f32,
    pub drag: Option<(Option<String>, f32, f32, bool)>,
    pub moving: bool,
    /// Origine de la page à l'écran (pixels de la fenêtre) et échelle, mémorisées au dessin.
    pub view: Cell<(f32, f32, f32, f32, f32)>,
}

impl Default for MemGraph {
    fn default() -> Self {
        MemGraph { pos: HashMap::new(), sel: None, pan: (0.0, 0.0), zoom: 1.0, drag: None, moving: true, view: Cell::new((0.0, 0.0, 1.0, 1.0, 1.0)) }
    }
}

fn seed(id: &str) -> f32 {
    id.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) as f32 / u32::MAX as f32
}

impl MemGraph {
    /// Ajoute les nouveaux souvenirs (autour du centre), retire ceux qui ont disparu.
    pub fn sync(&mut self, v: Option<&Value>) {
        let nodes = arr(v, "noeuds");
        let ids: Vec<String> = nodes.iter().map(|n| s(n, "id").to_string()).collect();
        self.pos.retain(|k, _| ids.contains(k));
        for (i, id) in ids.iter().enumerate() {
            self.pos.entry(id.clone()).or_insert_with(|| {
                if id == "moi" {
                    return [0.0, 0.0, 0.0, 0.0];
                }
                // angle propre à chaque souvenir, sur deux cercles en alternance
                let n = ids.len().max(2) as f32;
                let a = (i as f32 / n + seed(id) * 0.02) * std::f32::consts::TAU;
                let r = if i % 2 == 0 { 190.0 } else { 290.0 };
                [a.cos() * r, a.sin() * r, 0.0, 0.0]
            });
        }
        if self.sel.as_ref().is_some_and(|s| !ids.contains(s)) {
            self.sel = None;
        }
        self.moving = true;
    }

    /// Un pas du placement par forces. Renvoie `true` tant que ça bouge.
    pub fn step(&mut self, v: Option<&Value>) -> bool {
        let links: Vec<(String, String)> = arr(v, "liens").iter().map(|l| (s(l, "a").to_string(), s(l, "b").to_string())).collect();
        let ids: Vec<String> = self.pos.keys().cloned().collect();
        let held = self.drag.as_ref().and_then(|d| d.0.clone());
        let mut f: HashMap<&str, (f32, f32)> = ids.iter().map(|i| (i.as_str(), (0.0, 0.0))).collect();
        for (i, a) in ids.iter().enumerate() {
            for b in ids.iter().skip(i + 1) {
                let (pa, pb) = (self.pos[a], self.pos[b]);
                let (dx, dy) = (pa[0] - pb[0], pa[1] - pb[1]);
                let d2 = (dx * dx + dy * dy).max(25.0);
                let k = 9000.0 / d2;
                let d = d2.sqrt();
                let (fx, fy) = (dx / d * k, dy / d * k);
                let e = f.get_mut(a.as_str()).unwrap();
                e.0 += fx;
                e.1 += fy;
                let e = f.get_mut(b.as_str()).unwrap();
                e.0 -= fx;
                e.1 -= fy;
            }
        }
        for (a, b) in &links {
            let (Some(pa), Some(pb)) = (self.pos.get(a), self.pos.get(b)) else { continue };
            let (dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
            let d = (dx * dx + dy * dy).sqrt().max(1.0);
            let k = (d - 210.0) * 0.02;
            let (fx, fy) = (dx / d * k, dy / d * k);
            if let Some(e) = f.get_mut(a.as_str()) {
                e.0 += fx;
                e.1 += fy;
            }
            if let Some(e) = f.get_mut(b.as_str()) {
                e.0 -= fx;
                e.1 -= fy;
            }
        }
        let mut moved = 0.0;
        for id in &ids {
            if Some(id) == held.as_ref() {
                continue;
            }
            let (fx, fy) = f[id.as_str()];
            let p = self.pos.get_mut(id).unwrap();
            if id == "moi" {
                p[0] *= 0.9;
                p[1] *= 0.9;
            }
            p[2] = (p[2] + fx - p[0] * 0.002) * 0.82;
            p[3] = (p[3] + fy - p[1] * 0.002) * 0.82;
            p[2] = p[2].clamp(-20.0, 20.0);
            p[3] = p[3].clamp(-20.0, 20.0);
            p[0] += p[2];
            p[1] += p[3];
            moved += p[2].abs() + p[3].abs();
        }
        self.moving = moved > 0.3 * ids.len().max(1) as f32 || held.is_some();
        self.moving
    }

    /// Point de la fenêtre (pixels) → coordonnées du monde de la carte.
    pub fn to_world(&self, px: f32, py: f32) -> (f32, f32) {
        let (ox, oy, sc, w, h) = self.view.get();
        let (lx, ly) = ((px - ox) / sc - w / 2.0, (py - oy) / sc - h / 2.0);
        ((lx - self.pan.0) / self.zoom, (ly - self.pan.1) / self.zoom)
    }

    /// Souvenir sous le point (pixels de la fenêtre).
    pub fn node_at(&self, px: f32, py: f32) -> Option<String> {
        let (wx, wy) = self.to_world(px, py);
        let r = 16.0 / self.zoom;
        self.pos.iter().find(|(_, p)| (p[0] - wx).powi(2) + (p[1] - wy).powi(2) < r * r * 1.6).map(|(k, _)| k.clone())
    }

    pub fn inside(&self, px: f32, py: f32) -> bool {
        let (ox, oy, sc, w, h) = self.view.get();
        px >= ox && py >= oy && px < ox + w * sc && py < oy + h * sc
    }
}

/// Valeur suivante d'un réglage numérique de Jarvis.
pub fn step_number(v: f64, dir: f64) -> f64 {
    let st = if v.abs() < 3.0 && v.fract().abs() > 1e-9 || v.abs() < 1.5 { 0.1 } else { 1.0 };
    ((v + st * dir) * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sections_et_carte() {
        assert_eq!(section("taches"), View::Tasks);
        assert_eq!(section("inconnue"), View::Main);
        assert_eq!(section("appareils"), View::Batteries);
        let v = json!({"noeuds": [{"id": "moi"}, {"id": "a"}, {"id": "b"}], "liens": [{"a": "moi", "b": "a"}]});
        let mut g = MemGraph::default();
        g.sync(Some(&v));
        assert_eq!(g.pos.len(), 3);
        for _ in 0..400 {
            if !g.step(Some(&v)) {
                break;
            }
        }
        let (pa, pb) = (g.pos["moi"], g.pos["a"]);
        let d = ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2)).sqrt();
        assert!(d > 80.0 && d < 500.0, "distance {d}");
        assert_eq!(step_number(0.5, 1.0), 0.6);
        assert_eq!(step_number(30.0, -1.0), 29.0);
    }
}
