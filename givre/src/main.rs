//! Givre : widgets de bureau en verre flou (heure, météo, calendrier, vacances).
//! Win32 natif + dessin logiciel (tiny-skia) : pas de navigateur, très peu de RAM et de CPU.

#![windows_subsystem = "windows"]
#![allow(unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]

mod accounts;
mod api;
mod backdrop;
mod brain;
mod config;
mod data;
mod extra;
mod gfx;
mod ile_settings;
mod island;
mod island_cards;
mod island_draw;
mod jarvis_link;
mod preview;
mod pronote;
mod settings;
mod sys;
mod widgets;

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use chrono::{Local, NaiveDate, NaiveTime, Timelike};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use backdrop::Backdrop;
use config::{Config, Place};
use data::{Data, Job, Msg};
use extra::{DCmd, MCmd};
use pronote::PJob;
use gfx::{Fonts, Theme};
use settings::{F, Settings};
use sys::wide;
use widgets::{Action, Hit, Kind, Look};

const MAIN_CLASS: &str = "GivreMain";
const WIDGET_CLASS: &str = "GivreWidget";
const SETTINGS_CLASS: &str = "GivreSettings";
const ILE_CLASS: &str = "GivreIle";
const ILE_SET_CLASS: &str = "GivreIleSettings";

const WM_APP_TRAY: u32 = WM_APP + 1;
const WM_APP_SHOW: u32 = WM_APP + 3;
const WM_APP_QUIT: u32 = WM_APP + 4;
/// Demande à un Givre déjà lancé de passer en mode Dynamic Island (`givre.exe --ile`).
const WM_APP_ILE: u32 = WM_APP + 5;
/// Ouvrir le menu (bouton « ••• » de l'île).
const WM_APP_MENU: u32 = WM_APP + 6;

const T_CLOCK: usize = 1;
const T_VIS: usize = 2;
const T_WEATHER: usize = 3;
const T_HOLI: usize = 4;
const T_SAVE: usize = 5;
const T_RETRY: usize = 6;
const T_SAFE: usize = 7;
const T_SYNC: usize = 8;
const T_PRONOTE: usize = 9;
const T_ILE_CLOCK: usize = 10;
const T_ILE_ANIM: usize = 11;
const T_ILE_LEAVE: usize = 12;
const T_RING: usize = 13;
const T_ILE_FX: usize = 14;
const T_ILE_OPEN: usize = 15;
const T_ILE_SLOW: usize = 16;
const T_ILE_WATCH: usize = 17;

static MAIN: AtomicIsize = AtomicIsize::new(0);

struct Slot {
    kind: Kind,
    hwnd: HWND,
    hits: Vec<Hit>,
    m: i32,
}

pub struct App {
    main: HWND,
    cfg: Config,
    fonts: Fonts,
    backdrop: Option<Backdrop>,
    data: Data,
    slots: Vec<Slot>,
    shown: bool,
    edit: bool,
    icon: HICON,
    month_off: i32,
    dpi: f32,
    jobs: Sender<Job>,
    rx: Receiver<Msg>,
    settings: Option<Settings>,
    last_day: Option<NaiveDate>,
    last_block: Option<String>,
    /// Lien (+ recherche Outlook) ouvert quand on clique sur la dernière notification.
    notif_link: Option<(String, String)>,
    status: String,
    taskbar_created: u32,
    drag: Option<Drag>,
    media_tx: Sender<MCmd>,
    discord_tx: Sender<DCmd>,
    winnotif_tx: Sender<()>,
    winnotif_on: Arc<std::sync::atomic::AtomicBool>,
    printer_ip: Arc<Mutex<String>>,
    pn_tx: Sender<PJob>,
    sel_day: Option<NaiveDate>,
    /// Dynamic Island (mode « barre en haut de l'écran »).
    ile: Option<island::Island>,
    ile_fonts: Option<(usize, Fonts)>,
    ile_set_win: Option<ile_settings::IleSettings>,
    ile_last_act: Option<Action>,
    ile_last_mode: island::Mode,
    fx_cur: Option<u32>,
    clocks: island::Clocks,
    chat: island::Chat,
    jarvis: island::Jarvis,
    notifs: Vec<island::INotif>,
    ring: Option<island::Ring>,
    discord_unread: Option<u32>,
    pending: HashMap<u64, (Origin, usize, bool)>,
    api_port: Arc<AtomicU32>,
    notif_next: u64,
    out: data::Out,
    jarvis_cmds: u32,
    jarvis_day: Option<NaiveDate>,
    clips: Vec<String>,
    clip_skip: Option<String>,
    /// Corps des mails déjà téléchargés (cache), requêtes /mail en attente, question sur un mail.
    mail_bodies: HashMap<String, String>,
    mail_waiters: Vec<(String, usize, Sender<(u16, String)>)>,
    mail_ai: island::MailAi,
    mail_ask_wait: Option<(String, usize, String, u64)>,
}

/// Glisser un widget : position du curseur et du widget au moment du clic.
struct Drag {
    hwnd: HWND,
    sx: i32,
    sy: i32,
    px: i32,
    py: i32,
    moved: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Accès à l'état (None si déjà utilisé : évite toute panique quand Windows rappelle en cascade).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|c| {
        let mut g = c.try_borrow_mut().ok()?;
        g.as_mut().map(f)
    })
}

fn snap(v: i32, g: i32) -> i32 {
    if g <= 1 { v } else { ((v as f32 / g as f32).round() as i32) * g }
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp & 0xFFFF) as i16 as i32, ((lp >> 16) & 0xFFFF) as i16 as i32)
}

/// Disposition par défaut : deux colonnes en haut à droite de l'écran.
pub fn default_places(cfg: &Config, dpi: f32) -> Vec<(Kind, Place)> {
    let wa = sys::work_area();
    let s = dpi * cfg.scale;
    let cw = (widgets::W * s).round() as i32;
    let gap = (16.0 * dpi) as i32;
    let edge = (28.0 * dpi) as i32;
    let x1 = wa.right - edge - cw;
    let x2 = x1 - gap - cw;
    let x3 = x2 - gap - cw;
    let x4 = x3 - gap - cw;
    let h = |k: Kind| (widgets::size(k, cfg, true).1 * s).round() as i32 + gap;
    let y0 = wa.top + edge;
    let p = |x, y| Place { x, y, scale: 1.0 };
    vec![
        (Kind::Clock, p(x1, y0)),
        (Kind::Weather, p(x1, y0 + h(Kind::Clock))),
        (Kind::Printer, p(x1, y0 + h(Kind::Clock) + h(Kind::Weather))),
        (Kind::Calendar, p(x2, y0)),
        (Kind::Countdown, p(x2, y0 + h(Kind::Calendar))),
        (Kind::Music, p(x2, y0 + h(Kind::Calendar) + h(Kind::Countdown))),
        (Kind::Mail, p(x3, y0)),
        (Kind::Discord, p(x3, y0 + h(Kind::Mail))),
        (Kind::Grades, p(x3, y0 + h(Kind::Mail) + h(Kind::Discord))),
        (Kind::NextClass, p(x4, y0)),
        (Kind::Homework, p(x4, y0 + h(Kind::NextClass))),
        (Kind::School, p(x4, y0 + h(Kind::NextClass) + h(Kind::Homework))),
    ]
}

impl App {
    fn theme(&self) -> Theme {
        let dark = match self.cfg.theme {
            0 => true,
            1 => false,
            _ => !sys::apps_light_theme(),
        };
        let acc = if self.cfg.accent_auto {
            self.backdrop.as_ref().map(|b| b.accent).unwrap_or(self.cfg.accent)
        } else {
            self.cfg.accent
        };
        Theme::new(dark, acc, self.cfg.opacity)
    }

    fn place(&self, k: Kind) -> Place {
        self.cfg.places.get(k.key()).copied().unwrap_or(Place { x: 100, y: 100, scale: 1.0 })
    }

    fn ensure_places(&mut self) {
        let (sw, sh) = sys::screen_size();
        for (k, p) in default_places(&self.cfg, self.dpi) {
            let ok = self
                .cfg
                .places
                .get(k.key())
                .map(|q| q.x > -200 && q.y > -200 && q.x < sw - 40 && q.y < sh - 40)
                .unwrap_or(false);
            if !ok {
                self.cfg.places.insert(k.key().to_string(), p);
            }
        }
    }

    fn ensure_backdrop(&mut self) {
        let ile = self.cfg.ile.on && self.cfg.ile.blur;
        if !(self.cfg.blur || self.cfg.accent_auto || ile) {
            self.backdrop = None;
            return;
        }
        match &mut self.backdrop {
            Some(b) => b.set_blur(if self.cfg.ile.on { self.cfg.ile.blur_amount } else { self.cfg.blur_amount }),
            None => {
                let (sw, sh) = sys::screen_size();
                let amount = if self.cfg.ile.on { self.cfg.ile.blur_amount } else { self.cfg.blur_amount };
                self.backdrop = Some(Backdrop::load(sw, sh, amount));
            }
        }
    }

    fn render(&mut self, i: usize) {
        let theme = self.theme();
        let kind = self.slots[i].kind;
        let place = self.place(kind);
        let look = Look {
            cfg: &self.cfg,
            fonts: &self.fonts,
            theme: &theme,
            backdrop: self.backdrop.as_ref(),
            dpi: self.dpi,
            edit: self.edit,
            sel: self.sel_day,
        };
        let now = Local::now().naive_local();
        if let Some((pm, m, hits)) = widgets::render(kind, &look, &self.data, place, self.month_off, now) {
            unsafe { gfx::present(self.slots[i].hwnd, &pm, place.x - m, place.y - m) };
            self.slots[i].hits = hits;
            self.slots[i].m = m;
        }
    }

    fn render_all(&mut self) {
        for i in 0..self.slots.len() {
            self.render(i);
        }
    }

    fn render_kind(&mut self, k: Kind) {
        if let Some(i) = self.slots.iter().position(|s| s.kind == k) {
            self.render(i);
        }
    }

    /// Crée / détruit les fenêtres selon les widgets activés.
    fn sync_slots(&mut self) {
        let mut i = 0;
        while i < self.slots.len() {
            if !self.slots[i].kind.enabled(&self.cfg) {
                let s = self.slots.remove(i);
                unsafe { DestroyWindow(s.hwnd) };
            } else {
                i += 1;
            }
        }
        for k in Kind::ALL {
            if k.enabled(&self.cfg) && !self.slots.iter().any(|s| s.kind == k) {
                let hwnd = unsafe {
                    CreateWindowExW(
                        WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                        wide(WIDGET_CLASS).as_ptr(),
                        wide("Givre").as_ptr(),
                        WS_POPUP,
                        0,
                        0,
                        1,
                        1,
                        null_mut(),
                        null_mut(),
                        GetModuleHandleW(null()),
                        null(),
                    )
                };
                if hwnd.is_null() {
                    continue;
                }
                self.slots.push(Slot { kind: k, hwnd, hits: Vec::new(), m: 0 });
                if self.shown {
                    let i = self.slots.len() - 1;
                    self.render(i);
                    unsafe {
                        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                        sys::pin_to_desktop(hwnd);
                    }
                }
            }
        }
    }

    /// Remet les widgets à leur place : juste au-dessus du bureau (sous les fenêtres normales),
    /// ou devant tout en mode édition pour pouvoir les déplacer.
    fn place_all(&self) {
        for s in &self.slots {
            unsafe {
                if self.edit {
                    sys::raise(s.hwnd)
                } else {
                    sys::pin_to_desktop(s.hwnd)
                }
            };
        }
    }

    fn set_shown(&mut self, show: bool) {
        if show == self.shown {
            return;
        }
        self.shown = show;
        unsafe {
            if show {
                self.last_day = None;
                self.render_all();
                for s in &self.slots {
                    ShowWindow(s.hwnd, SW_SHOWNOACTIVATE);
                }
                self.place_all();
                self.tick();
            } else {
                for s in &self.slots {
                    ShowWindow(s.hwnd, SW_HIDE);
                }
                KillTimer(self.main, T_CLOCK);
                sys::trim_memory();
            }
        }
    }

    /// Les widgets restent sur le bureau, sous les fenêtres normales. Pour économiser la mémoire et
    /// le processeur, ils sont cachés (plus aucun dessin) seulement quand UNE fenêtre recouvre tout
    /// le bureau, et réaffichés dès qu'un bout du bureau redevient visible.
    fn check_visibility(&mut self) {
        if self.cfg.ile.on {
            // mode Dynamic Island : pas de widgets sur le bureau ; l'île se cache en plein écran
            self.set_shown(false);
            self.ile_fullscreen_check();
            return;
        }
        let block = if self.cfg.hide_when_windows && !self.edit { sys::covering_window() } else { None };
        if block != self.last_block {
            match &block {
                Some(b) => sys::log(&format!("widgets en pause (bureau recouvert par : {b})")),
                None => sys::log("widgets affichés (bureau visible)"),
            }
            self.last_block = block.clone();
        }
        let was = self.shown;
        self.set_shown(block.is_none());
        if self.shown && was {
            self.place_all();
        }
    }

    /// Mise à jour de l'heure, calée sur le changement de minute (ou de seconde).
    fn tick(&mut self) {
        if !self.shown {
            return;
        }
        let now = Local::now();
        let today = now.date_naive();
        if self.last_day != Some(today) {
            self.last_day = Some(today);
            self.render_all();
        } else {
            self.render_kind(Kind::Clock);
            self.render_kind(Kind::NextClass);
        }
        let ms = now.timestamp_subsec_millis().min(999);
        let wait = if self.cfg.clock.seconds && self.cfg.clock.on { 1000 - ms } else { (60 - now.second()) * 1000 - ms };
        unsafe { SetTimer(self.main, T_CLOCK, wait + 20, None) };
    }

    fn refresh_weather(&self) {
        let _ = self.jobs.send(Job::Weather(self.cfg.weather.lat, self.cfg.weather.lon));
    }

    fn refresh_holidays(&self) {
        let _ = self.jobs.send(Job::Holidays(self.cfg.countdown.academie.clone()));
    }

    fn sync(&self) {
        let _ = self.jobs.send(Job::Sync(accounts::SyncCfg {
            ms_client: self.cfg.mail.ms_client.trim().to_string(),
            model: self.cfg.mail.model.trim().to_string(),
            ollama_url: self.cfg.mail.ollama_url.trim().to_string(),
            ai: self.cfg.mail.ai,
        }));
    }

    fn pn_sync(&self) {
        let _ = self.pn_tx.send(PJob::Sync(self.cfg.pronote.google));
    }

    /// Lit la capture du QR code Pronote dans le presse-papiers.
    fn paste_qr(&mut self) {
        self.status = match sys::clipboard_image() {
            None => "Aucune image dans le presse-papiers : fais Win+Maj+S sur le QR code Pronote, puis clique « Coller ».".into(),
            Some((w, h, px)) => match pronote::read_qr(w, h, &px) {
                Ok(q) => {
                    self.cfg.pn_qr = q;
                    "QR code Pronote lu. Tape ton code à 4 chiffres, puis « Se connecter ».".into()
                }
                Err(e) => e,
            },
        };
    }

    fn pn_login(&mut self) {
        let pin = self.cfg.pn_pin.trim().to_string();
        if self.cfg.pn_qr.is_empty() {
            self.status = "Colle d'abord la capture du QR code (bouton « Coller »).".into();
        } else if pin.len() != 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
            self.status = "Le code doit faire 4 chiffres.".into();
        } else {
            let _ = self.pn_tx.send(PJob::Login(self.cfg.pn_qr.clone(), pin));
            self.status = "Connexion à Pronote…".into();
        }
        self.cfg.pn_pin.clear();
    }

    fn refresh_accounts(&self) {
        let _ = self.jobs.send(Job::Accounts(self.cfg.mail.model.trim().to_string(), self.cfg.mail.ollama_url.trim().to_string()));
    }

    fn update_printer_ip(&self) {
        *self.printer_ip.lock().unwrap() = if self.cfg.printer.on { self.cfg.printer.ip.trim().to_string() } else { String::new() };
    }

    fn on_net(&mut self) {
        let mut dirty: Vec<Kind> = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Weather(Ok(w)) => {
                    self.data.weather = Some(w);
                    dirty.extend([Kind::Weather, Kind::Clock]);
                }
                Msg::Weather(Err(e)) => {
                    sys::log(&format!("météo : {e}"));
                    unsafe { SetTimer(self.main, T_RETRY, 5 * 60_000, None) };
                }
                Msg::Holidays(Ok(h)) => {
                    self.data.holidays = h;
                    dirty.extend([Kind::Calendar, Kind::Countdown]);
                }
                Msg::Holidays(Err(e)) => {
                    sys::log(&format!("vacances : {e}"));
                    self.status = e;
                    unsafe { SetTimer(self.main, T_RETRY, 5 * 60_000, None) };
                }
                Msg::Geo(Ok((name, lat, lon))) => {
                    self.cfg.weather.city = name;
                    self.cfg.weather.lat = lat;
                    self.cfg.weather.lon = lon;
                    self.data.weather = None;
                    self.status = String::new();
                    self.refresh_weather();
                    self.save_soon();
                    dirty.push(Kind::Weather);
                }
                Msg::Geo(Err(e)) => self.status = e,
                Msg::Events(ev) => {
                    self.data.events = ev;
                    dirty.push(Kind::Calendar);
                }
                Msg::Mail(mb) => {
                    self.data.mail = mb;
                    dirty.push(Kind::Mail);
                }
                Msg::Accounts(a) => self.data.accounts = a,
                Msg::Changed => {
                    self.refresh_accounts();
                    self.sync();
                }
                Msg::Status(s) => self.status = s,
                Msg::Notify(t, b, l, f) => {
                    if self.cfg.mail.notify {
                        self.notify(island::Src::Mail, &t, &b, Some((l, f)));
                    }
                }
                Msg::Media(m) => {
                    self.data.media = m;
                    dirty.push(Kind::Music);
                }
                Msg::Discord(mut n) => {
                    n.title = sys::clean(&n.title);
                    n.body = sys::clean(&n.body);
                    // messages privés seulement, sauf option « messages des serveurs »
                    if !self.cfg.ile.discord_servers && sys::discord_is_server(&n.title) {
                        continue;
                    }
                    // même message reçu par la connexion Discord et par les notifications Windows
                    if self.data.discord.iter().any(|o| o.title == n.title && o.body == n.body) {
                        continue;
                    }
                    let fresh = Local::now().naive_local() - n.time < chrono::Duration::minutes(2);
                    if self.cfg.ile.on && fresh {
                        let (ti, bo) = (n.title.clone(), n.body.clone());
                        self.notify(island::Src::Discord, &ti, &bo, None);
                    } else if self.cfg.discord.notify && fresh && self.cfg.discord.important(&n.title, &n.body) {
                        self.notif_link = None;
                        unsafe { tray(self.main, self.icon, NIM_MODIFY, Some((n.title.as_str(), n.body.as_str()))) };
                    }
                    self.data.discord.push(n);
                    self.data.discord.sort_by_key(|n| n.time);
                    if self.data.discord.len() > 40 {
                        self.data.discord.remove(0);
                    }
                    dirty.push(Kind::Discord);
                }
                Msg::WinNotifState(s) => {
                    self.data.winnotif_state = s;
                    dirty.push(Kind::Discord);
                }
                Msg::DiscordState(s) => {
                    self.data.discord_state = s;
                    dirty.push(Kind::Discord);
                }
                Msg::Printer(p) => {
                    self.data.printer = p;
                    dirty.push(Kind::Printer);
                }
                Msg::School(s) => {
                    if s.connected {
                        self.cfg.pn_qr.clear();
                    }
                    self.check_good_grades(&s);
                    self.data.pronote = s;
                    dirty.extend([Kind::NextClass, Kind::Homework, Kind::Grades, Kind::School, Kind::Calendar]);
                }
                Msg::Answer(id, r) => self.ai_answer(id, r),
                Msg::Api(req, tx) => self.api(req, tx),
                Msg::JarvisLink(st) => self.jarvis_link(st),
                Msg::MailBody(id, r) => self.mail_body(id, r),
                Msg::PNotify(k, t, b) => {
                    let p = &self.cfg.pronote;
                    let hidden_grade = k == 0 && self.cfg.ile.on && !self.cfg.ile.show_grades;
                    if !hidden_grade && [p.n_grade, p.n_hw, p.n_cancel].get(k as usize).copied().unwrap_or(false) {
                        let base = &self.data.pronote.base;
                        let link = (!base.is_empty()).then(|| (format!("{base}/eleve.html"), String::new()));
                        self.notify(island::Src::Pronote, &t, &b, link);
                    }
                }
            }
        }
        if self.shown {
            dirty.sort_by_key(|k| *k as u8);
            dirty.dedup();
            for k in dirty {
                self.render_kind(k);
            }
        }
        self.ile_render();
        self.refresh_settings();
    }

    fn create_event(&mut self) {
        let d = &self.cfg.draft;
        let title = d.title.trim().to_string();
        let ds = d.date.trim();
        let date = NaiveDate::parse_from_str(ds, "%d/%m/%Y").or_else(|_| NaiveDate::parse_from_str(ds, "%Y-%m-%d"));
        let mut ts = d.time.trim().replace(['h', 'H'], ":");
        if ts.ends_with(':') {
            ts.push_str("00");
        }
        let time = if ts.is_empty() { Ok(None) } else { NaiveTime::parse_from_str(&ts, "%H:%M").map(Some) };
        let dur = d.dur.trim().parse::<i64>().unwrap_or(60);
        match (title.is_empty(), date, time) {
            (true, _, _) => self.status = "Donne un titre à l'événement.".into(),
            (_, Err(_), _) => self.status = "Date invalide (JJ/MM/AAAA).".into(),
            (_, _, Err(_)) => self.status = "Heure invalide (HH:MM).".into(),
            (false, Ok(date), Ok(time)) => {
                let _ = self.jobs.send(Job::CreateEvent(title, date, time, dur));
                self.status = "Création de l'événement…".into();
                self.cfg.draft.title.clear();
            }
        }
    }

    fn open_settings_at(&mut self, page: usize) {
        self.open_settings();
        if let Some(st) = &mut self.settings {
            st.set_page(page);
        }
        self.refresh_settings();
    }

    fn save_soon(&self) {
        unsafe { SetTimer(self.main, T_SAVE, 800, None) };
    }

    // ---------- Paramètres ----------

    fn refresh_settings(&mut self) {
        let Some(mut st) = self.settings.take() else { return };
        let t = self.theme();
        let p = &self.data.pronote;
        let pn = settings::PnInfo {
            connected: p.connected,
            who: if p.class.is_empty() { p.name.clone() } else { format!("{} ({})", p.name, p.class) },
            state: if !p.error.is_empty() {
                format!("Problème : {}", p.error)
            } else if let Some(u) = p.updated {
                format!("Dernière mise à jour : {}", u.format("%d/%m à %H:%M"))
            } else {
                "Chargement…".into()
            },
            qr: !self.cfg.pn_qr.is_empty(),
        };
        let info = settings::Info {
            pn,
            edit: self.edit,
            mem: sys::mem_text(),
            status: self.status.clone(),
            acc: self.data.accounts.clone(),
            discord: self.data.discord_state.clone(),
            winnotif: self.data.winnotif_state.clone(),
        };
        if let Some(pm) = settings::draw(&mut st, &self.cfg, &self.fonts, &t, self.dpi, &info) {
            settings::store(&mut st, &pm);
        }
        let h = st.hwnd;
        self.settings = Some(st);
        unsafe { InvalidateRect(h, null(), 0) };
    }

    fn open_settings(&mut self) {
        unsafe {
            if let Some(st) = &self.settings {
                ShowWindow(st.hwnd, SW_RESTORE);
                SetForegroundWindow(st.hwnd);
                return;
            }
            let (w, h) = ((settings::WIN_W * self.dpi).round() as i32, (settings::WIN_H * self.dpi).round() as i32);
            let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
            let mut r = RECT { left: 0, top: 0, right: w, bottom: h };
            AdjustWindowRectEx(&mut r, style, 0, 0);
            let wa = sys::work_area();
            let (ww, wh) = (r.right - r.left, r.bottom - r.top);
            let (x, y) = (wa.left + (wa.right - wa.left - ww) / 2, wa.top + (wa.bottom - wa.top - wh) / 2);
            let hwnd = CreateWindowExW(
                0,
                wide(SETTINGS_CLASS).as_ptr(),
                wide("Givre — Paramètres").as_ptr(),
                style,
                x,
                y,
                ww,
                wh,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            if hwnd.is_null() {
                return;
            }
            let dark = self.theme().dark;
            let on: i32 = dark as i32;
            DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as _, &on as *const i32 as _, 4);
            let cap: u32 = if dark { 0x0020_1E1E } else { 0x00F7_F2F2 };
            DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR as _, &cap as *const u32 as _, 4);
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, self.icon as isize);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, self.icon as isize);
            self.settings = Some(Settings::new(hwnd));
            self.refresh_settings();
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }

    /// Applique un réglage modifié (aperçu en direct sur les widgets).
    fn apply(&mut self, f: F) {
        match f {
            F::Blur | F::BlurAmt | F::AccentAuto => self.ensure_backdrop(),
            F::Theme => sys::set_menu_theme(self.theme().dark),
            F::ClockOn | F::WeatherOn | F::CalOn | F::CountOn => self.sync_slots(),
            F::City => {
                self.status = format!("Recherche de « {} »…", self.cfg.weather.city);
                let _ = self.jobs.send(Job::Geocode(self.cfg.weather.city.clone()));
            }
            F::Acad => {
                self.data.holidays.clear();
                self.status.clear();
                self.refresh_holidays();
            }
            F::Autostart => sys::set_autostart(self.cfg.autostart),
            F::HideWin => self.check_visibility(),
            F::Edit => {
                self.edit = !self.edit;
                self.check_visibility();
            }
            F::Reset => {
                self.cfg.places.clear();
                self.ensure_places();
            }
            F::Folder => sys::open_path(&config::dir()),
            F::MailOn | F::MusicOn => self.sync_slots(),
            F::PrinterOn => {
                self.sync_slots();
                self.update_printer_ip();
            }
            F::PrinterIp => self.update_printer_ip(),
            F::AiOn | F::Model | F::OllamaUrl | F::MsClient => {
                self.refresh_accounts();
                self.sync();
            }
            F::GAdd => {
                let _ = self.jobs.send(Job::GoogleAdd);
            }
            F::GDel(i) => {
                let _ = self.jobs.send(Job::GoogleDel(i as usize));
            }
            F::MsAdd => {
                let _ = self.jobs.send(Job::MsAdd(self.cfg.mail.ms_client.trim().to_string()));
            }
            F::MsDel => {
                let _ = self.jobs.send(Job::MsDel);
            }
            F::DcAdd => {
                let (id, secret) = (self.cfg.discord.id.trim().to_string(), self.cfg.discord.secret.trim().to_string());
                if id.is_empty() || secret.is_empty() {
                    self.status = "Discord : entre l'ID d'application et le secret client.".into();
                } else {
                    let _ = self.discord_tx.send(DCmd::Connect(id, secret));
                    self.status = "Discord : accepte la demande qui s'affiche dans Discord.".into();
                }
            }
            F::DcWin | F::DiscordOn => {
                self.sync_slots();
                self.winnotif_on.store(self.cfg.discord.on && self.cfg.discord.win_notifs, std::sync::atomic::Ordering::Relaxed);
                let _ = self.winnotif_tx.send(());
            }
            F::EvCreate => self.create_event(),
            F::PnNext | F::PnHw | F::PnGrades | F::PnInfo => self.sync_slots(),
            F::PnPaste => self.paste_qr(),
            F::PnLogin => self.pn_login(),
            F::PnSync => {
                self.status = "Actualisation de Pronote…".into();
                self.pn_sync();
            }
            F::PnGoogle => {
                if self.cfg.pronote.google {
                    self.status = "Copie vers Google Agenda « Pronote (Givre) »…".into();
                    self.pn_sync();
                }
            }
            F::PnDel => {
                let _ = self.pn_tx.send(PJob::Logout);
            }
            F::Quit => unsafe {
                PostMessageW(self.main, WM_APP_QUIT, 0, 0);
            },
            _ => {}
        }
        if self.shown {
            self.render_all();
        }
        self.save_soon();
    }

    fn with_settings<R>(&mut self, f: impl FnOnce(&mut Settings, &mut Config) -> R) -> Option<R> {
        let mut st = self.settings.take()?;
        let r = f(&mut st, &mut self.cfg);
        self.settings = Some(st);
        Some(r)
    }

    fn settings_down(&mut self, x: f32, y: f32) -> bool {
        let dpi = self.dpi;
        let Some(changed) = self.with_settings(|st, c| settings::down(st, c, x / dpi, y / dpi)) else { return false };
        for f in changed {
            self.apply(f);
        }
        self.refresh_settings();
        self.settings.as_ref().map(|s| s.drag.is_some()).unwrap_or(false)
    }

    fn settings_move(&mut self, x: f32) {
        let dpi = self.dpi;
        if let Some(Some(f)) = self.with_settings(|st, c| settings::drag(st, c, x / dpi)) {
            self.apply(f);
            self.refresh_settings();
        }
    }

    fn settings_up(&mut self) {
        self.with_settings(|st, _| st.drag = None);
    }

    fn settings_wheel(&mut self, d: i16) {
        self.with_settings(|st, _| settings::wheel(st, d));
        self.refresh_settings();
    }

    fn settings_char(&mut self, ch: u32) {
        // Ctrl+V sur la page Comptes (hors champ de texte) : coller le QR code Pronote
        if ch == 22 && self.settings.as_ref().is_some_and(|s| s.page() == 3 && !s.has_focus()) {
            self.paste_qr();
            self.refresh_settings();
            return;
        }
        if let Some(Some(f)) = self.with_settings(|st, c| settings::on_char(st, c, ch)) {
            self.apply(f);
        }
        self.refresh_settings();
    }

    fn settings_key(&mut self, vk: u32) {
        self.with_settings(|st, _| settings::on_key(st, vk));
        self.refresh_settings();
    }

    // ---------- Souris sur les widgets ----------

    fn slot_of(&self, hwnd: HWND) -> Option<usize> {
        self.slots.iter().position(|s| s.hwnd == hwnd)
    }

    fn click(&mut self, hwnd: HWND, x: i32, y: i32) {
        let Some(i) = self.slot_of(hwnd) else { return };
        let (fx, fy) = (x as f32, y as f32);
        let act = self.slots[i]
            .hits
            .iter()
            .find(|h| fx >= h.x && fx < h.x + h.w && fy >= h.y && fy < h.y + h.h)
            .map(|h| h.act);
        match act {
            Some(Action::PrevMonth) => self.month_off -= 1,
            Some(Action::NextMonth) => self.month_off += 1,
            Some(Action::Today) => {
                self.month_off = 0;
                self.sel_day = None;
            }
            Some(Action::Refresh) => self.refresh_weather(),
            Some(Action::Day(d)) => {
                // 1er clic : affiche la journée en bas ; 2e clic sur le même jour : nouvel événement
                if self.sel_day == Some(d) {
                    self.cfg.draft.date = d.format("%d/%m/%Y").to_string();
                    self.open_settings_at(2);
                    return;
                }
                self.sel_day = Some(d);
            }
            Some(Action::Homework(n)) => {
                if let Some(h) = self.data.pronote.homework.get_mut(n as usize) {
                    h.done = !h.done;
                    let _ = self.pn_tx.send(PJob::Done(h.id.clone(), h.done));
                }
                self.render_kind(Kind::Calendar);
            }
            Some(Action::OpenPronote) => {
                let b = self.data.pronote.base.clone();
                if !b.is_empty() {
                    sys::open_url(&format!("{b}/eleve.html"));
                }
                return;
            }
            Some(Action::Prev) => {
                let _ = self.media_tx.send(MCmd::Prev);
                return;
            }
            Some(Action::Toggle) => {
                let _ = self.media_tx.send(MCmd::Toggle);
                return;
            }
            Some(Action::Next) => {
                let _ = self.media_tx.send(MCmd::Next);
                return;
            }
            Some(Action::OpenPrinter) => {
                sys::open_path(std::path::Path::new(&self.cfg.printer.app));
                return;
            }
            Some(Action::Accounts) => {
                self.open_settings_at(3);
                return;
            }
            Some(Action::OpenMail(n)) => {
                if let Some(m) = self.data.mail.mails.get(n as usize) {
                    let (l, f) = (m.link.clone(), m.find.clone());
                    self.open_mail(&l, &f);
                }
                return;
            }
            Some(Action::OpenInbox) => {
                sys::open_url("https://mail.google.com/mail/u/0/#inbox");
                return;
            }
            Some(Action::Ile(_) | Action::IleN(..)) | None => return,
        }
        self.render(i);
    }

    /// Ouvre un mail. Mail Hotmail transféré : Gmail (le mail exact) + Outlook.com avec le sujet copié,
    /// car Outlook.com ne sait pas lancer une recherche depuis une adresse web.
    fn open_mail(&mut self, link: &str, find: &str) {
        if !link.is_empty() {
            sys::open_url(link);
        }
        if !find.is_empty() {
            sys::copy_text(find);
            sys::open_url(accounts::OUTLOOK);
            self.notif_link = None;
            let t = "Sujet copié pour Outlook";
            let b = "Dans Outlook, clique sur Rechercher, fais Ctrl+V puis Entrée.";
            unsafe { tray(self.main, self.icon, NIM_MODIFY, Some((t, b))) };
        }
    }

    fn wheel(&mut self, hwnd: HWND, d: i16, ctrl: bool) {
        let Some(i) = self.slot_of(hwnd) else { return };
        let k = self.slots[i].kind;
        if (self.edit || ctrl) && !self.cfg.locked {
            let mut p = self.place(k);
            p.scale = (p.scale + 0.05 * d.signum() as f32).clamp(0.6, 2.0);
            self.cfg.places.insert(k.key().into(), p);
            self.save_soon();
        } else if k == Kind::Calendar {
            self.month_off -= d.signum() as i32;
        } else {
            return;
        }
        self.render(i);
    }

    /// Bouton gauche enfoncé : on retient le point de départ (clic simple ou glisser).
    fn press(&mut self, hwnd: HWND) -> bool {
        let Some(i) = self.slot_of(hwnd) else { return false };
        let p = self.place(self.slots[i].kind);
        let mut c = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut c) };
        self.drag = Some(Drag { hwnd, sx: c.x, sy: c.y, px: p.x, py: p.y, moved: false });
        true
    }

    /// Souris déplacée bouton enfoncé : le widget suit (aimanté à la grille).
    fn drag_to(&mut self) {
        if self.cfg.locked {
            return;
        }
        let Some(d) = &mut self.drag else { return };
        let mut c = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut c) };
        let (dx, dy) = (c.x - d.sx, c.y - d.sy);
        if !d.moved && dx * dx + dy * dy < 25 {
            return;
        }
        d.moved = true;
        let g = (self.cfg.grid * self.dpi).round() as i32;
        let (nx, ny, hwnd) = (snap(d.px + dx, g), snap(d.py + dy, g), d.hwnd);
        let Some(i) = self.slot_of(hwnd) else { return };
        let k = self.slots[i].kind;
        let mut p = self.place(k);
        if p.x == nx && p.y == ny {
            return;
        }
        p.x = nx;
        p.y = ny;
        self.cfg.places.insert(k.key().into(), p);
        self.render(i);
    }

    /// Bouton relâché : fin du glisser, ou clic simple si la souris n'a pas bougé.
    fn release(&mut self, hwnd: HWND, x: i32, y: i32) {
        match self.drag.take() {
            Some(d) if d.moved => self.save_soon(),
            _ => self.click(hwnd, x, y),
        }
    }

    fn dragging(&self) -> bool {
        self.drag.as_ref().map(|d| d.moved).unwrap_or(false)
    }

    // ---------- Dynamic Island ----------

    /// Active / désactive le mode Dynamic Island (les widgets du bureau sont cachés pendant ce temps).
    fn ile_set(&mut self, on: bool) {
        if self.cfg.ile.on != on {
            self.cfg.ile.on = on;
            self.save_soon();
        }
        unsafe {
            if on {
                if self.ile.is_none() {
                    let hwnd = CreateWindowExW(
                        WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                        wide(ILE_CLASS).as_ptr(),
                        wide("Givre Island").as_ptr(),
                        WS_POPUP,
                        0,
                        0,
                        1,
                        1,
                        null_mut(),
                        null_mut(),
                        GetModuleHandleW(null()),
                        null(),
                    );
                    if hwnd.is_null() {
                        sys::log("île : impossible de créer la fenêtre");
                        return;
                    }
                    self.ile = Some(island::Island::new(hwnd));
                    windows_sys::Win32::System::DataExchange::AddClipboardFormatListener(hwnd);
                    sys::log("mode Dynamic Island activé");
                }
                self.set_shown(false);
                self.ensure_backdrop();
                self.ile_render();
                if let Some(st) = &self.ile {
                    ShowWindow(st.hwnd, SW_SHOWNOACTIVATE);
                }
                self.ile_fullscreen_check();
                SetTimer(self.main, T_SYNC, (self.cfg.ile.mail_refresh.max(1.0) * 60_000.0) as u32, None);
                SetTimer(self.main, T_ILE_SLOW, 10_000, None);
                SetTimer(self.main, T_ILE_WATCH, 1000, None);
                self.ile_slow();
            } else {
                if let Some(st) = self.ile.take() {
                    DestroyWindow(st.hwnd);
                    sys::log("mode Dynamic Island désactivé");
                }
                for t in [T_ILE_ANIM, T_ILE_LEAVE, T_ILE_OPEN, T_ILE_SLOW, T_ILE_WATCH] {
                    KillTimer(self.main, t);
                }
                SetTimer(self.main, T_SYNC, 5 * 60_000, None);
                self.check_visibility();
            }
        }
        self.update_fx(None);
    }

    /// Montre l'île (si elle s'était cachée toute seule) et relance le compte à rebours.
    fn ile_wake(&mut self) {
        let Some(st) = &mut self.ile else { return };
        st.seen = std::time::Instant::now();
        st.edge_since = None;
        if !st.gone {
            return;
        }
        st.gone = false;
        if !st.hidden {
            unsafe {
                ShowWindow(st.hwnd, SW_SHOWNOACTIVATE);
                SetWindowPos(st.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
        unsafe { SetTimer(self.main, T_ILE_WATCH, 1000, None) };
        self.ile_anim();
        self.ile_render();
    }

    /// Toutes les secondes (île visible) ou 20 fois par seconde (île cachée, simple suivi de la souris) :
    /// cache l'île après N s sans survol, la fait revenir quand la souris reste collée en haut.
    fn ile_watch(&mut self) {
        let busy = self.ring.is_some() || self.jarvis.active();
        let c = &self.cfg.ile;
        let Some(st) = &mut self.ile else { return };
        let now = std::time::Instant::now();
        if c.always_open || !c.auto_hide || c.auto_hide_secs <= 0.0 || st.typing || st.want || st.tracking || st.toast.is_some() || busy {
            st.seen = now;
        }
        if st.gone {
            let mut p = POINT { x: 0, y: 0 };
            unsafe { GetCursorPos(&mut p) };
            let (x0, x1) = st.zone;
            if p.y <= 1 && p.x >= x0 - 20 && p.x <= x1 + 20 {
                let since = *st.edge_since.get_or_insert(now);
                if since.elapsed().as_secs_f32() >= c.reveal_secs.max(0.05) {
                    self.ile_wake();
                }
            } else {
                st.edge_since = None;
            }
        } else if c.auto_hide && !c.always_open && c.auto_hide_secs > 0.0 && st.seen.elapsed().as_secs_f32() >= c.auto_hide_secs {
            st.gone = true;
            st.edge_since = None;
            unsafe { SetTimer(self.main, T_ILE_WATCH, 50, None) };
            self.ile_anim();
        }
    }

    fn ile_model(&self) -> String {
        match self.cfg.ile.ai_model {
            1 => {
                let m = self.cfg.mail.model.trim();
                if m.is_empty() { "llama3.2".into() } else { m.into() }
            }
            i => brain::MODELS[i.min(brain::MODELS.len() - 1)].to_string(),
        }
    }

    /// Construit le contexte de dessin et appelle `f`.
    fn ile_with<T>(&self, f: impl FnOnce(&island_draw::R) -> T) -> Option<T> {
        let st = self.ile.as_ref()?;
        let mut c = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut c) };
        let (sw, _) = sys::screen_size();
        let r = island_draw::R {
            cfg: &self.cfg,
            fonts: self.ile_fonts.as_ref().map(|x| &x.1).unwrap_or(&self.fonts),
            dpi: self.dpi,
            backdrop: self.backdrop.as_ref(),
            d: &self.data,
            st,
            clocks: &self.clocks,
            chat: &self.chat,
            ring: self.ring.as_ref(),
            jarvis: &self.jarvis,
            notifs: &self.notifs,
            discord_unread: self.discord_unread,
            now: Local::now().naive_local(),
            sw,
            mouse: (c.x, c.y),
            model: self.ile_model(),
            jarvis_cmds: self.jarvis_cmds,
            clips: &self.clips,
            mail_ai: &self.mail_ai,
        };
        Some(f(&r))
    }

    fn ile_render(&mut self) {
        if self.ile.as_ref().is_none_or(|s| s.hidden || (s.gone && s.slide >= 1.0)) {
            self.update_fx(None);
            return;
        }
        let font = self.cfg.ile.font;
        if self.ile_fonts.as_ref().is_none_or(|f| f.0 != font) {
            self.ile_fonts = Fonts::load_family(font).map(|f| (font, f));
        }
        let res = self.ile_with(|r| (island_draw::render(r), island_draw::fx_interval(r)));
        let Some((img, fx)) = res else { return };
        if let (Some((pm, x, y, hits)), Some(st)) = (img, &mut self.ile) {
            unsafe { gfx::present(st.hwnd, &pm, x, y) };
            st.hits = hits;
            st.win = (x, y);
            if !st.gone {
                st.zone = (x, x + pm.width() as i32);
            }
        }
        self.update_fx(fx);
    }

    /// Minuterie des animations : démarrée seulement quand il y a quelque chose qui bouge.
    fn update_fx(&mut self, fx: Option<u32>) {
        let visible = self.ile.as_ref().is_some_and(|s| !s.hidden && !(s.gone && s.slide >= 1.0));
        let want = if visible { fx } else if self.clocks.busy() || self.ring.is_some() { Some(500) } else { None };
        if want != self.fx_cur {
            self.fx_cur = want;
            unsafe {
                match want {
                    Some(ms) => SetTimer(self.main, T_ILE_FX, ms, None),
                    None => {
                        KillTimer(self.main, T_ILE_FX);
                        0
                    }
                };
            }
        }
    }

    /// Chaque minute : alarme, heure.
    fn ile_tick(&mut self) {
        let now = Local::now();
        if self.clocks.alarm_due(&self.cfg.ile, now.naive_local()) {
            let label = format!("Alarme {:02}:{:02}", self.cfg.ile.alarm_h, self.cfg.ile.alarm_m);
            self.ile_ring(label);
        }
        self.ile_render();
        let ms = now.timestamp_subsec_millis().min(999);
        let wait = (60 - now.second()) * 1000 - ms;
        unsafe { SetTimer(self.main, T_ILE_CLOCK, wait + 20, None) };
    }

    /// Toutes les 10 s : plein écran, non-lus Discord dans Chrome.
    fn ile_slow(&mut self) {
        self.ile_fullscreen_check();
        let u = if self.cfg.ile.discord_title { island::chrome_discord_unread() } else { None };
        if u != self.discord_unread {
            let old = self.discord_unread.unwrap_or(0);
            self.discord_unread = u;
            if let Some(n) = u.filter(|n| *n > old) {
                let body = format!("{n} message{} non lu{} dans l'onglet Discord de Chrome.", if n > 1 { "s" } else { "" }, if n > 1 { "s" } else { "" });
                self.notify(island::Src::Discord, "Discord", &body, None);
            }
            self.ile_render();
        }
    }

    /// Animations (et fin des minuteurs, durée des notifications).
    fn ile_fx(&mut self) {
        for name in self.clocks.finished() {
            self.ile_ring(format!("Minuteur : {name}"));
        }
        if let Some(msg) = self.clocks.pomo_check(self.cfg.ile.pomo_break) {
            self.ile_ring(format!("Focus : {msg}"));
        }
        if let Some(st) = &mut self.ile {
            let n = self.cfg.ile.n_secs.max(1.0);
            let mut changed = false;
            if st.toast.as_ref().is_some_and(|(_, t)| t.elapsed().as_secs_f32() > n) {
                st.toast = None;
                changed = true;
            }
            if st.confetti.is_some_and(|t| t.elapsed().as_secs_f32() > 4.2) {
                st.confetti = None;
            }
            let mode = st.mode(self.ring.is_some(), self.jarvis.active(), &self.cfg.ile);
            if changed || mode != self.ile_last_mode {
                self.ile_last_mode = mode;
                self.ile_anim();
            }
        }
        if self.ring.as_ref().is_some_and(|r| r.since.elapsed().as_secs_f32() > self.cfg.ile.ring_minutes * 60.0) {
            self.ile_stop_ring(false);
        }
        if self.ile.is_some() {
            self.ile_render();
        } else {
            self.update_fx(None);
        }
    }

    fn beep(&self, kind: usize) {
        let k = match kind {
            0 => MB_OK,
            1 => MB_ICONASTERISK,
            2 => MB_ICONEXCLAMATION,
            3 => MB_ICONHAND,
            _ => return,
        };
        unsafe { windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(k) };
    }

    fn ile_ring(&mut self, label: String) {
        sys::log(&format!("île : sonnerie ({label})"));
        self.ring = Some(island::Ring { label: label.clone(), since: std::time::Instant::now() });
        self.ile_wake();
        if let Some(st) = &mut self.ile {
            if st.hidden {
                st.hidden = false;
                unsafe { ShowWindow(st.hwnd, SW_SHOWNOACTIVATE) };
            }
        }
        self.beep(self.cfg.ile.ring_sound);
        unsafe { SetTimer(self.main, T_RING, 1500, None) };
        let body = format!("Il est {}.", Local::now().format("%H:%M"));
        let (_, toast) = island::Src::Timer.wanted(&self.cfg.ile);
        if toast || self.ile.is_none() {
            unsafe { tray(self.main, self.icon, NIM_MODIFY, Some((label.as_str(), body.as_str()))) };
        }
        self.notif_next += 1;
        let src = if label.starts_with("Alarme") { island::Src::Alarm } else { island::Src::Timer };
        self.notifs.push(island::INotif { id: self.notif_next, src, title: label, body, time: Local::now().naive_local() });
        self.ile_anim();
        self.ile_render();
    }

    fn ile_ring_tick(&mut self) {
        if self.ring.is_some() {
            self.beep(self.cfg.ile.ring_sound);
        } else {
            unsafe { KillTimer(self.main, T_RING) };
        }
    }

    fn ile_stop_ring(&mut self, snooze: bool) {
        unsafe { KillTimer(self.main, T_RING) };
        if self.ring.take().is_none() {
            return;
        }
        if snooze {
            self.clocks.snooze = Some(Local::now().naive_local() + chrono::Duration::seconds((self.cfg.ile.snooze_min * 60.0) as i64));
        }
        self.ile_anim();
        unsafe { SetTimer(self.main, T_ILE_LEAVE, self.cfg.ile.close_delay.max(100.0) as u32 + 800, None) };
    }

    fn ile_anim(&mut self) {
        if self.ile.is_some() {
            unsafe { SetTimer(self.main, T_ILE_ANIM, 15, None) };
        }
    }

    fn ile_anim_step(&mut self) {
        let tgt = self.ile_with(island_draw::target);
        let more = match (tgt, &mut self.ile) {
            (Some((mode, tw, th)), Some(st)) => {
                self.ile_last_mode = mode;
                let a = st.step(tw, th, &self.cfg.ile);
                let b = st.step_slide(&self.cfg.ile);
                if st.gone && st.slide >= 1.0 {
                    // partie : fenêtre cachée (plus de clic bloqué, plus de dessin)
                    unsafe { ShowWindow(st.hwnd, SW_HIDE) };
                }
                a || b
            }
            _ => false,
        };
        if !more {
            unsafe { KillTimer(self.main, T_ILE_ANIM) };
        }
        self.ile_render();
    }

    fn ile_open(&mut self) {
        let Some(st) = &mut self.ile else { return };
        if !st.want {
            st.want = true;
            if self.cfg.ile.open_sound {
                sys::soft_sound(0);
            }
            self.ile_anim();
        }
    }

    /// Souris sur l'île (true) ou partie (false).
    fn ile_hover(&mut self, on: bool) {
        if let Some(st) = &mut self.ile {
            st.seen = std::time::Instant::now();
        }
        let Some(st) = &self.ile else { return };
        let c = &self.cfg.ile;
        unsafe {
            if on {
                KillTimer(self.main, T_ILE_LEAVE);
                if !st.want && !c.open_on_click {
                    if c.open_delay < 10.0 {
                        self.ile_open();
                    } else {
                        SetTimer(self.main, T_ILE_OPEN, c.open_delay as u32, None);
                    }
                }
            } else {
                KillTimer(self.main, T_ILE_OPEN);
                if !st.typing {
                    SetTimer(self.main, T_ILE_LEAVE, c.close_delay.max(100.0) as u32, None);
                }
            }
        }
    }

    fn cursor_in_ile(&self) -> bool {
        let Some(st) = &self.ile else { return false };
        let mut c = POINT { x: 0, y: 0 };
        let mut r: RECT = unsafe { std::mem::zeroed() };
        unsafe {
            GetCursorPos(&mut c);
            GetWindowRect(st.hwnd, &mut r);
        }
        c.x >= r.left && c.x < r.right && c.y >= r.top && c.y < r.bottom
    }

    /// Délai après le départ de la souris : on referme si elle n'est vraiment plus dessus.
    fn ile_leave(&mut self) {
        unsafe { KillTimer(self.main, T_ILE_LEAVE) };
        if self.cursor_in_ile() {
            return;
        }
        let Some(st) = &mut self.ile else { return };
        if st.typing {
            return;
        }
        if st.want {
            st.want = false;
            st.view = island::View::Main;
            st.sel = None;
            st.scroll = 0;
            self.ile_anim();
        }
    }

    fn ile_fullscreen_check(&mut self) {
        let fs = self.cfg.ile.hide_fullscreen && island::fullscreen_app();
        let ring = self.ring.is_some();
        let Some(st) = &mut self.ile else { return };
        let hide = fs && !ring;
        if hide == st.hidden {
            return;
        }
        st.hidden = hide;
        let h = st.hwnd;
        if !hide && st.gone && st.slide >= 1.0 {
            return; // cachée toute seule : elle reviendra par le haut de l'écran
        }
        unsafe {
            if hide {
                ShowWindow(h, SW_HIDE);
            } else {
                ShowWindow(h, SW_SHOWNOACTIVATE);
                SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
        if !hide {
            self.ile_render();
        }
    }

    fn ile_hit(&self, x: i32, y: i32) -> Option<Action> {
        let st = self.ile.as_ref()?;
        let (fx, fy) = (x as f32, y as f32);
        st.hits.iter().rev().find(|h| fx >= h.x && fx < h.x + h.w && fy >= h.y && fy < h.y + h.h).map(|h| h.act)
    }

    fn set_view(&mut self, v: island::View, sel: Option<usize>) {
        if let Some(st) = &mut self.ile {
            if st.view != v {
                st.scroll = 0;
            }
            st.view = v;
            st.sel = sel;
            if v == island::View::Notifs {
                st.unseen = 0;
            }
        }
        self.ile_anim();
    }

    fn ile_click(&mut self, x: i32, y: i32, dbl: bool) {
        let Some(st) = &self.ile else { return };
        let open = st.want || self.cfg.ile.always_open || st.typing;
        if !open && self.ring.is_none() && st.toast.is_none() {
            // pilule fermée : clic sur l'indicateur de Jarvis = parler, ailleurs = ouvrir
            if matches!(self.ile_hit(x, y), Some(Action::Ile(island::A_JARVIS_LISTEN))) && !dbl {
                jarvis_link::fire("/ecouter", serde_json::json!({}));
                return;
            }
            self.ile_open();
            return;
        }
        if dbl {
            // double-clic : on reprend l'action du premier clic (la page a pu changer entre-temps)
            let act = self.ile_last_act.take();
            let pronote = |a: &App| {
                let b = a.data.pronote.base.clone();
                if !b.is_empty() {
                    sys::open_url(&format!("{b}/eleve.html"));
                }
            };
            match act {
                Some(Action::IleN(island::N_HW | island::N_GRADE, _)) => pronote(self),
                Some(Action::IleN(island::N_VIEW, v)) if v == island::View::Homework as u16 || v == island::View::Grades as u16 => pronote(self),
                Some(Action::IleN(island::N_MAIL, i)) => {
                    if let Some(m) = self.data.mail.mails.get(i as usize) {
                        let (l, f) = (m.link.clone(), m.find.clone());
                        self.open_mail(&l, &f);
                    }
                }
                Some(Action::IleN(island::N_VIEW, v)) if v == island::View::Mails as u16 => sys::open_url("https://mail.google.com/mail/u/0/#inbox"),
                Some(Action::IleN(island::N_DISCORD, _)) => sys::open_url("https://discord.com/channels/@me"),
                Some(Action::IleN(island::N_VIEW, v)) if v == island::View::Discord as u16 => sys::open_url("https://discord.com/channels/@me"),
                _ => {}
            }
            return;
        }
        let Some(act) = self.ile_hit(x, y) else { return };
        self.ile_last_act = Some(act);
        let hwnd = st.hwnd;
        let now = Local::now().naive_local();
        match act {
            Action::Ile(code) => {
                let a = &mut self.cfg.ile;
                match code {
                    island::A_ALARM => a.alarm_on = !a.alarm_on,
                    island::A_ALARM_DAYS => a.alarm_days = (a.alarm_days + 1) % 3,
                    island::A_H_DOWN => a.alarm_h = (a.alarm_h + 23) % 24,
                    island::A_H_UP => a.alarm_h = (a.alarm_h + 1) % 24,
                    island::A_M_DOWN => a.alarm_m = (a.alarm_m + 55) / 5 * 5 % 60,
                    island::A_M_UP => a.alarm_m = (a.alarm_m / 5 * 5 + 5) % 60,
                    island::A_STOP => return self.ile_stop_ring(false),
                    island::A_SNOOZE => return self.ile_stop_ring(true),
                    island::A_ASK => {
                        if let Some(st) = &mut self.ile {
                            st.typing = true;
                        }
                        // l'île prend le clavier le temps de taper la question
                        unsafe {
                            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE as isize));
                            SetForegroundWindow(hwnd);
                            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(hwnd);
                        }
                    }
                    island::A_SEND => self.ile_send(),
                    island::A_OLLAMA => self.refresh_accounts(),
                    island::A_SETTINGS => self.open_ile_settings(),
                    island::A_BACK => self.set_view(island::View::Main, None),
                    island::A_CHRONO => self.clocks.chrono_toggle(),
                    island::A_CHRONO_RESET => self.clocks.chrono_cmd("zero"),
                    island::A_NEWCHAT => {
                        self.chat.log.clear();
                        self.chat.error.clear();
                        if self.jarvis.online() {
                            jarvis_link::fire("/oublier", serde_json::json!({}));
                        }
                    }
                    island::A_JARVIS_LISTEN => jarvis_link::fire("/ecouter", serde_json::json!({})),
                    island::A_JARVIS_MIC => {
                        self.jarvis.micro = !self.jarvis.micro;
                        jarvis_link::fire("/micro", serde_json::json!({"actif": self.jarvis.micro}));
                    }
                    island::A_CLEAR_NOTIFS => {
                        self.notifs.clear();
                        if let Some(st) = &mut self.ile {
                            st.unseen = 0;
                        }
                    }
                    island::A_DISCORD_OPEN => sys::open_url("https://discord.com/channels/@me"),
                    island::A_MENU => unsafe {
                        PostMessageW(self.main, WM_APP_MENU, 0, 0);
                    },
                    island::A_POMO => self.clocks.pomo_toggle(self.cfg.ile.pomo_work),
                    island::A_POMO_RESET => self.clocks.pomo_reset(),
                    island::A_PREV => {
                        let _ = self.media_tx.send(MCmd::Prev);
                    }
                    island::A_PLAY => {
                        let _ = self.media_tx.send(MCmd::Toggle);
                    }
                    island::A_NEXT => {
                        let _ = self.media_tx.send(MCmd::Next);
                    }
                    island::A_VOLUME => volume_key(0xAD),
                    island::A_VOL_UP => volume_key(0xAF),
                    island::A_VOL_DOWN => volume_key(0xAE),
                    island::A_DND => {
                        self.cfg.ile.dnd = !self.cfg.ile.dnd;
                        self.save_soon();
                    }
                    island::A_NOTIF_SETTINGS => self.open_ile_settings_tab(10),
                    island::A_TABS_SETTINGS => self.open_ile_settings_tab(13),
                    _ => {}
                }
                if matches!(code, island::A_ALARM | island::A_ALARM_DAYS | island::A_H_DOWN | island::A_H_UP | island::A_M_DOWN | island::A_M_UP) {
                    self.save_soon();
                }
            }
            Action::IleN(code, n) => match code {
                island::N_VIEW => self.set_view(island::View::from(n), None),
                island::N_HW => {
                    // 1er clic : détails ; clic sur le devoir déjà choisi : le cocher
                    let cur = self.ile.as_ref().map(|s| (s.view, s.sel));
                    if cur == Some((island::View::Homework, Some(n as usize))) {
                        self.toggle_homework(n as usize);
                    } else {
                        self.set_view(island::View::Homework, Some(n as usize));
                    }
                }
                island::N_HW_DONE => self.toggle_homework(n as usize),
                island::N_GRADE => self.set_view(island::View::Grades, Some(n as usize)),
                island::N_MAIL => {
                    let i = n as usize;
                    if self.mail_ai.n != Some(i) {
                        self.mail_ai = island::MailAi::default();
                    }
                    self.set_view(island::View::Mails, Some(i));
                    // Jarvis sait de quel mail on parle (« fais-moi un résumé » à la voix)
                    if let Some(m) = self.data.mail.mails.get(i) {
                        jarvis_link::fire("/selection", serde_json::json!({"type": "mail", "n": i, "de": m.from, "sujet": m.subject}));
                    }
                }
                island::N_MAIL_ASK => {
                    if let Some(i) = self.ile.as_ref().and_then(|s| s.sel) {
                        let q = if n == 0 { "Fais-moi un résumé de ce mail." } else { "Que pourrais-je répondre à ce mail ?" };
                        self.mail_ask(i, q.to_string());
                    }
                }
                island::N_DISCORD => self.set_view(island::View::Discord, Some(n as usize)),
                island::N_TAB => {
                    if let Some(tab) = island::Tab::ALL.get(n as usize) {
                        self.set_view(tab.view(), None);
                    }
                }
                island::N_CLIP => {
                    if let Some(s) = self.clips.get(n as usize).cloned() {
                        self.clip_skip = Some(s.clone());
                        sys::copy_text(&s);
                    }
                }
                island::N_TIMER_ADD => {
                    self.clocks.add_timer(n as i64, "");
                }
                island::N_TIMER_PAUSE => self.clocks.toggle_timer(n as usize),
                island::N_TIMER_DEL => {
                    if (n as usize) < self.clocks.timers.len() {
                        self.clocks.timers.remove(n as usize);
                    }
                }
                _ => {}
            },
            Action::OpenMail(n) => {
                if let Some(m) = self.data.mail.mails.get(n as usize) {
                    let (l, f) = (m.link.clone(), m.find.clone());
                    self.open_mail(&l, &f);
                }
            }
            Action::OpenInbox => sys::open_url("https://mail.google.com/mail/u/0/#inbox"),
            Action::OpenPronote => {
                let b = self.data.pronote.base.clone();
                if !b.is_empty() {
                    sys::open_url(&format!("{b}/eleve.html"));
                }
            }
            Action::Refresh => self.refresh_weather(),
            Action::Accounts => self.open_settings_at(3),
            _ => {}
        }
        let _ = now;
        self.ile_render();
    }

    fn toggle_homework(&mut self, i: usize) {
        if let Some(h) = self.data.pronote.homework.get_mut(i) {
            h.done = !h.done;
            let _ = self.pn_tx.send(PJob::Done(h.id.clone(), h.done));
        }
    }

    fn ile_wheel(&mut self, d: i16, sx: i32, sy: i32) {
        // molette sur le haut-parleur : volume de Windows
        let win = self.ile.as_ref().map(|s| s.win).unwrap_or((0, 0));
        if matches!(self.ile_hit(sx - win.0, sy - win.1), Some(Action::Ile(island::A_VOLUME))) {
            volume_key(if d > 0 { 0xAF } else { 0xAE });
            return;
        }
        let Some(st) = &mut self.ile else { return };
        if st.view == island::View::Main {
            return;
        }
        st.scroll = if d < 0 { st.scroll + 1 } else { st.scroll.saturating_sub(1) };
        self.ile_render();
    }

    /// Envoie la question tapée à l'IA.
    fn ile_send(&mut self) {
        let Some(st) = &mut self.ile else { return };
        let q = st.input.trim().to_string();
        if q.is_empty() {
            return;
        }
        st.input.clear();
        let mail = (st.view == island::View::Mails).then_some(st.sel).flatten();
        match mail {
            Some(i) => self.mail_ask(i, q),
            None => self.ai_ask(q, Origin::Island),
        }
    }

    /// Fin de la saisie : l'île redevient « non activable » (elle ne vole plus le clavier).
    fn ile_end_typing(&mut self) {
        let Some(st) = &mut self.ile else { return };
        if !st.typing {
            return;
        }
        st.typing = false;
        let hwnd = st.hwnd;
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE as isize);
            SetTimer(self.main, T_ILE_LEAVE, self.cfg.ile.close_delay.max(100.0) as u32, None);
        }
        self.ile_render();
    }

    fn ile_char(&mut self, ch: u32) {
        let Some(st) = &mut self.ile else { return };
        if !st.typing {
            return;
        }
        match ch {
            8 => {
                st.input.pop();
            }
            13 => self.ile_send(),
            // Ctrl+V : colle le texte du presse-papiers
            22 => {
                if let Some(t) = sys::clipboard_text() {
                    st.input.push_str(t.replace(['\r', '\n'], " ").trim());
                }
            }
            27 => {
                st.input.clear();
                return self.ile_end_typing();
            }
            c if c >= 32 => {
                if let Some(c) = char::from_u32(c) {
                    if st.input.chars().count() < 400 {
                        st.input.push(c);
                    }
                }
            }
            _ => {}
        }
        self.ile_render();
    }

    // ---------- Notifications ----------

    fn notify(&mut self, src: island::Src, title: &str, body: &str, link: Option<(String, String)>) {
        if !self.cfg.ile.on {
            self.notif_link = link;
            unsafe { tray(self.main, self.icon, NIM_MODIFY, Some((title, body))) };
            return;
        }
        self.notif_next += 1;
        let n = island::INotif { id: self.notif_next, src, title: sys::clean(title), body: sys::clean(body), time: Local::now().naive_local() };
        self.notifs.push(n.clone());
        if self.notifs.len() > 60 {
            self.notifs.remove(0);
        }
        let (isl, toast) = src.wanted(&self.cfg.ile);
        if let Some(st) = &mut self.ile {
            st.unseen += 1;
            if isl {
                st.toast = Some((n, std::time::Instant::now()));
            }
        }
        if toast {
            self.notif_link = link;
            unsafe { tray(self.main, self.icon, NIM_MODIFY, Some((title, body))) };
        }
        if isl {
            self.ile_wake();
        }
        if isl && self.cfg.ile.n_sound {
            sys::soft_sound(1);
        }
        self.ile_anim();
        self.ile_render();
    }

    /// Nouvelle bonne note : confettis (option).
    fn check_good_grades(&mut self, s: &pronote::School) {
        if !self.cfg.ile.on || !self.cfg.ile.confetti || !self.cfg.ile.show_grades || !self.data.pronote.connected {
            return;
        }
        let old: std::collections::HashSet<&str> = self.data.pronote.grades.iter().map(|g| g.id.as_str()).collect();
        let good = s.grades.iter().filter(|g| !old.contains(g.id.as_str())).any(|g| {
            match (pronote::grade_num(&g.value), pronote::grade_num(&g.out_of)) {
                (Some(v), Some(o)) if o > 0.0 => v / o * 20.0 >= self.cfg.ile.confetti_min,
                _ => false,
            }
        });
        if good {
            if let Some(st) = &mut self.ile {
                st.confetti = Some(std::time::Instant::now());
            }
        }
    }

    // ---------- IA ----------

    /// Résumé à jour des données, donné à l'IA dans sa consigne.
    fn ai_context(&self) -> String {
        use chrono::Datelike;
        let c = &self.cfg.ile;
        let now = Local::now().naive_local();
        let wd = now.weekday().num_days_from_monday() as usize;
        let mut s = format!(
            "Nous sommes le {} {} {} {}, il est {}.\n",
            widgets::JOURS[wd].to_lowercase(),
            now.day(),
            widgets::MOIS[now.month0() as usize],
            now.year(),
            now.format("%H:%M")
        );
        if let Some(w) = &self.data.weather {
            s += &format!(
                "Météo à {} : {}, {}, ressenti {}, vent {}.",
                self.cfg.weather.city,
                island_draw::temp_txt(c, w.temp),
                data::wmo_label(w.code, w.day).to_lowercase(),
                island_draw::temp_txt(c, w.feels),
                island_draw::wind_txt(c, w.wind)
            );
            if let Some(r) = &w.rain {
                s += &format!(" {r}.");
            }
            for d in w.days.iter().take(3) {
                s += &format!(" {} : {} à {}, {}.", d.date.format("%d/%m"), island_draw::temp_txt(c, d.min), island_draw::temp_txt(c, d.max), data::wmo_label(d.code, true).to_lowercase());
            }
            s.push('\n');
        }
        let p = &self.data.pronote;
        if p.connected {
            if let Some(l) = p.next(now).0 {
                s += &format!("Prochain cours : {} de {} à {}{} ({}).\n", l.subject, l.start.format("%H:%M"), l.end.format("%H:%M"), if l.room.is_empty() { String::new() } else { format!(", salle {}", l.room) }, widgets::countdown_txt(l, now));
            }
            let today = now.date();
            let hw: Vec<String> = p
                .homework
                .iter()
                .filter(|h| h.due >= today)
                .take(10)
                .map(|h| format!("pour le {} en {} : {}{}", h.due.format("%d/%m"), h.subject, h.text.chars().take(160).collect::<String>(), if h.done { " (fait)" } else { "" }))
                .collect();
            s += &if hw.is_empty() { "Aucun devoir à venir.\n".to_string() } else { format!("Devoirs : {}.\n", hw.join(" ; ")) };
            let g: Vec<String> = p.grades.iter().take(6).map(|g| format!("{} {}/{} le {}", g.subject, pronote::grade_txt(&g.value), g.out_of, g.date.format("%d/%m"))).collect();
            if !g.is_empty() {
                s += &format!("Dernières notes : {}.", g.join(", "));
            }
            if !p.overall.is_empty() {
                s += &format!(" Moyenne générale : {}.", p.overall);
            }
            s.push('\n');
        } else {
            s += "Pronote n'est pas connecté.\n";
        }
        let mb = &self.data.mail;
        if mb.accounts > 0 {
            let list: Vec<String> = mb
                .mails
                .iter()
                .enumerate()
                .take(10)
                .map(|(i, m)| format!("[{i}] {} — {}{}", m.from, m.subject, if island_draw::is_important(m, c) { " (important)" } else { "" }))
                .collect();
            s += &format!("Mails non lus ({}) : {}.\n", mb.mails.len(), if list.is_empty() { "aucun".into() } else { list.join(" ; ") });
        }
        let dc: Vec<String> = self.data.discord.iter().rev().take(3).map(|n| format!("{} : {}", n.title, n.body.chars().take(100).collect::<String>())).collect();
        if !dc.is_empty() {
            s += &format!("Derniers messages Discord : {}.\n", dc.join(" ; "));
        }
        let k = &self.clocks;
        let mut tm: Vec<String> = k.timers.iter().map(|t| format!("minuteur « {} » : {} restant{}", t.name, island::fmt_dur(t.remaining()), if t.end.is_none() { " (en pause)" } else { "" })).collect();
        if k.chrono_since.is_some() || !k.chrono_acc.is_zero() {
            tm.push(format!("chronomètre : {}{}", island::fmt_dur(k.chrono().as_secs_f64()), if k.chrono_since.is_some() { " (en marche)" } else { " (en pause)" }));
        }
        s += &format!("Minuteurs : {}. ", if tm.is_empty() { "aucun".into() } else { tm.join(" ; ") });
        s += &if c.alarm_on { format!("Alarme réglée à {:02}:{:02}.\n", c.alarm_h, c.alarm_m) } else { "Pas d'alarme.\n".into() };
        s
    }

    fn ai_ask(&mut self, q: String, origin: Origin) {
        let c = &self.cfg.ile;
        let n = c.ai_history.round().max(0.0) as usize;
        let done: Vec<(String, String)> = self.chat.log.iter().filter(|(_, a)| !a.is_empty()).cloned().collect();
        let history = done[done.len().saturating_sub(n)..].to_vec();
        let ctx = if c.ai_context { self.ai_context() } else { String::new() };
        let json = c.ai_actions;
        self.chat.next_id += 1;
        let id = self.chat.next_id;
        let fallback = self.cfg.mail.model.trim().to_string();
        let req = brain::AskReq {
            id,
            model: self.ile_model(),
            fallback: if fallback.is_empty() { "llama3.2".into() } else { fallback },
            url: self.cfg.mail.ollama_url.trim().to_string(),
            system: brain::system_prompt(c.ai_style, &ctx, json),
            history,
            question: q.clone(),
            temp: c.ai_temp,
            json,
        };
        if self.pending.is_empty() && self.chat.log.len() > 30 {
            let cut = self.chat.log.len() - 30;
            self.chat.log.drain(..cut);
        }
        self.chat.log.push((q, String::new()));
        self.chat.error.clear();
        self.chat.asking = true;
        if self.jarvis.online() && matches!(origin, Origin::Island) {
            // Jarvis est là : c'est lui le cerveau (même mémoire que la voix, il fait les actions lui-même)
            self.pending.insert(id, (origin, self.chat.log.len() - 1, false));
            jarvis_link::ask(self.out.clone(), id, req.question);
        } else {
            self.pending.insert(id, (origin, self.chat.log.len() - 1, json));
            let _ = self.jobs.send(Job::Ask(req));
        }
        self.ile_render();
    }

    fn ai_answer(&mut self, id: u64, r: Result<String, String>) {
        let Some((origin, idx, json)) = self.pending.remove(&id) else { return };
        if let Origin::Mail(n) = origin {
            if self.mail_ai.n == Some(n) {
                self.mail_ai.asking = false;
                self.mail_ai.answer = match r {
                    Ok(raw) => jarvis_link::strip_think(&raw),
                    Err(e) => format!("Désolé : {e}"),
                };
            }
            self.ile_render();
            return;
        }
        self.chat.asking = !self.pending.is_empty();
        let text = match r {
            Ok(raw) => {
                let (text, acts) = brain::parse(&raw, json);
                if self.cfg.ile.ai_actions {
                    self.ai_apply(acts);
                }
                if let Some(e) = self.chat.log.get_mut(idx) {
                    e.1 = text.clone();
                }
                text
            }
            Err(e) => {
                sys::log(&format!("IA : {e}"));
                if idx < self.chat.log.len() && self.chat.log[idx].1.is_empty() {
                    self.chat.log.remove(idx);
                    for v in self.pending.values_mut() {
                        if v.1 > idx {
                            v.1 -= 1;
                        }
                    }
                }
                self.chat.error = e.clone();
                format!("Désolé, je n'arrive pas à joindre l'IA : {e}")
            }
        };
        if let Origin::Api(tx) = origin {
            let _ = tx.send((200, serde_json::json!({"ok": true, "reponse": text}).to_string()));
        }
        self.ile_render();
    }

    fn ai_apply(&mut self, acts: Vec<brain::AiAction>) {
        for a in acts {
            sys::log(&format!("IA : action {a:?}"));
            match a {
                brain::AiAction::Timer(secs, name) => {
                    self.clocks.add_timer(secs, &name);
                }
                brain::AiAction::Alarm(h, m) => {
                    self.cfg.ile.alarm_on = true;
                    self.cfg.ile.alarm_h = h;
                    self.cfg.ile.alarm_m = m;
                    self.save_soon();
                }
                brain::AiAction::Chrono(cmd) => self.clocks.chrono_cmd(&cmd),
                brain::AiAction::OpenMail(n) => {
                    if let Some(m) = self.data.mail.mails.get(n) {
                        let (l, f) = (m.link.clone(), m.find.clone());
                        self.open_mail(&l, &f);
                    }
                }
                brain::AiAction::OpenPronote => {
                    let b = self.data.pronote.base.clone();
                    if !b.is_empty() {
                        sys::open_url(&format!("{b}/eleve.html"));
                    }
                }
                brain::AiAction::OpenInbox => sys::open_url("https://mail.google.com/mail/u/0/#inbox"),
                brain::AiAction::Weather => self.refresh_weather(),
                brain::AiAction::Notif(t, b) => self.notify(island::Src::Jarvis, &t, &b, None),
            }
        }
    }

    // ---------- Mails : corps complet, questions à Jarvis ----------

    /// Demande le corps du mail au fil réseau (une seule fois par mail).
    fn fetch_body(&mut self, m: &data::Mail) {
        let busy = self.mail_waiters.iter().any(|w| w.0 == m.id) || self.mail_ask_wait.as_ref().is_some_and(|w| w.0 == m.id);
        if !busy {
            let _ = self.jobs.send(Job::MailBody(m.clone(), self.cfg.mail.ms_client.trim().to_string()));
        }
    }

    fn mail_json(&self, n: usize, m: &data::Mail, body: &str) -> String {
        serde_json::json!({"ok": true, "n": n, "de": m.from, "sujet": m.subject, "date": m.date.format("%Y-%m-%d %H:%M").to_string(), "texte": body}).to_string()
    }

    fn mail_body(&mut self, id: String, r: Result<String, String>) {
        if let Ok(b) = &r {
            self.mail_bodies.insert(id.clone(), b.clone());
        }
        let waiters: Vec<_> = self.mail_waiters.iter().filter(|w| w.0 == id).map(|w| (w.1, w.2.clone())).collect();
        self.mail_waiters.retain(|w| w.0 != id);
        for (n, tx) in waiters {
            let msg = match (&r, self.data.mail.mails.get(n)) {
                (Ok(b), Some(m)) => (200, self.mail_json(n, m, b)),
                (Err(e), _) => (200, serde_json::json!({"ok": false, "erreur": e}).to_string()),
                (_, None) => (200, serde_json::json!({"ok": false, "erreur": "mail introuvable"}).to_string()),
            };
            let _ = tx.send(msg);
        }
        if let Some((wid, n, q, qid)) = self.mail_ask_wait.take() {
            if wid == id {
                let body = r.unwrap_or_else(|e| format!("(corps du mail indisponible : {e})"));
                self.mail_fallback(qid, n, &body, &q);
            } else {
                self.mail_ask_wait = Some((wid, n, q, qid));
            }
        }
    }

    /// Question sur un mail : à Jarvis s'il est là, sinon au cerveau de secours avec le corps du mail.
    fn mail_ask(&mut self, n: usize, demande: String) {
        let Some(m) = self.data.mail.mails.get(n).cloned() else { return };
        self.mail_ai = island::MailAi { n: Some(n), question: demande.clone(), answer: String::new(), asking: true };
        self.chat.next_id += 1;
        let id = self.chat.next_id;
        if self.jarvis.online() {
            let texte = format!("À propos du mail n°{n} (de {}, « {} ») : {demande}", m.from, m.subject);
            self.pending.insert(id, (Origin::Mail(n), usize::MAX, false));
            jarvis_link::ask(self.out.clone(), id, texte);
        } else if let Some(body) = self.mail_bodies.get(&m.id).cloned() {
            self.mail_fallback(id, n, &body, &demande);
        } else {
            self.fetch_body(&m);
            self.mail_ask_wait = Some((m.id.clone(), n, demande, id));
        }
        self.ile_render();
    }

    fn mail_fallback(&mut self, id: u64, n: usize, body: &str, demande: &str) {
        let Some(m) = self.data.mail.mails.get(n) else { return };
        let ctx = format!("Mail n°{n}\nDe : {}\nObjet : {}\nReçu le : {}\n\n{body}", m.from, m.subject, m.date.format("%d/%m/%Y à %H:%M"));
        let fallback = self.cfg.mail.model.trim().to_string();
        let req = brain::AskReq {
            id,
            model: self.ile_model(),
            fallback: if fallback.is_empty() { "llama3.2".into() } else { fallback },
            url: self.cfg.mail.ollama_url.trim().to_string(),
            system: brain::system_prompt(self.cfg.ile.ai_style, &ctx, false),
            history: Vec::new(),
            question: format!("À propos de ce mail : {demande}"),
            temp: self.cfg.ile.ai_temp,
            json: false,
        };
        self.pending.insert(id, (Origin::Mail(n), usize::MAX, false));
        let _ = self.jobs.send(Job::Ask(req));
    }

    // ---------- Jarvis ----------

    /// Nouvelle de GET :47801/etat (toutes les 5 s).
    fn jarvis_link(&mut self, st: Option<(String, bool)>) {
        let was = self.jarvis.online();
        if let Some((etat, micro)) = st {
            self.jarvis.seen = Some(std::time::Instant::now());
            self.jarvis.micro = micro;
            if etat != self.jarvis.state {
                if etat != "repos" {
                    self.ile_wake();
                }
                self.jarvis.state = etat;
                self.jarvis.at = Some(std::time::Instant::now());
            }
        }
        if was != self.jarvis.online() {
            sys::log(if was { "Jarvis : hors ligne" } else { "Jarvis : en ligne" });
            self.ile_anim();
        }
        self.ile_render();
    }

    // ---------- API locale (Jarvis) ----------

    fn api(&mut self, req: api::ApiReq, tx: Sender<(u16, String)>) {
        use serde_json::json;
        let ok = |tx: &Sender<(u16, String)>, v: serde_json::Value| {
            let _ = tx.send((200, v.to_string()));
        };
        match req {
            api::ApiReq::Etat => ok(&tx, self.etat_json()),
            api::ApiReq::Demander { texte, source } => {
                sys::log(&format!("API : question de {}", if source.is_empty() { "?" } else { &source }));
                self.ai_ask(texte, Origin::Api(tx));
            }
            api::ApiReq::Jarvis { etat, texte, question, reponse } => {
                let e = etat.trim().to_lowercase().replace(['é', 'è'], "e");
                if !e.is_empty() && !matches!(e.as_str(), "repos" | "ecoute" | "reflechit" | "parle") {
                    let _ = tx.send((400, json!({"ok": false, "erreur": "etat doit valoir repos, ecoute, reflechit ou parle"}).to_string()));
                    return;
                }
                let now = std::time::Instant::now();
                self.jarvis.seen = Some(now);
                if !e.is_empty() {
                    if e != "repos" {
                        self.ile_wake();
                    }
                    self.jarvis.state = e;
                    self.jarvis.at = Some(now);
                }
                if !texte.is_empty() {
                    self.jarvis.text = texte;
                }
                if !question.is_empty() {
                    let today = Local::now().date_naive();
                    if self.jarvis_day != Some(today) {
                        self.jarvis_day = Some(today);
                        self.jarvis_cmds = 0;
                    }
                    self.jarvis_cmds += 1;
                }
                // échanges à la voix : ajoutés au fil du chat de l'île
                match (question.is_empty(), reponse.is_empty()) {
                    (false, false) => self.chat.log.push((question, reponse)),
                    (false, true) => self.chat.log.push((question, String::new())),
                    (true, false) => match self.chat.log.last_mut().filter(|(_, a)| a.is_empty()) {
                        Some(last) => last.1 = reponse,
                        None => self.chat.log.push((String::new(), reponse)),
                    },
                    (true, true) => {}
                }
                if self.chat.log.len() > 40 && self.pending.is_empty() {
                    let cut = self.chat.log.len() - 40;
                    self.chat.log.drain(..cut);
                }
                ok(&tx, json!({"ok": true}));
                self.ile_anim();
                self.ile_render();
            }
            api::ApiReq::Notif { titre, texte, source } => {
                let (titre, texte) = (sys::clean(&titre), sys::clean(&texte));
                let src = island::Src::parse(&source);
                if src == island::Src::Discord && !self.cfg.ile.discord_servers && sys::discord_is_server(&titre) {
                    ok(&tx, json!({"ok": true, "ignore": "message de serveur"}));
                    return;
                }
                let titre = if titre.is_empty() { src.label().to_string() } else { titre };
                ok(&tx, json!({"ok": true}));
                if src == island::Src::Discord {
                    // MP lus par l'extension Chrome de Jarvis : dans le module Discord, sans doublon
                    if self.data.discord.iter().any(|o| o.title == titre && o.body == texte) {
                        return;
                    }
                    self.data.discord.push(data::Notif { title: titre.clone(), body: texte.clone(), time: Local::now().naive_local() });
                    if self.data.discord.len() > 40 {
                        self.data.discord.remove(0);
                    }
                }
                self.notify(src, &titre, &texte, None);
            }
            api::ApiReq::Timer { secs, nom } => {
                let id = self.clocks.add_timer(secs.round() as i64, &nom);
                ok(&tx, json!({"ok": true, "id": id}));
                self.ile_render();
            }
            api::ApiReq::TimerCancel { id, tous } => {
                let before = self.clocks.timers.len();
                if tous {
                    self.clocks.timers.clear();
                } else if let Some(id) = id {
                    self.clocks.timers.retain(|t| t.id != id);
                }
                let n = before - self.clocks.timers.len();
                ok(&tx, json!({"ok": n > 0 || tous, "annules": n}));
                self.ile_render();
            }
            api::ApiReq::Alarm { heure, active } => {
                if heure.is_empty() {
                    self.cfg.ile.alarm_on = active;
                } else if let Some((h, m)) = brain::parse_hhmm(&heure) {
                    self.cfg.ile.alarm_h = h;
                    self.cfg.ile.alarm_m = m;
                    self.cfg.ile.alarm_on = active;
                } else {
                    let _ = tx.send((400, json!({"ok": false, "erreur": "heure invalide (HH:MM)"}).to_string()));
                    return;
                }
                self.save_soon();
                ok(&tx, json!({"ok": true, "heure": format!("{:02}:{:02}", self.cfg.ile.alarm_h, self.cfg.ile.alarm_m), "active": self.cfg.ile.alarm_on}));
                self.ile_render();
            }
            api::ApiReq::Chrono { commande } => {
                self.clocks.chrono_cmd(&commande.to_lowercase());
                ok(&tx, json!({"ok": true, "secondes": self.clocks.chrono().as_secs_f64().floor(), "en_marche": self.clocks.chrono_since.is_some()}));
                self.ile_render();
            }
            api::ApiReq::Open { quoi, n } => {
                let base = self.data.pronote.base.clone();
                let res = match quoi.as_str() {
                    "pronote" | "devoirs" | "notes" if !base.is_empty() => {
                        sys::open_url(&format!("{base}/eleve.html"));
                        if quoi == "devoirs" {
                            self.ile_open();
                            self.set_view(island::View::Homework, None);
                        }
                        Ok(())
                    }
                    "pronote" | "devoirs" | "notes" => Err("Pronote n'est pas connecté"),
                    "mails" => {
                        sys::open_url("https://mail.google.com/mail/u/0/#inbox");
                        Ok(())
                    }
                    "mail" => match self.data.mail.mails.get(n).map(|m| (m.link.clone(), m.find.clone())) {
                        Some((l, f)) => {
                            self.open_mail(&l, &f);
                            Ok(())
                        }
                        None => Err("pas de mail avec ce numéro"),
                    },
                    _ => Err("« quoi » doit valoir pronote, mails, mail ou devoirs"),
                };
                match res {
                    Ok(()) => ok(&tx, json!({"ok": true})),
                    Err(e) => {
                        let _ = tx.send((400, json!({"ok": false, "erreur": e}).to_string()));
                    }
                }
            }
            api::ApiReq::WeatherRefresh => {
                self.refresh_weather();
                ok(&tx, json!({"ok": true}));
            }
            api::ApiReq::Mail { n } => {
                let Some(m) = self.data.mail.mails.get(n).cloned() else {
                    ok(&tx, json!({"ok": false, "erreur": format!("pas de mail n°{n} (voir /etat)")}));
                    return;
                };
                if let Some(b) = self.mail_bodies.get(&m.id) {
                    let _ = tx.send((200, self.mail_json(n, &m, b)));
                } else {
                    self.fetch_body(&m);
                    self.mail_waiters.push((m.id.clone(), n, tx));
                }
            }
            api::ApiReq::Notifs { depuis } => {
                let list: Vec<_> = self
                    .notifs
                    .iter()
                    .filter(|n| n.id > depuis)
                    .map(|n| {
                        json!({"id": n.id, "source": n.src.key(), "titre": n.title, "texte": n.body, "heure": n.time.format("%H:%M").to_string()})
                    })
                    .collect();
                ok(&tx, json!({"ok": true, "dernier": self.notif_next, "notifs": list}));
            }
        }
    }

    /// Résumé détaillé, en français, servant de contexte à l'IA de Jarvis.
    fn etat_json(&self) -> serde_json::Value {
        use serde_json::json;
        let c = &self.cfg.ile;
        let now = Local::now().naive_local();
        let today = now.date();
        let meteo = match &self.data.weather {
            Some(w) => json!({
                "ville": self.cfg.weather.city,
                "actuelle": {"temperature": w.temp.round(), "ressenti": w.feels.round(), "ciel": data::wmo_label(w.code, w.day),
                    "vent_kmh": w.wind.round(), "humidite": w.humidity.round(), "pluie": w.rain, "lever": w.sunrise, "coucher": w.sunset},
                "prochaines_heures": w.hours.iter().take(8).map(|h| json!({"heure": h.label, "temperature": h.temp.round(), "ciel": data::wmo_label(h.code, h.day)})).collect::<Vec<_>>(),
                "prochains_jours": w.days.iter().take(5).map(|d| json!({"date": d.date.format("%Y-%m-%d").to_string(), "jour": widgets::day_label(d.date, today), "min": d.min.round(), "max": d.max.round(), "ciel": data::wmo_label(d.code, true)})).collect::<Vec<_>>(),
                "phrase": island_draw::weather_phrase(c, w.code, w.day, w.temp, chrono::Datelike::ordinal(&now)),
            }),
            None => json!({"disponible": false}),
        };
        let p = &self.data.pronote;
        let pronote = if !p.connected {
            json!({"connecte": false})
        } else {
            let tomorrow = today + chrono::Duration::days(1);
            let cours = |d: chrono::NaiveDate| {
                p.lessons
                    .iter()
                    .filter(|l| l.start.date() == d)
                    .map(|l| json!({"debut": l.start.format("%H:%M").to_string(), "fin": l.end.format("%H:%M").to_string(), "matiere": l.subject, "salle": l.room, "prof": l.teacher, "annule": l.bad(), "statut": l.status}))
                    .collect::<Vec<_>>()
            };
            json!({
                "connecte": true,
                "eleve": p.name, "classe": p.class,
                "prochain_cours": p.next(now).0.map(|l| json!({"matiere": l.subject, "debut": l.start.format("%H:%M").to_string(), "dans": widgets::countdown_txt(l, now)})),
                "cours_aujourdhui": cours(today),
                "cours_demain": cours(tomorrow),
                "devoirs": p.homework.iter().filter(|h| h.due >= today).take(20).map(|h| json!({"matiere": h.subject, "pour": h.due.format("%Y-%m-%d").to_string(), "pour_quand": widgets::day_label(h.due, today), "texte": h.text, "fait": h.done})).collect::<Vec<_>>(),
                "notes": p.grades.iter().take(5).map(|g| json!({"matiere": g.subject, "note": format!("{}/{}", pronote::grade_txt(&g.value), g.out_of), "moyenne_classe": g.class_avg, "coef": g.coef, "date": g.date.format("%Y-%m-%d").to_string()})).collect::<Vec<_>>(),
                "moyenne_generale": p.overall,
                "messages_non_lus": p.talks.iter().filter(|t| t.unread > 0).map(|t| json!({"sujet": t.subject, "de": t.from, "quand": t.when, "non_lus": t.unread})).collect::<Vec<_>>(),
                "actualites_non_lues": p.news.iter().filter(|n| !n.read).take(5).map(|n| json!({"titre": n.title, "de": n.author, "date": n.date.format("%Y-%m-%d").to_string(), "texte": n.text.chars().take(300).collect::<String>()})).collect::<Vec<_>>(),
                "mis_a_jour": p.updated.map(|u| u.format("%H:%M").to_string()),
            })
        };
        let level = |m: &data::Mail| {
            if m.urgent {
                "urgent"
            } else if m.cat == data::Cat::Important {
                "important"
            } else if m.cat == data::Cat::Reply {
                "à répondre"
            } else {
                "normal"
            }
        };
        let mails: Vec<_> = self
            .data
            .mail
            .mails
            .iter()
            .enumerate()
            .filter(|(_, m)| island_draw::is_important(m, c))
            .take(10)
            .map(|(i, m)| json!({"n": i, "de": m.from, "sujet": m.subject, "extrait": m.snippet.chars().take(200).collect::<String>(), "date": m.date.format("%Y-%m-%d %H:%M").to_string(), "niveau": level(m)}))
            .collect();
        let mails_json = if self.data.mail.accounts == 0 { json!({"connecte": false}) } else { json!(mails) };
        let minuteurs: Vec<_> = self.clocks.timers.iter().map(|t| json!({"id": t.id, "nom": t.name, "restant_s": t.remaining().ceil(), "total_s": t.total, "pause": t.end.is_none()})).collect();
        let discord: Vec<_> = self.data.discord.iter().rev().take(8).map(|n| json!({"de": n.title, "texte": n.body, "heure": n.time.format("%H:%M").to_string()})).collect();
        json!({
            "ok": true,
            "heure": now.format("%H:%M").to_string(),
            "date": now.format("%Y-%m-%d").to_string(),
            "jour": format!("{} {}", widgets::JOURS[chrono::Datelike::weekday(&now).num_days_from_monday() as usize].to_lowercase(), now.format("%d/%m/%Y")),
            "meteo": meteo,
            "pronote": pronote,
            "mails": mails_json,
            "mails_non_lus": self.data.mail.mails.len(),
            "minuteurs": minuteurs,
            "chrono": {"en_marche": self.clocks.chrono_since.is_some(), "secondes": self.clocks.chrono().as_secs_f64().floor()},
            "alarme": {"active": c.alarm_on, "heure": format!("{:02}:{:02}", c.alarm_h, c.alarm_m), "sonne": self.ring.as_ref().map(|r| r.label.clone())},
            "discord": discord,
            "discord_non_lus_chrome": self.discord_unread,
            "jarvis": {"etat": self.jarvis.state, "texte": self.jarvis.text},
            "ile": c.on,
        })
    }

    // ---------- Réglages de l'île ----------

    fn open_ile_settings(&mut self) {
        unsafe {
            if let Some(st) = &self.ile_set_win {
                ShowWindow(st.hwnd, SW_RESTORE);
                SetForegroundWindow(st.hwnd);
                return;
            }
            let s = self.dpi;
            let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
            let mut rc = RECT { left: 0, top: 0, right: (ile_settings::W * s) as i32, bottom: (ile_settings::H * s) as i32 };
            AdjustWindowRectEx(&mut rc, style, 0, 0);
            let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
            let (sw, sh) = sys::screen_size();
            let hwnd = CreateWindowExW(
                0,
                wide(ILE_SET_CLASS).as_ptr(),
                wide("Givre — réglages de l'île").as_ptr(),
                style,
                (sw - w) / 2,
                (sh - h) / 3,
                w,
                h,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            if hwnd.is_null() {
                return;
            }
            let dark: i32 = 1;
            DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as _, 4);
            self.ile_set_win = Some(ile_settings::IleSettings { hwnd, tab: 0, scroll: 0, hits: Vec::new(), pm: None });
            self.ile_settings_draw();
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }

    fn open_ile_settings_tab(&mut self, tab: usize) {
        self.open_ile_settings();
        if let Some(st) = &mut self.ile_set_win {
            st.tab = tab;
            st.scroll = 0;
        }
        self.ile_settings_draw();
    }

    /// Nouveau texte dans le presse-papiers : gardé (10 derniers, en mémoire seulement).
    fn on_clip(&mut self) {
        let Some(s) = sys::clipboard_text() else { return };
        let s = s.trim().to_string();
        if s.is_empty() || self.clip_skip.take().is_some_and(|k| k.trim() == s) {
            return;
        }
        self.clips.retain(|x| *x != s);
        self.clips.insert(0, s.chars().take(2000).collect());
        self.clips.truncate(10);
        if self.ile.as_ref().is_some_and(|st| st.view == island::View::Clips) {
            self.ile_render();
        }
    }

    /// Souris qui bouge sur l'île ouverte : redessin (boutons qui apparaissent au survol).
    fn ile_mouse_move(&mut self) {
        let open = self.ile.as_ref().is_some_and(|s| s.want && s.view == island::View::Main) && self.cfg.ile.layout == 0;
        if open && self.data.media.is_some() {
            self.ile_render();
        }
    }

    fn ile_settings_draw(&mut self) {
        let Some(mut st) = self.ile_set_win.take() else { return };
        let acc = self.cfg.accent;
        let font = self.ile_fonts.as_ref().map(|x| &x.1).unwrap_or(&self.fonts);
        ile_settings::draw(&mut st, &mut self.cfg.ile, font, acc, self.dpi);
        let h = st.hwnd;
        self.ile_set_win = Some(st);
        unsafe { InvalidateRect(h, null(), 0) };
    }

    fn ile_settings_click(&mut self, x: f32, y: f32, dir: i32) {
        let Some(st) = &mut self.ile_set_win else { return };
        let Some(act) = ile_settings::hit(st, x, y) else { return };
        let old = self.cfg.ile.clone();
        let list = ile_settings::opts();
        match act {
            ile_settings::SAct::Tab(i) => {
                st.tab = i;
                st.scroll = 0;
            }
            ile_settings::SAct::Toggle(i) => ile_settings::change(&list[i], &mut self.cfg.ile, 1),
            ile_settings::SAct::Dec(i) => ile_settings::change(&list[i], &mut self.cfg.ile, -1),
            ile_settings::SAct::Inc(i) => ile_settings::change(&list[i], &mut self.cfg.ile, dir),
            ile_settings::SAct::Reset => {
                let t = st.tab;
                ile_settings::reset_tab(t, &mut self.cfg.ile);
            }
        }
        self.ile_cfg_changed(&old);
        self.ile_settings_draw();
    }

    fn ile_settings_wheel(&mut self, d: i16) {
        let Some(st) = &mut self.ile_set_win else { return };
        st.scroll = if d < 0 { st.scroll + 2 } else { st.scroll.saturating_sub(2) };
        self.ile_settings_draw();
    }

    /// Une option de l'île a changé : on applique ce qui doit l'être.
    fn ile_cfg_changed(&mut self, old: &config::IleCfg) {
        let c = self.cfg.ile.clone();
        if c.font != old.font {
            self.ile_fonts = None;
        }
        if c.blur != old.blur || (c.blur_amount - old.blur_amount).abs() > 0.001 {
            self.ensure_backdrop();
        }
        self.api_port.store(if c.api_on { c.api_port.round() as u32 } else { 0 }, Ordering::Relaxed);
        if c.on && (c.mail_refresh - old.mail_refresh).abs() > 0.01 {
            unsafe { SetTimer(self.main, T_SYNC, (c.mail_refresh.max(1.0) * 60_000.0) as u32, None) };
        }
        if !c.auto_hide || c.always_open || c.auto_hide_secs <= 0.0 {
            self.ile_wake();
        }
        if c.hide_fullscreen != old.hide_fullscreen {
            self.ile_fullscreen_check();
        }
        self.save_soon();
        self.ile_anim();
        self.ile_render();
    }
}

/// Touche multimédia de volume (0xAD couper, 0xAE moins, 0xAF plus) : Windows affiche son propre indicateur.
fn volume_key(vk: u8) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, keybd_event};
    unsafe {
        keybd_event(vk, 0, KEYEVENTF_EXTENDEDKEY, 0);
        keybd_event(vk, 0, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP, 0);
    }
}

/// Qui attend la réponse de l'IA.
enum Origin {
    Island,
    /// Question sur le mail n° n (page détail d'un mail).
    Mail(usize),
    Api(Sender<(u16, String)>),
}

// ---------- Icône de la zone de notification ----------

unsafe fn tray(main: HWND, icon: HICON, action: u32, info: Option<(&str, &str)>) {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = main;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_APP_TRAY;
    nid.hIcon = icon;
    sys::copy_wide(&mut nid.szTip, "Givre — widgets de bureau");
    if let Some((t, m)) = info {
        nid.uFlags |= NIF_INFO;
        sys::copy_wide(&mut nid.szInfoTitle, t);
        sys::copy_wide(&mut nid.szInfo, m);
        nid.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
        nid.hBalloonIcon = icon;
    }
    Shell_NotifyIconW(action, &nid);
}

unsafe fn show_menu() {
    let Some((main, edit, auto, locked, ile, hide, grades)) =
        with_app(|a| (a.main, a.edit, a.cfg.autostart, a.cfg.locked, a.cfg.ile.on, a.cfg.ile.auto_hide, a.cfg.ile.show_grades))
    else {
        return;
    };
    let m = CreatePopupMenu();
    let add = |id: usize, text: &str, checked: bool| {
        let flags = MF_STRING | if checked { MF_CHECKED } else { 0 };
        unsafe { AppendMenuW(m, flags, id, wide(text).as_ptr()) };
    };
    add(1, "Paramètres…", false);
    add(2, "Modifier la disposition", edit);
    add(5, "Verrouiller la disposition", locked);
    add(3, "Actualiser", false);
    add(6, "Mode Dynamic Island (haut de l'écran)", ile);
    add(7, "Réglages de l'île…", false);
    add(12, "Cacher l'île automatiquement", hide);
    add(13, "Afficher les notes Pronote sur l'île", grades);
    add(10, "Redémarrer Jarvis", false);
    add(11, "Quitter Jarvis", false);
    AppendMenuW(m, MF_SEPARATOR, 0, null());
    add(4, "Lancer au démarrage de Windows", auto);
    AppendMenuW(m, MF_SEPARATOR, 0, null());
    add(9, "Quitter Givre", false);
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(main);
    let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, main, null());
    PostMessageW(main, WM_NULL, 0, 0);
    DestroyMenu(m);
    match cmd {
        1 => {
            with_app(|a| a.open_settings());
        }
        2 => {
            with_app(|a| {
                a.apply(F::Edit);
                a.refresh_settings();
            });
        }
        3 => {
            with_app(|a| {
                a.refresh_weather();
                a.refresh_holidays();
                a.pn_sync();
            });
        }
        4 => {
            with_app(|a| {
                a.cfg.autostart = !a.cfg.autostart;
                a.apply(F::Autostart);
                a.refresh_settings();
            });
        }
        5 => {
            with_app(|a| {
                a.cfg.locked = !a.cfg.locked;
                a.apply(F::Locked);
                a.refresh_settings();
            });
        }
        6 => {
            with_app(|a| a.ile_set(!ile));
        }
        7 => {
            with_app(|a| a.open_ile_settings());
        }
        10 => {
            with_app(|a| jarvis_link::restart(a.cfg.ile.jarvis_path.clone()));
        }
        12 | 13 => {
            with_app(|a| {
                let old = a.cfg.ile.clone();
                if cmd == 12 {
                    a.cfg.ile.auto_hide = !hide;
                } else {
                    a.cfg.ile.show_grades = !grades;
                }
                a.ile_cfg_changed(&old);
                a.ile_settings_draw();
            });
        }
        11 => {
            jarvis_link::fire("/quitter", serde_json::json!({}));
        }
        9 => {
            DestroyWindow(main);
        }
        _ => {}
    }
}

// ---------- Procédures de fenêtres ----------

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            match wp {
                T_CLOCK => {
                    with_app(|a| a.tick());
                }
                T_VIS | T_SAFE => {
                    if wp == T_VIS {
                        KillTimer(hwnd, T_VIS);
                    }
                    with_app(|a| a.check_visibility());
                }
                T_WEATHER => {
                    with_app(|a| a.refresh_weather());
                }
                T_HOLI => {
                    with_app(|a| a.refresh_holidays());
                }
                T_SYNC => {
                    with_app(|a| a.sync());
                }
                T_PRONOTE => {
                    with_app(|a| a.pn_sync());
                }
                T_ILE_CLOCK => {
                    with_app(|a| a.ile_tick());
                }
                T_ILE_ANIM => {
                    with_app(|a| a.ile_anim_step());
                }
                T_ILE_LEAVE => {
                    with_app(|a| a.ile_leave());
                }
                T_RING => {
                    with_app(|a| a.ile_ring_tick());
                }
                T_ILE_FX => {
                    with_app(|a| a.ile_fx());
                }
                T_ILE_OPEN => {
                    KillTimer(hwnd, T_ILE_OPEN);
                    with_app(|a| a.ile_open());
                }
                T_ILE_SLOW => {
                    with_app(|a| a.ile_slow());
                }
                T_ILE_WATCH => {
                    with_app(|a| a.ile_watch());
                }
                T_SAVE => {
                    KillTimer(hwnd, T_SAVE);
                    with_app(|a| config::save(&a.cfg));
                }
                T_RETRY => {
                    KillTimer(hwnd, T_RETRY);
                    with_app(|a| {
                        a.refresh_weather();
                        if a.data.holidays.is_empty() {
                            a.refresh_holidays();
                        }
                    });
                }
                _ => {}
            }
            return 0;
        }
        data::WM_APP_NET => {
            with_app(|a| a.on_net());
            return 0;
        }
        WM_APP_TRAY => {
            match (lp & 0xFFFF) as u32 {
                WM_LBUTTONUP => {
                    with_app(|a| a.open_settings());
                }
                // clic sur la notification : ouvre le mail (ou Pronote)
                NIN_BALLOONUSERCLICK => {
                    with_app(|a| {
                        if let Some((l, f)) = a.notif_link.take() {
                            a.open_mail(&l, &f);
                        }
                    });
                }
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(),
                _ => {}
            }
            return 0;
        }
        WM_APP_SHOW => {
            with_app(|a| a.open_settings());
            return 0;
        }
        WM_APP_QUIT => {
            DestroyWindow(hwnd);
            return 0;
        }
        WM_APP_MENU => {
            show_menu();
            return 0;
        }
        WM_APP_ILE => {
            with_app(|a| a.ile_set(wp != 0));
            return 0;
        }
        WM_SETTINGCHANGE => {
            let wall = wp as u32 == SPI_SETDESKWALLPAPER;
            with_app(|a| {
                if wall {
                    a.backdrop = None;
                    a.ensure_backdrop();
                }
                if a.shown {
                    a.render_all();
                }
                a.refresh_settings();
            });
            return 0;
        }
        WM_DISPLAYCHANGE => {
            with_app(|a| {
                a.dpi = sys::dpi();
                a.backdrop = None;
                a.ensure_backdrop();
                a.ensure_places();
                if a.shown {
                    a.render_all();
                }
                a.ile_render();
            });
            return 0;
        }
        WM_POWERBROADCAST => {
            if wp == 0x12 {
                with_app(|a| {
                    a.refresh_weather();
                    a.pn_sync();
                    a.last_day = None;
                    a.tick();
                });
            }
            return 1;
        }
        WM_DESTROY => {
            with_app(|a| {
                config::save(&a.cfg);
                tray(a.main, a.icon, NIM_DELETE, None);
                for s in &a.slots {
                    DestroyWindow(s.hwnd);
                }
                if let Some(st) = &a.ile {
                    DestroyWindow(st.hwnd);
                }
                if let Some(st) = &a.ile_set_win {
                    DestroyWindow(st.hwnd);
                }
                if let Some(st) = &a.settings {
                    DestroyWindow(st.hwnd);
                }
            });
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    let tc = with_app(|a| a.taskbar_created).unwrap_or(0);
    if tc != 0 && msg == tc {
        with_app(|a| tray(a.main, a.icon, NIM_ADD, None));
        return 0;
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe extern "system" fn widget_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_SETCURSOR => {
            if with_app(|a| a.dragging() || (a.edit && !a.cfg.locked)).unwrap_or(false) {
                SetCursor(LoadCursorW(null_mut(), IDC_SIZEALL));
                return 1;
            }
        }
        WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN => {
            if with_app(|a| a.press(hwnd)).unwrap_or(false) {
                SetCapture(hwnd);
            }
            return 0;
        }
        WM_MOUSEMOVE => {
            if wp & 1 != 0 {
                with_app(|a| a.drag_to());
                if with_app(|a| a.dragging()).unwrap_or(false) {
                    SetCursor(LoadCursorW(null_mut(), IDC_SIZEALL));
                }
            }
            return 0;
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            with_app(|a| a.release(hwnd, x, y));
            ReleaseCapture();
            return 0;
        }
        WM_CAPTURECHANGED => {
            with_app(|a| {
                if a.drag.take().is_some_and(|d| d.moved) {
                    a.save_soon();
                }
            });
            return 0;
        }
        WM_MOUSEWHEEL => {
            let d = ((wp >> 16) & 0xFFFF) as u16 as i16;
            let ctrl = wp & 0x8 != 0;
            with_app(|a| a.wheel(hwnd, d, ctrl));
            return 0;
        }
        WM_LBUTTONDBLCLK => {
            with_app(|a| a.open_settings());
            return 0;
        }
        WM_RBUTTONUP | WM_NCRBUTTONUP => {
            show_menu();
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe extern "system" fn ile_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
    match msg {
        WM_MOUSEACTIVATE => {
            let typing = with_app(|a| a.ile.as_ref().is_some_and(|s| s.typing)).unwrap_or(false);
            return if typing { MA_ACTIVATE } else { MA_NOACTIVATE } as LRESULT;
        }
        WM_MOUSEMOVE => {
            let start = with_app(|a| {
                let st = a.ile.as_mut()?;
                let first = !st.tracking;
                st.tracking = true;
                Some(first)
            })
            .flatten()
            .unwrap_or(false);
            if start {
                let mut tme = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                TrackMouseEvent(&mut tme);
                with_app(|a| a.ile_hover(true));
            }
            with_app(|a| a.ile_mouse_move());
            return 0;
        }
        0x02A3 => {
            // WM_MOUSELEAVE
            with_app(|a| {
                if let Some(st) = &mut a.ile {
                    st.tracking = false;
                }
                a.ile_hover(false);
            });
            return 0;
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            with_app(|a| a.ile_click(x, y, false));
            return 0;
        }
        WM_LBUTTONDBLCLK => {
            let (x, y) = xy(lp);
            with_app(|a| a.ile_click(x, y, true));
            return 0;
        }
        WM_MOUSEWHEEL => {
            let d = ((wp >> 16) & 0xFFFF) as u16 as i16;
            let (sx, sy) = xy(lp);
            with_app(|a| a.ile_wheel(d, sx, sy));
            return 0;
        }
        WM_CHAR => {
            with_app(|a| a.ile_char(wp as u32));
            return 0;
        }
        WM_KILLFOCUS => {
            with_app(|a| a.ile_end_typing());
            return 0;
        }
        0x031D => {
            // WM_CLIPBOARDUPDATE
            with_app(|a| a.on_clip());
            return 0;
        }
        WM_RBUTTONUP => {
            show_menu();
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe extern "system" fn ile_set_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (x, y) = xy(lp);
    let (x, y) = (x as f32, y as f32);
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            with_app(|a| {
                if let Some(pm) = a.ile_set_win.as_ref().and_then(|s| s.pm.as_ref()) {
                    let (w, h) = (pm.width() as i32, pm.height() as i32);
                    let mut buf = vec![0u8; pm.data().len()];
                    gfx::rgba_to_bgra(pm.data(), &mut buf);
                    let bmi = gfx::bmi32(w, h);
                    SetDIBitsToDevice(hdc, 0, 0, w as u32, h as u32, 0, 0, 0, h as u32, buf.as_ptr() as _, &bmi, DIB_RGB_COLORS);
                }
            });
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_LBUTTONDOWN => {
            with_app(|a| a.ile_settings_click(x, y, 1));
            return 0;
        }
        WM_RBUTTONDOWN => {
            with_app(|a| a.ile_settings_click(x, y, -1));
            return 0;
        }
        WM_MOUSEWHEEL => {
            let d = ((wp >> 16) & 0xFFFF) as u16 as i16;
            with_app(|a| a.ile_settings_wheel(d));
            return 0;
        }
        WM_KEYDOWN => {
            if wp as u32 == 0x1B {
                DestroyWindow(hwnd);
                return 0;
            }
        }
        WM_DESTROY => {
            with_app(|a| {
                a.ile_set_win = None;
                config::save(&a.cfg);
                sys::trim_memory();
            });
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

unsafe extern "system" fn settings_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (x, y) = xy(lp);
    let (x, y) = (x as f32, y as f32);
    match msg {
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            with_app(|a| {
                if let Some(st) = &a.settings {
                    settings::blit(st, hdc);
                }
            });
            EndPaint(hwnd, &ps);
            return 0;
        }
        WM_ERASEBKGND => return 1,
        WM_LBUTTONDOWN => {
            if with_app(|a| a.settings_down(x, y)).unwrap_or(false) {
                SetCapture(hwnd);
            }
            return 0;
        }
        WM_MOUSEMOVE => {
            if wp & 1 != 0 {
                with_app(|a| a.settings_move(x));
            }
            return 0;
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            with_app(|a| a.settings_up());
            return 0;
        }
        WM_MOUSEWHEEL => {
            let d = ((wp >> 16) & 0xFFFF) as u16 as i16;
            with_app(|a| a.settings_wheel(d));
            return 0;
        }
        WM_CHAR => {
            with_app(|a| a.settings_char(wp as u32));
            return 0;
        }
        WM_KEYDOWN => {
            with_app(|a| a.settings_key(wp as u32));
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            return 0;
        }
        WM_DESTROY => {
            with_app(|a| {
                if let Some(mut st) = a.settings.take() {
                    if let Some(f) = settings::commit(&mut st, &mut a.cfg) {
                        a.apply(f);
                    }
                }
                config::save(&a.cfg);
                sys::trim_memory();
            });
            return 0;
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Appelé par Windows quand une fenêtre s'ouvre, se ferme, se réduit… (aucune scrutation en boucle).
unsafe extern "system" fn win_event(_h: HWINEVENTHOOK, ev: u32, hwnd: HWND, obj: i32, child: i32, _t: u32, _ms: u32) {
    if obj != 0 || child != 0 || hwnd.is_null() {
        return;
    }
    if ev != 0x0003 && GetAncestor(hwnd, GA_ROOT) != hwnd {
        return;
    }
    // nos propres widgets (sinon les remettre en place relancerait l'événement en boucle)
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == GetCurrentProcessId() {
        return;
    }
    let main = MAIN.load(Ordering::Relaxed) as HWND;
    if !main.is_null() {
        SetTimer(main, T_VIS, 80, None);
    }
}

unsafe fn register(name: &str, proc_: WNDPROC, style: u32, icon: HICON) {
    let n = wide(name);
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style,
        lpfnWndProc: proc_,
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: GetModuleHandleW(null()),
        hIcon: icon,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: n.as_ptr(),
        hIconSm: icon,
    };
    RegisterClassExW(&wc);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--pronote-test" {
        pronote::selftest(&args[2]);
        return;
    }
    if args.len() >= 2 && args[1] == "--notif-test" {
        extra::notif_selftest();
        return;
    }
    if args.len() >= 3 && args[1] == "--apercu" {
        preview::run(&args[2]);
        return;
    }
    if args.len() >= 3 && args[1] == "--apercu-ile" {
        preview::run_ile(&args[2]);
        return;
    }
    // --ile : démarre en mode Dynamic Island ; --widgets : revient aux widgets du bureau
    let ile = args.iter().any(|a| a == "--ile").then_some(true).or(args.iter().any(|a| a == "--widgets").then_some(false));
    unsafe { run(ile) }
}

unsafe fn run(ile: Option<bool>) {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let test = config::test_profile().is_some();
    // une seule instance : « Givre-instance-unique » (nom connu de Jarvis) + l'ancien nom
    CreateMutexW(null(), 0, wide(if test { "Givre-instance-test" } else { "Givre-instance-unique" }).as_ptr());
    let mut other = GetLastError() == ERROR_ALREADY_EXISTS;
    if !test {
        CreateMutexW(null(), 0, wide("Local\\GivreWidgets").as_ptr());
        other |= GetLastError() == ERROR_ALREADY_EXISTS;
    }
    if other && !test {
        let h = FindWindowW(wide(MAIN_CLASS).as_ptr(), null());
        if !h.is_null() {
            match ile {
                Some(on) => PostMessageW(h, WM_APP_ILE, on as usize, 0),
                None => PostMessageW(h, WM_APP_SHOW, 0, 0),
            };
        }
        return;
    }
    let Some(fonts) = Fonts::load() else {
        MessageBoxW(null_mut(), wide("Polices Windows introuvables.").as_ptr(), wide("Givre").as_ptr(), MB_ICONERROR);
        return;
    };
    config::import_hidden();
    let first_run = !config::exists();
    let mut cfg = config::load();
    cfg.autostart = sys::autostart_enabled();
    let icon = gfx::make_icon(32);
    register(MAIN_CLASS, Some(main_proc), 0, icon);
    register(WIDGET_CLASS, Some(widget_proc), CS_DBLCLKS, icon);
    register(SETTINGS_CLASS, Some(settings_proc), 0, icon);
    register(ILE_CLASS, Some(ile_proc), CS_DBLCLKS, icon);
    register(ILE_SET_CLASS, Some(ile_set_proc), 0, icon);
    if let Some(on) = ile {
        cfg.ile.on = on;
    }
    let main = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        wide(MAIN_CLASS).as_ptr(),
        wide("Givre").as_ptr(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if main.is_null() {
        return;
    }
    MAIN.store(main as isize, Ordering::Relaxed);
    let vault = Arc::new(Mutex::new(accounts::load()));
    let (jobs, rx, out) = data::spawn(main as isize, vault.clone());
    let media_tx = extra::spawn_media(out.clone());
    let discord_tx = extra::spawn_discord(out.clone(), vault.clone(), cfg.discord.id.clone(), cfg.discord.secret.clone());
    let winnotif_on = Arc::new(std::sync::atomic::AtomicBool::new(cfg.discord.on && cfg.discord.win_notifs));
    let winnotif_tx = extra::spawn_winnotif(out.clone(), winnotif_on.clone());
    let printer_ip = Arc::new(Mutex::new(if cfg.printer.on { cfg.printer.ip.clone() } else { String::new() }));
    extra::spawn_printer(out.clone(), printer_ip.clone());
    let pn_tx = pronote::spawn(out.clone(), vault.clone());
    let api_port = Arc::new(AtomicU32::new(if cfg.ile.api_on { cfg.ile.api_port.round() as u32 } else { 0 }));
    api::spawn(out.clone(), api_port.clone());
    // Jarvis : surveillance de son API ; lancé avec Givre (pas avec un profil de test)
    jarvis_link::spawn(out.clone(), cfg.ile.jarvis_autostart && !test, cfg.ile.jarvis_path.clone());
    let app = App {
        main,
        cfg,
        fonts,
        backdrop: None,
        data: Data::default(),
        slots: Vec::new(),
        shown: false,
        edit: false,
        icon,
        month_off: 0,
        dpi: sys::dpi(),
        jobs,
        rx,
        settings: None,
        last_day: None,
        last_block: None,
        notif_link: None,
        status: String::new(),
        taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        drag: None,
        media_tx,
        discord_tx,
        winnotif_tx,
        winnotif_on,
        printer_ip,
        pn_tx,
        sel_day: None,
        ile: None,
        ile_fonts: None,
        ile_set_win: None,
        ile_last_act: None,
        ile_last_mode: island::Mode::Pill,
        fx_cur: None,
        clocks: island::Clocks::default(),
        chat: island::Chat::default(),
        jarvis: island::Jarvis::default(),
        notifs: Vec::new(),
        ring: None,
        discord_unread: None,
        pending: HashMap::new(),
        api_port,
        notif_next: 0,
        out: out.clone(),
        jarvis_cmds: 0,
        jarvis_day: None,
        clips: Vec::new(),
        clip_skip: None,
        mail_bodies: HashMap::new(),
        mail_waiters: Vec::new(),
        mail_ai: island::MailAi::default(),
        mail_ask_wait: None,
    };
    APP.with(|c| *c.borrow_mut() = Some(app));
    with_app(|a| {
        sys::set_menu_theme(a.theme().dark);
        a.ensure_backdrop();
        a.ensure_places();
        a.sync_slots();
        a.refresh_weather();
        a.refresh_holidays();
        a.refresh_accounts();
        a.sync();
        SetTimer(main, T_SYNC, 5 * 60_000, None);
        a.pn_sync();
        SetTimer(main, T_PRONOTE, 30 * 60_000, None);
        SetTimer(main, T_WEATHER, 30 * 60_000, None);
        SetTimer(main, T_HOLI, 12 * 3_600_000, None);
        SetTimer(main, T_SAFE, 20_000, None);
        tray(main, a.icon, NIM_ADD, None);
        if a.cfg.ile.on {
            a.ile_set(true);
        }
        a.ile_tick();
        a.check_visibility();
        if first_run {
            tray(
                main,
                a.icon,
                NIM_MODIFY,
                Some((
                    "Givre est lancé",
                    "Tes widgets restent sur le bureau, sous tes fenêtres. Clique sur l'icône près de l'horloge pour les réglages.",
                )),
            );
            config::save(&a.cfg);
        }
    });
    // Événements « fenêtre au premier plan / déplacée ou redimensionnée / réduite / affichée / cachée /
    // agrandie / changée de bureau virtuel » : aucune scrutation en boucle, juste un contrôle 80 ms après.
    for (lo, hi) in [(0x0003, 0x0003), (0x000B, 0x000B), (0x0016, 0x0017), (0x8001, 0x8003), (0x800B, 0x800B), (0x8017, 0x8018)] {
        SetWinEventHook(lo, hi, null_mut(), Some(win_event), 0, 0, 2);
    }
    sys::trim_memory();
    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}
