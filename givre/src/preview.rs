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
    d.accounts.ai = "Ollama est prêt avec le modèle qwen2.5:7b.".into();
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
        INotif { id: 1, src: Src::Pronote, title: "Nouvelle note".into(), body: "Mathématiques : 16,5/20".into(), time: now - chrono::Duration::minutes(50) },
        INotif { id: 2, src: Src::Mail, title: "Mail urgent — Collège Jean Moulin".into(), body: "Sortie au musée : autorisation à rendre lundi".into(), time: now - chrono::Duration::minutes(42) },
        INotif { id: 3, src: Src::Discord, title: "Lucas".into(), body: "t'es chaud pour une game ce soir ?".into(), time: now - chrono::Duration::minutes(12) },
        INotif { id: 4, src: Src::Jarvis, title: "Jarvis".into(), body: "Pense à sortir le chien.".into(), time: now - chrono::Duration::minutes(2) },
    ];
    let jarvis_idle = Jarvis::default();
    let jarvis_on = Jarvis { state: "parle".into(), text: "Minuteur de dix minutes lancé.".into(), at: Some(std::time::Instant::now()), seen: Some(std::time::Instant::now()), micro: true };
    let ring = Ring { label: "Minuteur : pâtes".into(), since: std::time::Instant::now() };
    let full = backdrop::compose(sw as u32, sh as u32, sw as u32);
    let mail_ai = island::MailAi { n: Some(0), question: "Fais-moi un résumé de ce mail.".into(), answer: "Le collège te demande de rendre l'autorisation signée pour la sortie au musée avant lundi 8 h. Pense à la faire signer ce week-end.".into(), asking: false };
    let clips: Vec<String> = vec!["Exercice 4 p. 52".into(), "https://www.youtube.com/watch?v=exemple".into(), "rdv 14h30 dentiste".into()];
    let shot = |d: &Data, name: &str, cfg: &Config, st: &Island, ring: Option<&Ring>, jarvis: &Jarvis, log: &mut String| {
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
    // fenêtre des réglages : un onglet par image
    let mut c3 = cfg.ile.clone();
    for tab in 0..crate::ile_settings::TABS.len() {
        let mut st = crate::ile_settings::IleSettings { hwnd: std::ptr::null_mut(), tab, scroll: 0, hits: Vec::new(), pm: None };
        crate::ile_settings::draw(&mut st, &mut c3, &fonts, cfg.accent, dpi);
        if let Some(pm) = &st.pm {
            let _ = pm.save_png(out.join(format!("reglages_{tab:02}.png")));
        }
    }
    log.push_str(&format!("{} options dans les réglages\n", crate::ile_settings::opts().len()));
    let _ = std::fs::write(out.join("log.txt"), log);
}
