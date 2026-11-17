//! `givre.exe --apercu DOSSIER` : dessine les widgets sur une copie du fond d'écran (images PNG),
//! sans rien afficher à l'écran. Sert à vérifier le rendu.

use std::time::Instant;

use tiny_skia::{IntRect, IntSize, Pixmap, PixmapPaint, Transform};

use crate::backdrop::{self, Backdrop};
use crate::config::Config;
use crate::data::{self, Data};
use crate::gfx::{Fonts, Theme};
use crate::settings::{self, Settings};
use crate::sys;
use crate::widgets::{self, Look};

pub fn run(out: &str) {
    let out = std::path::PathBuf::from(out);
    let _ = std::fs::create_dir_all(&out);
    let mut log = String::new();
    let Some(fonts) = Fonts::load() else {
        let _ = std::fs::write(out.join("log.txt"), "polices introuvables");
        return;
    };
    let cfg = Config::default();
    let ag = data::agent();
    let mut d = Data::default();
    match data::fetch_weather(&ag, cfg.weather.lat, cfg.weather.lon) {
        Ok(w) => d.weather = Some(w),
        Err(e) => log.push_str(&format!("météo : {e}\n")),
    }
    match data::fetch_holidays(&ag, &cfg.countdown.academie) {
        Ok(h) => {
            log.push_str(&format!("{} périodes de vacances\n", h.len()));
            d.holidays = h;
        }
        Err(e) => log.push_str(&format!("vacances : {e}\n")),
    }
    d.pronote = crate::pronote::demo();
    log.push_str(&format!("fenêtre qui recouvre tout le bureau : {:?}\n", sys::covering_window()));
    let (sw, sh) = sys::screen_size();
    let dpi = sys::dpi();
    let t0 = Instant::now();
    let bd = Backdrop::load(sw, sh, cfg.blur_amount);
    log.push_str(&format!("fond d'écran préparé en {:?}\n", t0.elapsed()));
    let places = crate::default_places(&cfg, dpi);
    let now = chrono::Local::now().naive_local();
    for (name, dark) in [("sombre", true), ("clair", false)] {
        let theme = Theme::new(dark, cfg.accent, cfg.opacity);
        let look = Look { cfg: &cfg, fonts: &fonts, theme: &theme, backdrop: Some(&bd), dpi, edit: false, sel: None };
        let full = backdrop::compose(sw as u32, sh as u32, sw as u32);
        let Some(mut desk) = IntSize::from_wh(sw as u32, sh as u32).and_then(|s| Pixmap::from_vec(full, s)) else {
            continue;
        };
        let t1 = Instant::now();
        for (k, p) in &places {
            if let Some((pm, m, _)) = widgets::render(*k, &look, &d, *p, 0, now) {
                desk.draw_pixmap(p.x - m, p.y - m, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
            }
        }
        log.push_str(&format!("rendu des {} widgets ({name}) : {:?}\n", places.len(), t1.elapsed()));
        // calendrier avec le jour de demain choisi
        let look2 = Look { sel: Some(now.date() + chrono::Duration::days(1)), ..look };
        if let Some((pm, _, _)) = widgets::render(widgets::Kind::Calendar, &look2, &d, places[3].1, 0, now) {
            let _ = pm.save_png(out.join(format!("calendrier_jour_{name}.png")));
        }
        let x0 = (places.iter().map(|(_, p)| p.x).min().unwrap_or(0) - 40).max(0);
        if let Some(crop) = IntRect::from_xywh(x0, 0, (sw - x0) as u32, (sh * 2 / 3) as u32).and_then(|r| desk.clone_rect(r)) {
            let _ = crop.save_png(out.join(format!("zoom_{name}.png")));
        }
        let _ = desk.save_png(out.join(format!("bureau_{name}.png")));
    }
    let theme = Theme::new(true, cfg.accent, cfg.opacity);
    let pn = settings::PnInfo { connected: false, ..Default::default() };
    let info = settings::Info { pn, edit: false, mem: sys::mem_text(), status: String::new(), acc: Default::default(), discord: String::new(), winnotif: String::new() };
    for page in 0..5 {
        let mut st = Settings::new(std::ptr::null_mut());
        st.set_page(page);
        if let Some(pm) = settings::draw(&mut st, &cfg, &fonts, &theme, dpi, &info) {
            let _ = pm.save_png(out.join(format!("parametres_{page}.png")));
        }
    }
    log.push_str(&format!("{}\n", sys::mem_text()));
    let _ = std::fs::write(out.join("log.txt"), log);
}

