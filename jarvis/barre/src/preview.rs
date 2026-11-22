//! `givre.exe --apercu DOSSIER` : dessine les widgets sur une copie du fond d'écran (images PNG),
//! sans rien afficher à l'écran. Sert à vérifier le rendu.

use std::time::Instant;

use tiny_skia::{IntRect, IntSize, Pixmap, PixmapPaint, Transform};

use crate::backdrop::{self, Backdrop};
use crate::config::Config;
use crate::data::{self, Data};
use crate::gfx::{Fonts, Theme};
use crate::settings;
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
        let look = Look { cfg: &cfg, fonts: &fonts, theme: &theme, backdrop: Some(&bd), dpi, sel: None, edit: false };
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
    log.push_str(&format!("{}\n", sys::mem_text()));
    let _ = std::fs::write(out.join("log.txt"), log);
}

/// `givre.exe --apercu-ile DOSSIER` : images de la Dynamic Island (tous ses écrans et ses réglages),
/// avec des données d'exemple, sans rien afficher à l'écran.
pub fn run_ile(out: &str) {
    use crate::data::{Cat, Mail, Notif};
    use crate::island::{self, Chat, Clocks, INotif, Island, Jarvis, Mode, Ring, Src, View};
    use crate::island_draw::{self, R};
    let out = std::path::PathBuf::from(out);
    let _ = std::fs::create_dir_all(&out);
    let mut log = String::new();
    let Some(fonts) = Fonts::load() else {
        let _ = std::fs::write(out.join("log.txt"), "polices introuvables");
        return;
    };
    let mut cfg = Config::default();
    cfg.ile.on = true;
    cfg.ile.alarm_on = true;
    cfg.ile.alarm_h = 7;
    cfg.ile.alarm_m = 30;
    let mut d = Data::default();
    match data::fetch_weather(&data::agent(), cfg.weather.lat, cfg.weather.lon) {
        Ok(w) => d.weather = Some(w),
        Err(e) => log.push_str(&format!("météo : {e}\n")),
    }
    d.pronote = crate::pronote::demo();
    let now = chrono::Local::now().naive_local();
    let mail = |from: &str, subject: &str, snippet: &str, cat: Cat, urgent: bool| Mail {
        id: String::new(),
        from: from.into(),
        subject: subject.into(),
        snippet: snippet.into(),
        date: now - chrono::Duration::minutes(42),
        important: true,
        urgent,
        cat,
        link: String::new(),
        find: String::new(),
    };
    d.mail.accounts = 1;
    d.mail.summary = "Le collège attend l'autorisation de sortie pour lundi, et Maman te rappelle le dentiste mercredi.".into();
    d.mail.mails = vec![
        mail("Collège Jean Moulin", "Sortie au musée : autorisation à rendre lundi", "Bonjour, merci de rendre l'autorisation signée avant lundi 8 h.", Cat::Important, true),
        mail("Lucas", "Tu viens samedi ?", "On fait un tournoi chez moi, ramène ta manette !", Cat::Reply, false),
        mail("Steam", "Soldes d'automne", "Jusqu'à -80 %", Cat::Promo, false),
        mail("Maman", "Rendez-vous dentiste mercredi", "N'oublie pas, 14 h 30.", Cat::Important, false),
    ];
    d.discord = vec![
        Notif { title: "Lucas".into(), body: "t'es chaud pour une game ce soir ?".into(), time: now - chrono::Duration::minutes(12) },
        Notif { title: "Inès (#maths)".into(), body: "quelqu'un a fait l'exo 4 p. 52 ? je comprends pas la question b".into(), time: now - chrono::Duration::minutes(3) },
    ];
    d.accounts.ai = "En service : LM Studio · qwen2.5:7b.".into();
    let (sw, sh) = sys::screen_size();
    let dpi = sys::dpi();
    let bd = Backdrop::load(sw, sh, cfg.ile.blur_amount);
    let mut clocks = Clocks::default();
    clocks.add_timer(5 * 60, "pâtes");
    clocks.add_timer(25 * 60, "révisions");
    clocks.chrono_acc = std::time::Duration::from_millis(83_400);
    let mut chat = Chat::default();
    chat.log.push(("J'ai quoi comme devoirs pour demain ?".into(), "Pour demain, tu as l'exercice 4 page 52 en maths et une leçon d'histoire à apprendre.".into()));
    chat.log.push(("Lance un minuteur de 5 minutes pour les pâtes".into(), "C'est parti, minuteur de 5 minutes lancé pour les pâtes.".into()));
    chat.log.push(("Il va pleuvoir demain ?".into(), "Non, demain ce sera plutôt ensoleillé, entre 4 et 13 degrés.".into()));
    let notifs = vec![
        INotif { id: 1, kind: String::new(), src: Src::Pronote, title: "Nouvelle note".into(), body: "Mathématiques : 16,5/20".into(), time: now - chrono::Duration::minutes(50) },
        INotif { id: 2, kind: String::new(), src: Src::Mail, title: "Mail urgent — Collège Jean Moulin".into(), body: "Sortie au musée : autorisation à rendre lundi".into(), time: now - chrono::Duration::minutes(42) },
        INotif { id: 3, kind: String::new(), src: Src::Discord, title: "Lucas".into(), body: "t'es chaud pour une game ce soir ?".into(), time: now - chrono::Duration::minutes(12) },
        INotif { id: 4, kind: String::new(), src: Src::Jarvis, title: "Jarvis".into(), body: "Pense à sortir le chien.".into(), time: now - chrono::Duration::minutes(2) },
    ];
    let jarvis_idle = Jarvis::default();
    let jarvis_on = Jarvis { state: "parle".into(), text: "Minuteur de dix minutes lancé.".into(), at: Some(std::time::Instant::now()), seen: Some(std::time::Instant::now()), micro: true, said: None, toi: "Mets un minuteur de dix minutes".into(), dit: String::new() };
    let ring = Ring { label: "Minuteur : pâtes".into(), since: std::time::Instant::now() };
    let full = backdrop::compose(sw as u32, sh as u32, sw as u32);
    // données de Jarvis (vraies si l'assistant tourne), complétées d'exemples si elles sont vides
    let mut jd = crate::jarvis_data::JData::default();
    for n in ["tableau", "taches", "rappels", "habitudes", "copies", "routines", "memoire", "musique", "appareils", "pc", "parametres"] {
        let r = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .get(&format!("{}/api/{n}", crate::jarvis_link::BASE))
            .call()
            .map_err(|e| e.to_string())
            .and_then(|x| x.into_json::<serde_json::Value>().map_err(|e| e.to_string()));
        log.push_str(&format!("Jarvis /api/{n} : {}\n", if r.is_ok() { "ok" } else { "pas de réponse" }));
        jd.set(n, r);
    }
    let empty = |jd: &crate::jarvis_data::JData, n: &str, k: &str| crate::jarvis_data::arr(jd.get(n), k).is_empty();
    if crate::jarvis_data::arr(jd.get("taches"), "listes").iter().all(|l| crate::jarvis_data::arr(Some(l), "taches").is_empty()) {
        jd.set("taches", Ok(serde_json::json!({"ok": true, "listes": [{"nom": "À faire", "taches": [
            {"id": "a", "texte": "Finir l'exercice 4 de maths", "fait": false}, {"id": "b", "texte": "Ranger ma chambre", "fait": false},
            {"id": "c", "texte": "Acheter des piles pour la manette", "fait": true}]}, {"nom": "Courses", "taches": []}]})));
    }
    if empty(&jd, "rappels", "rappels") {
        jd.set("rappels", Ok(serde_json::json!({"ok": true, "rappels": [{"id": "r1", "texte": "Rendre l'autorisation de sortie", "quand": "2026-11-16 08:00", "repetition": "", "fait": false},
            {"id": "r2", "texte": "Entraînement de foot", "quand": "2026-11-18 17:30", "repetition": "hebdo", "fait": false}]})));
    }
    if empty(&jd, "copies", "copies") {
        jd.set("copies", Ok(serde_json::json!({"ok": true, "copies": [{"id": "c1", "texte": "Exercice 4 p. 52"}, {"id": "c2", "texte": "https://www.youtube.com/watch?v=exemple"}]})));
    }
    if empty(&jd, "appareils", "appareils") {
        jd.set("appareils", Ok(serde_json::json!({"ok": true, "appareils": [{"nom": "Manette Xbox", "type": "manette", "batterie": 62, "branche": false},
            {"nom": "Casque Bluetooth", "type": "casque", "batterie": 18, "branche": true}]})));
    }
    if crate::jarvis_data::arr(jd.get("memoire"), "noeuds").len() < 2 {
        let n = |id: &str, nom: &str, ty: &str, nb: u32| serde_json::json!({"id": id, "nom": nom, "type": ty, "nb_liens": nb, "cree": 1.0});
        jd.set("memoire", Ok(serde_json::json!({"ok": true, "sauvegarde": "18/11 16:40",
            "noeuds": [n("moi", "Moi", "moi", 7), n("maman", "Maman", "personne", 1), n("pronote", "Pronote", "ecole", 1),
                n("3h", "3h", "autre", 1), n("7_mai_2022", "7 mai 2022", "evenement", 1), n("les_livres", "les livres", "preference", 1),
                n("livre", "Livre", "objet", 1), n("chill_relax", "Chill Relax", "activite", 1)],
            "liens": [{"a": "moi", "b": "maman", "relation": "a pour mère"}, {"a": "moi", "b": "pronote", "relation": "utilise"},
                {"a": "moi", "b": "3h", "relation": "a dit"}, {"a": "moi", "b": "7_mai_2022", "relation": "a dit"},
                {"a": "moi", "b": "les_livres", "relation": "aime"}, {"a": "moi", "b": "livre", "relation": "a"}, {"a": "moi", "b": "chill_relax", "relation": "écoute"}],
            "suggestions": [{"id": "3h", "nom": "3h", "raison": "heure ou durée seule"}, {"id": "7_mai_2022", "nom": "7 mai 2022", "raison": "date seule"},
                {"id": "livre", "nom": "Livre", "raison": "doublon de « les livres »", "vers": "les_livres"}]})));
    }
    let mut mem = crate::jarvis_data::MemGraph::default();
    mem.sync(jd.get("memoire"));
    for _ in 0..400 {
        if !mem.step(jd.get("memoire")) {
            break;
        }
    }
    let ui = std::cell::RefCell::new(crate::island_big::BigUi::default());
    let pinfo = settings::Info { pn: Default::default(), ia: crate::ia::vue(), mem: String::new(), acc: Default::default(), discord: String::new(), winnotif: String::new() };
    let mail_ai = island::MailAi { n: Some(0), question: "Fais-moi un résumé de ce mail.".into(), answer: "Le collège te demande de rendre l'autorisation signée pour la sortie au musée avant lundi 8 h. Pense à la faire signer ce week-end.".into(), asking: false };
    let clips: Vec<String> = vec!["Exercice 4 p. 52".into(), "https://www.youtube.com/watch?v=exemple".into(), "rdv 14h30 dentiste".into()];
    let shot = |d: &Data, name: &str, cfg: &Config, st: &Island, ring: Option<&Ring>, jarvis: &Jarvis, log: &mut String| {
        let ui = ui.borrow();
        let r = R {
            cfg,
            fonts: &fonts,
            dpi,
            backdrop: Some(&bd),
            d,
            st,
            clocks: &clocks,
            chat: &chat,
            ring,
            jarvis,
            notifs: &notifs,
            discord_unread: Some(2),
            now,
            sw,
            mouse: (sw / 2 + 300, 200),
            model: "qwen3.5:4b".into(),
            jarvis_cmds: 3,
            clips: &clips,
            mail_ai: &mail_ai,
            jd: &jd,
            mem: &mem,
            ui: &ui,
            sh,
            status: "",
            info: Some(&pinfo),
        };
        let t0 = Instant::now();
        let (mode, _, _) = island_draw::target(&r);
        let Some((pm, x, y, hits)) = island_draw::render(&r) else {
            log.push_str(&format!("{name} : échec du rendu\n"));
            return;
        };
        log.push_str(&format!("{name} ({mode:?}) : {}x{} px, {} zones cliquables, {:?}\n", pm.width(), pm.height(), hits.len(), t0.elapsed()));
        let Some(mut desk) = IntSize::from_wh(sw as u32, sh as u32).and_then(|z| Pixmap::from_vec(full.clone(), z)) else { return };
        desk.draw_pixmap(x, y, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        let (x0, w) = ((x - 20).max(0), (pm.width() as i32 + 40).min(sw - (x - 20).max(0)));
        let ch = (pm.height() + 30).min(sh as u32);
        if let Some(crop) = IntRect::from_xywh(x0, 0, w as u32, ch).and_then(|rr| desk.clone_rect(rr)) {
            let _ = crop.save_png(out.join(format!("{name}.png")));
        }
    };
    let mk = |want: bool, view: View| {
        let mut s = Island::new(std::ptr::null_mut());
        s.want = want;
        s.view = view;
        s
    };
    shot(&d, "01_pilule", &cfg, &mk(false, View::Main), None, &jarvis_idle, &mut log);
    let mut fun = cfg.clone();
    fun.ile.pet = 1;
    fun.ile.eyes = true;
    fun.ile.rainbow = true;
    fun.ile.pill_date = true;
    shot(&d, "02_pilule_rigolote", &fun, &mk(false, View::Main), None, &jarvis_idle, &mut log);
    let mut t = mk(false, View::Main);
    t.toast = Some((notifs[2].clone(), std::time::Instant::now()));
    shot(&d, "03_notification", &cfg, &t, None, &jarvis_idle, &mut log);
    shot(&d, "04_jarvis", &cfg, &mk(false, View::Main), None, &jarvis_on, &mut log);
    shot(&d, "05_sonnerie", &cfg, &mk(false, View::Main), Some(&ring), &jarvis_idle, &mut log);
    let mut o = mk(true, View::Main);
    o.unseen = 3;
    shot(&d, "06_ouverte", &cfg, &o, None, &jarvis_on, &mut log);
    for (i, (name, p)) in [("nuit", 0usize), ("clair", 8), ("neon", 7), ("coucher", 5)].iter().enumerate() {
        let mut c2 = cfg.clone();
        c2.ile.preset = *p;
        if *p == 7 {
            c2.ile.font = 1;
            c2.ile.snow = true;
        }
        if *p == 5 {
            c2.ile.pirate = true;
            c2.ile.clock_color = 3;
        }
        shot(&d, &format!("07_theme_{i}_{name}"), &{ let mut c4 = c2.clone(); c4.ile.layout = 1; c4 }, &mk(true, View::Main), None, &jarvis_idle, &mut log);
    }
    let mut hw = mk(true, View::Homework);
    hw.sel = Some(0);
    shot(&d, "08_devoirs", &cfg, &hw, None, &jarvis_idle, &mut log);
    shot(&d, "09_notes", &cfg, &mk(true, View::Grades), None, &jarvis_idle, &mut log);
    let mut ml = mk(true, View::Mails);
    ml.sel = Some(0);
    shot(&d, "10_mails", &cfg, &ml, None, &jarvis_idle, &mut log);
    shot(&d, "11_discord", &cfg, &mk(true, View::Discord), None, &jarvis_idle, &mut log);
    shot(&d, "12_notifications", &cfg, &mk(true, View::Notifs), None, &jarvis_idle, &mut log);
    let mut chv = mk(true, View::Chat);
    chv.typing = true;
    chv.input = "Et après-demain ?".into();
    shot(&d, "13_conversation", &cfg, &chv, None, &jarvis_idle, &mut log);
    for (k, sl) in [0.35f32, 0.7].iter().enumerate() {
        let mut g = mk(false, View::Main);
        g.slide = *sl;
        g.gone = true;
        shot(&d, &format!("14_ile_qui_part_{k}"), &cfg, &g, None, &jarvis_idle, &mut log);
    }
    let mut ng = cfg.clone();
    ng.ile.show_grades = false;
    shot(&d, "15_ouverte_sans_notes", &ng, &mk(true, View::Main), None, &jarvis_idle, &mut log);
    // disposition « Cartes » : une image par onglet, puis l'accueil avec une fausse musique
    for tab in island::Tab::ALL {
        let mut s = mk(true, tab.view());
        if tab == island::Tab::School {
            s.sel = Some(0);
        }
        shot(&d, &format!("20_onglet_{:02}_{}", tab as usize, tab.label().to_lowercase()), &cfg, &s, None, &jarvis_on, &mut log);
    }
    let mut cols = cfg.clone();
    cols.ile.layout = 1;
    shot(&d, "21_colonnes", &cols, &mk(true, View::Main), None, &jarvis_idle, &mut log);
    let mut art = Pixmap::new(300, 300).unwrap();
    {
        use crate::gfx::{self, Rgba};
        if let Some(p) = gfx::rrect(0.0, 0.0, 300.0, 300.0, 0.0, false) {
            gfx::grad_fill(&mut art, &p, 0.0, 300.0, Rgba::rgb8(255, 150, 60), Rgba::rgb8(240, 96, 40));
        }
        gfx::circle(&mut art, 215.0, 85.0, 48.0, Rgba::rgb8(255, 236, 170));
        for (k, col) in [(0.0f32, Rgba::rgb8(214, 92, 40)), (40.0, Rgba::rgb8(170, 64, 30))] {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(0.0, 300.0);
            for i in 0..=30 {
                let u = i as f32 / 30.0;
                pb.line_to(u * 300.0, 190.0 + k - (u * 6.0 + k).sin() * 22.0);
            }
            pb.line_to(300.0, 300.0);
            pb.close();
            if let Some(p) = pb.finish() {
                gfx::fill(&mut art, &p, col);
            }
        }
    }
    d.media = Some(crate::data::Media { title: "Sunday Lemon".into(), artist: "Mellow Fruit".into(), app: "Spotify".into(), playing: true, art: Some(art) });
    shot(&d, "22_accueil_musique", &cfg, &mk(true, View::Main), None, &jarvis_on, &mut log);
    shot(&d, "23_onglet_musique", &cfg, &mk(true, View::Music), None, &jarvis_idle, &mut log);
    let mut blur = cfg.clone();
    blur.ile.art_blur = true;
    blur.ile.left_card = 1;
    blur.ile.left_grad = 2;
    shot(&d, "24_accueil_pochette_floue", &blur, &mk(true, View::Main), None, &jarvis_idle, &mut log);
    // musique venant de Chrome avec seulement l'icône de l'appli : pas de logo, fond météo
    let mut icon = Pixmap::new(256, 256).unwrap();
    crate::gfx::circle(&mut icon, 128.0, 128.0, 110.0, crate::gfx::Rgba::rgb8(219, 68, 55));
    crate::gfx::circle(&mut icon, 128.0, 128.0, 48.0, crate::gfx::Rgba::rgb8(66, 133, 244));
    d.media = Some(crate::data::Media { title: "Lofi hip hop radio - beats to relax".into(), artist: "Lofi Girl".into(), app: "Google Chrome".into(), playing: true, art: Some(icon) });
    log.push_str(&format!("icône Chrome reconnue : {}\n", crate::island_cards::art_is_icon(d.media.as_ref().unwrap().art.as_ref().unwrap())));
    shot(&d, "27_chrome_sans_logo", &cfg, &mk(true, View::Main), None, &jarvis_idle, &mut log);
    shot(&d, "28_chrome_onglet_musique", &cfg, &mk(true, View::Music), None, &jarvis_idle, &mut log);
    // téléphone (exemples) : SMS avec code masqué dans la bulle, appel, notification d'appli
    let ph = |kind: &str, from: &str, text: &str, app: &str, min: i64| crate::data::PhoneItem { kind: kind.into(), from: from.into(), text: text.into(), app: app.into(), time: now - chrono::Duration::minutes(min) };
    d.phone = vec![
        ph("appli", "Inès", "on se voit demain ?", "WhatsApp", 40),
        ph("appel", "Lucas", "Appel manqué", "", 25),
        ph("batterie", "Téléphone", "15 % restants", "", 10),
        ph("sms", "Banque", "Votre code de vérification est 482913. Ne le communiquez à personne.", "", 1),
    ];
    let mut tp = mk(false, View::Main);
    tp.toast = Some((INotif { id: 9, kind: "sms".into(), src: Src::Phone, title: "Banque".into(), body: "Votre code de vérification est 482913. Ne le communiquez à personne.".into(), time: now }, std::time::Instant::now()));
    shot(&d, "29_bulle_sms_code_masque", &cfg, &tp, None, &jarvis_idle, &mut log);
    shot(&d, "30_onglet_telephone", &cfg, &mk(true, View::Phone), None, &jarvis_idle, &mut log);
    // 1.4 : les dix nouveaux thèmes (pilule et île ouverte, avec leur décor)
    for p in 10..crate::island_draw::PRESETS.len() {
        let mut c2 = cfg.clone();
        c2.ile.preset = p;
        let nom = crate::island_draw::PRESET_NAMES[p].to_lowercase().replace(' ', "_");
        shot(&d, &format!("60_theme_{p}_{nom}_pilule"), &c2, &mk(false, View::Main), None, &jarvis_idle, &mut log);
        c2.ile.layout = 1;
        shot(&d, &format!("61_theme_{p}_{nom}_ouverte"), &c2, &mk(true, View::Main), None, &jarvis_idle, &mut log);
    }
    // 1.4 : imprimante 3D (pilule pendant une impression, page de la grande appli avec et sans caméra)
    d.printer = crate::data::Printer {
        online: true, status: 13, progress: 42, layer: 126, total: 300, left_min: 85, elapsed_min: 62,
        file: "support_casque_v3.gcode".into(), nozzle: 220, nozzle_target: 220, bed: 60, bed_target: 60, box_temp: 31,
        light: true, name: "Centauri Carbon".into(), ip: "192.168.1.50".into(), ..Default::default()
    };
    let mut pc = cfg.clone();
    pc.printer.on = true;
    pc.printer.ip = "192.168.1.50".into();
    shot(&d, "70_pilule_impression", &pc, &mk(false, View::Main), None, &jarvis_idle, &mut log);
    let mut pb = pc.clone();
    pb.ile.grande = 1;
    shot(&d, "71_grande_imprimante_sans_camera", &pb, &mk(true, View::Printer), None, &jarvis_idle, &mut log);
    {
        let mut cam = Pixmap::new(640, 360).unwrap();
        let mut paint = tiny_skia::Paint::default();
        paint.shader = tiny_skia::LinearGradient::new(
            tiny_skia::Point::from_xy(0.0, 0.0), tiny_skia::Point::from_xy(640.0, 360.0),
            vec![tiny_skia::GradientStop::new(0.0, tiny_skia::Color::from_rgba8(40, 40, 46, 255)), tiny_skia::GradientStop::new(1.0, tiny_skia::Color::from_rgba8(120, 110, 90, 255))],
            tiny_skia::SpreadMode::Pad, Transform::identity(),
        ).unwrap();
        cam.fill_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, 640.0, 360.0).unwrap(), &paint, Transform::identity(), None);
        ui.borrow_mut().printer_cam = Some(cam);
    }
    shot(&d, "72_grande_imprimante", &pb, &mk(true, View::Printer), None, &jarvis_idle, &mut log);
    ui.borrow_mut().printer_stop = Some(Instant::now());
    d.printer.status = 6;
    shot(&d, "73_grande_imprimante_pause_confirmer", &pb, &mk(true, View::Printer), None, &jarvis_idle, &mut log);
    shot(&d, "74_onglet_imprimante", &pc, &mk(true, View::Printer), None, &jarvis_idle, &mut log);
    let mut pv = pb.clone();
    pv.printer.ip.clear();
    shot(&d, "75_grande_imprimante_recherche", &pv, &mk(true, View::Printer), None, &jarvis_idle, &mut log);
    *ui.borrow_mut() = crate::island_big::BigUi::default();
    d.printer = Default::default();
    // grande île : plusieurs sections
    let mut big = cfg.clone();
    big.ile.grande = 1;
    for (name, v) in [("aujourdhui", View::Today), ("accueil", View::Main), ("taches", View::Tasks), ("rappels", View::Reminders), ("habitudes", View::Habits), ("memoire", View::Memory),
        ("pc", View::Pc), ("parametres", View::JSettings), ("routines", View::Routines), ("batteries", View::Batteries), ("copies", View::Clips), ("telephone", View::Phone), ("mails", View::Mails), ("pronote", View::School), ("minuteurs", View::Timers), ("discord", View::Discord)] {
        shot(&d, &format!("40_grande_{name}"), &big, &mk(true, v), None, &jarvis_idle, &mut log);
    }
    // mémoire en liste : suggestions cochées, puis confirmation de suppression
    {
        let mut u = ui.borrow_mut();
        u.mem_list = true;
        u.mem_checked = vec!["3h".into(), "7_mai_2022".into(), "livre".into()];
        u.msg = "Voici ce qui serait supprimé (3). Décoche ce que tu veux garder, puis « Supprimer ».".into();
    }
    shot(&d, "42_grande_memoire_liste", &big, &mk(true, View::Memory), None, &jarvis_idle, &mut log);
    {
        let mut u = ui.borrow_mut();
        u.mem_confirm = Some(Instant::now());
        u.mem_filter = 1;
        u.msg.clear();
    }
    shot(&d, "43_grande_memoire_confirmer", &big, &mk(true, View::Memory), None, &jarvis_idle, &mut log);
    *ui.borrow_mut() = crate::island_big::BigUi::default();
    // île normale : nouveaux onglets
    for (name, v) in [("taches", View::Tasks), ("pc", View::Pc), ("memoire", View::Memory), ("parametres", View::JSettings), ("habitudes", View::Habits)] {
        shot(&d, &format!("41_onglet_{name}"), &cfg, &mk(true, v), None, &jarvis_idle, &mut log);
    }
    // comptes : section, et carte « reconnecte Pronote »
    let mut big2 = cfg.clone();
    big2.ile.grande = 1;
    shot(&d, "50_grande_comptes", &big2, &mk(true, View::Accounts), None, &jarvis_idle, &mut log);
    d.pronote.error = "erreur du serveur (jeton refusé)".into();
    shot(&d, "51_accueil_reconnecter", &cfg, &mk(true, View::Main), None, &jarvis_idle, &mut log);
    shot(&d, "52_grande_accueil_reconnecter", &big2, &mk(true, View::Main), None, &jarvis_idle, &mut log);
    d.pronote.error.clear();
    let mut mv = mk(true, View::Mails);
    mv.sel = Some(0);
    shot(&d, "25_mail_jarvis", &cfg, &mv, None, &jarvis_on, &mut log);
    let mut mv2 = mk(true, View::Mails);
    mv2.sel = Some(1);
    mv2.typing = true;
    mv2.input = "Il veut quoi exactement ?".into();
    shot(&d, "26_mail_saisie", &cfg, &mv2, None, &jarvis_on, &mut log);
    let _ = Mode::Pill;
    let _ = island::fmt_dur(0.0);
    // grande appli › Paramètres : une page par image
    for p in 0..crate::reglages::PAGES.len() {
        ui.borrow_mut().reg_page = p;
        shot(&d, &format!("reglages_{p:02}"), &big2, &mk(true, View::JSettings), None, &jarvis_idle, &mut log);
    }
    let mut big3 = big2.clone();
    big3.ile.g_fond = 5;
    big3.ile.g_couleurs = true;
    big3.ile.g_selection = 3;
    big3.ile.g_heure = true;
    big3.ile.g_fond_colonne = true;
    big3.ile.g_texte = 1.15;
    big3.ile.g_cachees = vec![5, 6, 10];
    shot(&d, "reglages_style", &big3, &mk(true, View::Tasks), None, &jarvis_idle, &mut log);
    let mut big4 = big2.clone();
    big4.ile.g_reduite = true;
    big4.ile.g_largeur = 0.85;
    big4.ile.g_selection = 1;
    shot(&d, "reglages_reduite", &big4, &mk(true, View::Today), None, &jarvis_idle, &mut log);
    log.push_str(&format!("{} options dans les réglages\n", crate::ile_settings::opts().len()));
    let _ = std::fs::write(out.join("log.txt"), log);
}
