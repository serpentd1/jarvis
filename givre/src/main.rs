//! Givre : widgets de bureau en verre flou (heure, météo, calendrier, mails, Pronote…).
//! Win32 natif + dessin logiciel (tiny-skia) : pas de navigateur, très peu de RAM et de CPU.

#![windows_subsystem = "windows"]
#![allow(unsafe_op_in_unsafe_fn, clippy::missing_safety_doc)]

mod accounts;
mod backdrop;
mod config;
mod data;
mod extra;
mod gfx;
mod preview;
mod pronote;
mod settings;
mod sys;
mod widgets;

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
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

const WM_APP_TRAY: u32 = WM_APP + 1;
const WM_APP_SHOW: u32 = WM_APP + 3;
const WM_APP_QUIT: u32 = WM_APP + 4;

const T_CLOCK: usize = 1;
const T_VIS: usize = 2;
const T_WEATHER: usize = 3;
const T_HOLI: usize = 4;
const T_SAVE: usize = 5;
const T_RETRY: usize = 6;
const T_SAFE: usize = 7;
const T_SYNC: usize = 8;
const T_PRONOTE: usize = 9;

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
        if !(self.cfg.blur || self.cfg.accent_auto) {
            self.backdrop = None;
            return;
        }
        match &mut self.backdrop {
            Some(b) => b.set_blur(self.cfg.blur_amount),
            None => {
                let (sw, sh) = sys::screen_size();
                self.backdrop = Some(Backdrop::load(sw, sh, self.cfg.blur_amount));
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
                        self.notify(&t, &b, Some((l, f)));
                    }
                }
                Msg::Media(m) => {
                    self.data.media = m;
                    dirty.push(Kind::Music);
                }
                Msg::Discord(mut n) => {
                    n.title = sys::clean(&n.title);
                    n.body = sys::clean(&n.body);
                    // même message reçu par la connexion Discord et par les notifications Windows
                    if self.data.discord.iter().any(|o| o.title == n.title && o.body == n.body) {
                        continue;
                    }
                    let fresh = Local::now().naive_local() - n.time < chrono::Duration::minutes(2);
                    if self.cfg.discord.notify && fresh && self.cfg.discord.important(&n.title, &n.body) {
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
                    self.data.pronote = s;
                    dirty.extend([Kind::NextClass, Kind::Homework, Kind::Grades, Kind::School, Kind::Calendar]);
                }
                Msg::PNotify(k, t, b) => {
                    let p = &self.cfg.pronote;
                    if [p.n_grade, p.n_hw, p.n_cancel].get(k as usize).copied().unwrap_or(false) {
                        let base = &self.data.pronote.base;
                        let link = (!base.is_empty()).then(|| (format!("{base}/eleve.html"), String::new()));
                        self.notify(&t, &b, link);
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
            None => return,
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
    /// Notification dans la zone de notification de Windows (clic = ouvrir le lien).
    fn notify(&mut self, title: &str, body: &str, link: Option<(String, String)>) {
        self.notif_link = link;
        unsafe { tray(self.main, self.icon, NIM_MODIFY, Some((title, body))) };
    }
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
    let Some((main, edit, auto, locked)) = with_app(|a| (a.main, a.edit, a.cfg.autostart, a.cfg.locked)) else {
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
    unsafe { run() }
}

unsafe fn run() {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let test = config::test_profile().is_some();
    // Givre (les widgets du bureau) et Jarvis (l'assistant et son île) sont deux applis séparées :
    // chacune a son icône, et Givre se lance que Jarvis tourne ou non.
    // une seule instance : « Givre-instance-unique » + l'ancien nom
    CreateMutexW(null(), 0, wide(if test { "Givre-instance-test" } else { "Givre-instance-unique" }).as_ptr());
    let mut other = GetLastError() == ERROR_ALREADY_EXISTS;
    if !test {
        CreateMutexW(null(), 0, wide("Local\\GivreWidgets").as_ptr());
        other |= GetLastError() == ERROR_ALREADY_EXISTS;
    }
    if other && !test {
        let h = FindWindowW(wide(MAIN_CLASS).as_ptr(), null());
        if !h.is_null() {
            PostMessageW(h, WM_APP_SHOW, 0, 0);
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
