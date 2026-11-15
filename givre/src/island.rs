//! Dynamic Island : état (ouverture animée, minuteurs, conversation IA, notifications, Jarvis).
//! Le dessin est dans island_draw.rs, la fenêtre Windows dans main.rs.

use std::time::{Duration, Instant};

use chrono::{Datelike, NaiveDateTime, Timelike};
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

use crate::config::IleCfg;
use crate::widgets::Hit;

// ---------- Boutons (Action::Ile / Action::IleN) ----------

pub const A_ALARM: u8 = 1;
pub const A_H_DOWN: u8 = 2;
pub const A_H_UP: u8 = 3;
pub const A_M_DOWN: u8 = 4;
pub const A_M_UP: u8 = 5;
pub const A_STOP: u8 = 6;
pub const A_SNOOZE: u8 = 7;
pub const A_ASK: u8 = 8;
pub const A_SEND: u8 = 9;
pub const A_OLLAMA: u8 = 10;
pub const A_SETTINGS: u8 = 11;
pub const A_BACK: u8 = 12;
pub const A_CHRONO: u8 = 13;
pub const A_CHRONO_RESET: u8 = 14;
pub const A_NEWCHAT: u8 = 15;
pub const A_CLEAR_NOTIFS: u8 = 16;
pub const A_DISCORD_OPEN: u8 = 17;
pub const A_ALARM_DAYS: u8 = 18;
pub const A_JARVIS_LISTEN: u8 = 19;
pub const A_JARVIS_MIC: u8 = 20;
pub const A_MENU: u8 = 21;
pub const A_POMO: u8 = 22;
pub const A_POMO_RESET: u8 = 23;
pub const A_PREV: u8 = 24;
pub const A_PLAY: u8 = 25;
pub const A_NEXT: u8 = 26;
pub const A_VOLUME: u8 = 27;
pub const A_VOL_UP: u8 = 28;
pub const A_VOL_DOWN: u8 = 29;
pub const A_DND: u8 = 30;
pub const A_NOTIF_SETTINGS: u8 = 31;
pub const A_TABS_SETTINGS: u8 = 32;
/// IleN : vue (n = View), devoir choisi, devoir fait, mail choisi, minuteur +n s, pause/suppression minuteur n, note choisie.
pub const N_VIEW: u8 = 1;
pub const N_HW: u8 = 2;
pub const N_HW_DONE: u8 = 3;
pub const N_MAIL: u8 = 4;
pub const N_TIMER_ADD: u8 = 5;
pub const N_TIMER_PAUSE: u8 = 6;
pub const N_TIMER_DEL: u8 = 7;
pub const N_GRADE: u8 = 8;
pub const N_DISCORD: u8 = 9;
pub const N_TAB: u8 = 10;
pub const N_CLIP: u8 = 11;
pub const N_MAIL_ASK: u8 = 12;

/// Ce que l'île montre en ce moment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Pill,
    Toast,
    Jarvis,
    Ring,
    Main,
    View,
}

/// Pages de détail (clic sur un module).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Main = 0,
    Homework = 1,
    Grades = 2,
    Mails = 3,
    Discord = 4,
    Notifs = 5,
    Chat = 6,
    Music = 7,
    Timers = 8,
    Dnd = 9,
    School = 10,
    Calendar = 11,
    Clips = 12,
}

impl View {
    pub fn from(n: u16) -> View {
        match n {
            1 => View::Homework,
            2 => View::Grades,
            3 => View::Mails,
            4 => View::Discord,
            5 => View::Notifs,
            6 => View::Chat,
            7 => View::Music,
            8 => View::Timers,
            9 => View::Dnd,
            10 => View::School,
            11 => View::Calendar,
            12 => View::Clips,
            _ => View::Main,
        }
    }
}

/// Modules de l'île ouverte, avec leur largeur logique.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Module {
    Time,
    Timers,
    Weather,
    Pronote,
    Mail,
    Discord,
    Ai,
}

impl Module {
    pub fn width(self) -> f32 {
        match self {
            Module::Time => 200.0,
            Module::Timers => 205.0,
            Module::Weather => 215.0,
            Module::Pronote => 220.0,
            Module::Mail => 230.0,
            Module::Discord => 200.0,
            Module::Ai => 235.0,
        }
    }
}

/// Modules affichés, dans l'ordre choisi.
pub fn modules(c: &IleCfg) -> Vec<Module> {
    let mut v: Vec<(f32, usize, Module)> = [
        (c.m_time, c.o_time, Module::Time),
        (c.m_timers, c.o_timers, Module::Timers),
        (c.m_weather, c.o_weather, Module::Weather),
        (c.m_pronote, c.o_pronote, Module::Pronote),
        (c.m_mail, c.o_mail, Module::Mail),
        (c.m_discord, c.o_discord, Module::Discord),
        (c.m_ai, c.o_ai, Module::Ai),
    ]
    .into_iter()
    .enumerate()
    .filter(|(_, (on, _, _))| *on)
    .map(|(i, (_, o, m))| (o, i, m))
    .collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    let mut out: Vec<Module> = v.into_iter().map(|x| x.2).collect();
    if out.is_empty() {
        out.push(Module::Time);
    }
    out
}

/// Onglets de la barre du bas (disposition « Cartes »).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Jarvis,
    Music,
    Discord,
    Timers,
    Dnd,
    Notifs,
    Clips,
    School,
    Calendar,
    Mails,
}

impl Tab {
    pub const ALL: [Tab; 11] = [Tab::Home, Tab::Jarvis, Tab::Music, Tab::Discord, Tab::Timers, Tab::Dnd, Tab::Notifs, Tab::Clips, Tab::School, Tab::Calendar, Tab::Mails];
    pub fn view(self) -> View {
        match self {
            Tab::Home => View::Main,
            Tab::Jarvis => View::Chat,
            Tab::Music => View::Music,
            Tab::Discord => View::Discord,
            Tab::Timers => View::Timers,
            Tab::Dnd => View::Dnd,
            Tab::Notifs => View::Notifs,
            Tab::Clips => View::Clips,
            Tab::School => View::School,
            Tab::Calendar => View::Calendar,
            Tab::Mails => View::Mails,
        }
    }
    /// Onglet qui correspond à une page (les pages Devoirs / Notes vont avec « Pronote »).
    pub fn of(v: View) -> Tab {
        match v {
            View::Main => Tab::Home,
            View::Chat => Tab::Jarvis,
            View::Music => Tab::Music,
            View::Discord => Tab::Discord,
            View::Timers => Tab::Timers,
            View::Dnd => Tab::Dnd,
            View::Notifs => Tab::Notifs,
            View::Clips => Tab::Clips,
            View::School | View::Homework | View::Grades => Tab::School,
            View::Calendar => Tab::Calendar,
            View::Mails => Tab::Mails,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "ACCUEIL",
            Tab::Jarvis => "JARVIS",
            Tab::Music => "MUSIQUE",
            Tab::Discord => "DISCORD",
            Tab::Timers => "MINUTEURS",
            Tab::Dnd => "CALME",
            Tab::Notifs => "NOTIFS",
            Tab::Clips => "COPIES",
            Tab::School => "PRONOTE",
            Tab::Calendar => "AGENDA",
            Tab::Mails => "MAILS",
        }
    }
}

/// Onglets visibles, dans l'ordre choisi (Accueil toujours en premier).
pub fn tabs(c: &IleCfg) -> Vec<Tab> {
    let mut v: Vec<(f32, usize, Tab)> = [
        (c.t_jarvis, c.ot_jarvis, Tab::Jarvis),
        (c.t_music, c.ot_music, Tab::Music),
        (c.t_discord, c.ot_discord, Tab::Discord),
        (c.t_timers, c.ot_timers, Tab::Timers),
        (c.t_dnd, c.ot_dnd, Tab::Dnd),
        (c.t_notifs, c.ot_notifs, Tab::Notifs),
        (c.t_clips, c.ot_clips, Tab::Clips),
        (c.t_school, c.ot_school, Tab::School),
        (c.t_calendar, c.ot_calendar, Tab::Calendar),
        (c.t_mails, c.ot_mails, Tab::Mails),
    ]
    .into_iter()
    .enumerate()
    .filter(|(_, (on, _, _))| *on)
    .map(|(i, (_, o, t))| (o, i, t))
    .collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    std::iter::once(Tab::Home).chain(v.into_iter().map(|x| x.2)).collect()
}

/// Taille logique de l'île ouverte en disposition « Cartes ».
pub const CARDS_W: f32 = 680.0;
pub const CARDS_H: f32 = 40.0 + 236.0 + 58.0;

pub const GAP: f32 = 20.0;
pub const SIDE: f32 = 22.0;
pub const HEAD: f32 = 40.0;
pub const BODY: f32 = 236.0;

pub fn main_size(c: &IleCfg) -> (f32, f32) {
    let m = modules(c);
    let w = 2.0 * SIDE + m.iter().map(|x| x.width()).sum::<f32>() + GAP * (m.len().saturating_sub(1)) as f32;
    (w.max(420.0), HEAD + BODY)
}

// ---------- Notifications ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Src {
    Pronote,
    Mail,
    Discord,
    Timer,
    Alarm,
    Weather,
    Jarvis,
    Other,
}

impl Src {
    pub fn parse(s: &str) -> Src {
        match s.trim().to_lowercase().as_str() {
            "pronote" => Src::Pronote,
            "mail" | "mails" => Src::Mail,
            "discord" => Src::Discord,
            "minuteur" | "timer" => Src::Timer,
            "alarme" => Src::Alarm,
            "meteo" | "météo" => Src::Weather,
            "jarvis" | "" => Src::Jarvis,
            _ => Src::Other,
        }
    }
    /// Nom de la source pour l'API (/notifs).
    pub fn key(self) -> &'static str {
        match self {
            Src::Pronote => "pronote",
            Src::Mail => "mail",
            Src::Discord => "discord",
            Src::Timer => "minuteur",
            Src::Alarm => "alarme",
            Src::Weather => "meteo",
            Src::Jarvis | Src::Other => "jarvis",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Src::Pronote => "Pronote",
            Src::Mail => "Mail",
            Src::Discord => "Discord",
            Src::Timer => "Minuteur",
            Src::Alarm => "Alarme",
            Src::Weather => "Météo",
            Src::Jarvis => "Jarvis",
            Src::Other => "Info",
        }
    }
    /// Affichée dans l'île ? Toast Windows ?
    pub fn wanted(self, c: &IleCfg) -> (bool, bool) {
        let (i, t) = match self {
            Src::Pronote => (c.ni_pronote, c.nt_pronote),
            Src::Mail => (c.ni_mail, c.nt_mail),
            Src::Discord => (c.ni_discord, c.nt_discord),
            Src::Timer | Src::Alarm => (c.ni_timer, c.nt_timer),
            Src::Weather => (c.ni_jarvis, c.nt_jarvis),
            Src::Jarvis | Src::Other => (c.ni_jarvis, c.nt_jarvis),
        };
        (c.n_island && i && !c.dnd, c.n_toast && t && !c.dnd)
    }
}

#[derive(Clone)]
pub struct INotif {
    pub id: u64,
    pub src: Src,
    pub title: String,
    pub body: String,
    pub time: NaiveDateTime,
}

// ---------- Minuteurs, chronomètre, alarme ----------

pub struct Timer {
    pub id: u64,
    pub name: String,
    pub total: i64,
    /// En marche : instant de fin. En pause : secondes restantes.
    pub end: Option<Instant>,
    pub left: i64,
}

impl Timer {
    pub fn remaining(&self) -> f64 {
        match self.end {
            Some(e) => e.saturating_duration_since(Instant::now()).as_secs_f64(),
            None => self.left as f64,
        }
    }
}

#[derive(Default)]
pub struct Clocks {
    pub timers: Vec<Timer>,
    pub chrono_since: Option<Instant>,
    pub chrono_acc: Duration,
    pub snooze: Option<NaiveDateTime>,
    pub last_fire: Option<NaiveDateTime>,
    pub next_id: u64,
    /// Pomodoro : 0 prêt, 1 concentration, 2 pause ; fin (en marche) ou secondes restantes (en pause).
    pub pomo_phase: u8,
    pub pomo_end: Option<Instant>,
    pub pomo_left: f64,
}

impl Clocks {
    /// Ajoute un minuteur et renvoie son numéro.
    pub fn add_timer(&mut self, secs: i64, name: &str) -> u64 {
        if self.timers.len() >= 5 {
            self.timers.remove(0);
        }
        let name = if name.trim().is_empty() { fmt_dur(secs as f64) } else { name.trim().to_string() };
        self.next_id += 1;
        let id = self.next_id;
        self.timers.push(Timer { id, name, total: secs.max(1), end: Some(Instant::now() + Duration::from_secs(secs.max(1) as u64)), left: secs });
        id
    }
    pub fn toggle_timer(&mut self, i: usize) {
        if let Some(t) = self.timers.get_mut(i) {
            match t.end {
                Some(_) => {
                    t.left = t.remaining().ceil() as i64;
                    t.end = None;
                }
                None => t.end = Some(Instant::now() + Duration::from_secs(t.left.max(0) as u64)),
            }
        }
    }
    pub fn chrono(&self) -> Duration {
        self.chrono_acc + self.chrono_since.map(|s| s.elapsed()).unwrap_or_default()
    }
    pub fn chrono_cmd(&mut self, cmd: &str) {
        match cmd {
            "pause" | "stop" | "arreter" | "arrêter" => {
                if let Some(s) = self.chrono_since.take() {
                    self.chrono_acc += s.elapsed();
                }
            }
            "zero" | "zéro" | "reset" | "remettre" => {
                self.chrono_since = None;
                self.chrono_acc = Duration::ZERO;
            }
            _ => {
                if self.chrono_since.is_none() {
                    self.chrono_since = Some(Instant::now());
                }
            }
        }
    }
    pub fn chrono_toggle(&mut self) {
        if self.chrono_since.is_some() { self.chrono_cmd("pause") } else { self.chrono_cmd("demarrer") }
    }
    /// Minuteurs qui viennent de finir (retirés de la liste).
    pub fn finished(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        self.timers.retain(|t| {
            let done = t.end.is_some() && t.remaining() <= 0.0;
            if done {
                out.push(t.name.clone());
            }
            !done
        });
        out
    }
    /// L'alarme doit-elle sonner maintenant ?
    pub fn alarm_due(&mut self, c: &IleCfg, now: NaiveDateTime) -> bool {
        let minute = now.with_second(0).and_then(|t| t.with_nanosecond(0)).unwrap_or(now);
        if self.last_fire == Some(minute) {
            return false;
        }
        let wd = now.weekday().num_days_from_monday();
        let day_ok = match c.alarm_days {
            1 => wd < 5,
            2 => wd >= 5,
            _ => true,
        };
        let snoozed = self.snooze.is_some_and(|s| now >= s);
        let at_time = c.alarm_on && day_ok && now.hour() == c.alarm_h && now.minute() == c.alarm_m;
        if snoozed || at_time {
            self.snooze = None;
            self.last_fire = Some(minute);
            return true;
        }
        false
    }
    pub fn busy(&self) -> bool {
        self.chrono_since.is_some() || self.timers.iter().any(|t| t.end.is_some()) || self.pomo_end.is_some()
    }

    /// Bouton ▶/‖ du Pomodoro (durées en minutes).
    pub fn pomo_toggle(&mut self, work: f32) {
        match (self.pomo_phase, self.pomo_end) {
            (0, _) => {
                self.pomo_phase = 1;
                self.pomo_end = Some(Instant::now() + Duration::from_secs_f64(work.max(0.1) as f64 * 60.0));
            }
            (_, Some(_)) => {
                self.pomo_left = self.pomo_remaining(work);
                self.pomo_end = None;
            }
            (_, None) => self.pomo_end = Some(Instant::now() + Duration::from_secs_f64(self.pomo_left.max(0.0))),
        }
    }
    pub fn pomo_reset(&mut self) {
        self.pomo_phase = 0;
        self.pomo_end = None;
    }
    pub fn pomo_remaining(&self, work: f32) -> f64 {
        match (self.pomo_phase, self.pomo_end) {
            (0, _) => work as f64 * 60.0,
            (_, Some(e)) => e.saturating_duration_since(Instant::now()).as_secs_f64(),
            _ => self.pomo_left,
        }
    }
    /// Fin d'une phase : renvoie le message à sonner (et enchaîne concentration → pause → prêt).
    pub fn pomo_check(&mut self, brk: f32) -> Option<&'static str> {
        let e = self.pomo_end?;
        if Instant::now() < e {
            return None;
        }
        if self.pomo_phase == 1 {
            self.pomo_phase = 2;
            self.pomo_end = Some(Instant::now() + Duration::from_secs_f64(brk.max(0.1) as f64 * 60.0));
            Some("Focus terminé : pause !")
        } else {
            self.pomo_reset();
            Some("Pause terminée : au travail !")
        }
    }
}

/// « 4:05 », « 1:02:03 ».
pub fn fmt_dur(secs: f64) -> String {
    let s = secs.max(0.0).ceil() as i64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

pub struct Ring {
    pub label: String,
    pub since: Instant,
}

// ---------- IA ----------

#[derive(Default)]
pub struct Chat {
    /// (question, réponse) : tout ce qu'on affiche ; les derniers servent de mémoire à l'IA.
    pub log: Vec<(String, String)>,
    pub asking: bool,
    pub next_id: u64,
    pub error: String,
}

/// Question posée sur un mail (page détail d'un mail).
#[derive(Default)]
pub struct MailAi {
    pub n: Option<usize>,
    pub question: String,
    pub answer: String,
    pub asking: bool,
}

// ---------- Jarvis ----------

pub struct Jarvis {
    pub state: String,
    pub text: String,
    /// Dernier changement d'état reçu (POST /jarvis).
    pub at: Option<Instant>,
    /// Dernière nouvelle de Jarvis (réponse à GET :47801/etat ou POST /jarvis).
    pub seen: Option<Instant>,
    pub micro: bool,
}

impl Default for Jarvis {
    fn default() -> Self {
        Jarvis { state: "repos".into(), text: String::new(), at: None, seen: None, micro: true }
    }
}

impl Jarvis {
    /// Jarvis fait quelque chose (écoute, réfléchit, parle) depuis moins de 2 minutes, et il est en ligne.
    pub fn active(&self) -> bool {
        self.online() && self.state != "repos" && self.at.is_some_and(|t| t.elapsed().as_secs() < 120)
    }
    /// Nouvelle de Jarvis depuis moins de 10 s.
    pub fn online(&self) -> bool {
        self.seen.is_some_and(|t| t.elapsed().as_secs() <= 10)
    }
}

// ---------- L'île elle-même ----------

pub struct Island {
    pub hwnd: HWND,
    /// Taille logique actuelle (animée vers la cible) et vitesses.
    pub gw: f32,
    pub gh: f32,
    pub vw: f32,
    pub vh: f32,
    pub want: bool,
    pub tracking: bool,
    pub hits: Vec<Hit>,
    pub view: View,
    pub sel: Option<usize>,
    /// Défilement dans les pages de détail (en éléments).
    pub scroll: usize,
    pub typing: bool,
    pub input: String,
    pub hidden: bool,
    pub toast: Option<(INotif, Instant)>,
    pub t0: Instant,
    pub confetti: Option<Instant>,
    /// Position de la fenêtre à l'écran (pour les yeux qui suivent la souris).
    pub win: (i32, i32),
    pub unseen: usize,
    /// Cachée toute seule (pas de survol depuis N s) : 0 visible … 1 partie vers le haut.
    pub slide: f32,
    pub gone: bool,
    /// Dernier survol (ou événement qui l'a montrée).
    pub seen: Instant,
    /// Zone horizontale de l'île à l'écran, pour la faire revenir par le haut.
    pub zone: (i32, i32),
    /// Depuis quand la souris est collée en haut, dans la zone.
    pub edge_since: Option<Instant>,
}

impl Island {
    pub fn new(hwnd: HWND) -> Island {
        Island {
            hwnd,
            gw: 0.0,
            gh: 0.0,
            vw: 0.0,
            vh: 0.0,
            want: false,
            tracking: false,
            hits: Vec::new(),
            view: View::Main,
            sel: None,
            scroll: 0,
            typing: false,
            input: String::new(),
            hidden: false,
            toast: None,
            t0: Instant::now(),
            confetti: None,
            win: (0, 0),
            unseen: 0,
            slide: 0.0,
            gone: false,
            seen: Instant::now(),
            zone: (0, 0),
            edge_since: None,
        }
    }

    pub fn mode(&self, ring: bool, jarvis: bool, c: &IleCfg) -> Mode {
        if ring {
            Mode::Ring
        } else if self.want || self.typing || c.always_open {
            if self.view == View::Main { Mode::Main } else { Mode::View }
        } else if self.toast.is_some() {
            Mode::Toast
        } else if jarvis && c.jarvis_show {
            Mode::Jarvis
        } else {
            Mode::Pill
        }
    }

    /// Avance l'animation d'un pas vers la taille cible (ressort). `true` tant que ça bouge.
    pub fn step(&mut self, tw: f32, th: f32, c: &IleCfg) -> bool {
        if self.gw <= 0.0 {
            (self.gw, self.gh) = (tw, th);
            return false;
        }
        let k = (0.16 * c.anim_speed).clamp(0.03, 0.6);
        let damp = if c.bounce { 0.62 } else { 0.42 };
        self.vw = (self.vw + (tw - self.gw) * k) * damp;
        self.vh = (self.vh + (th - self.gh) * k) * damp;
        self.gw += self.vw;
        self.gh += self.vh;
        let done = (tw - self.gw).abs() < 0.4 && (th - self.gh).abs() < 0.4 && self.vw.abs() < 0.4 && self.vh.abs() < 0.4;
        if done {
            (self.gw, self.gh, self.vw, self.vh) = (tw, th, 0.0, 0.0);
        }
        !done
    }

    /// Avance la sortie / le retour vers le haut. `true` tant que ça bouge.
    pub fn step_slide(&mut self, c: &IleCfg) -> bool {
        let goal = if self.gone { 1.0 } else { 0.0 };
        if !c.hide_anim {
            self.slide = goal;
            return false;
        }
        let d = goal - self.slide;
        if d.abs() < 0.001 {
            return false;
        }
        let k = (0.11 * c.anim_speed).clamp(0.03, 0.5);
        self.slide = if d > 0.0 { (self.slide + k).min(1.0) } else { (self.slide - k).max(0.0) };
        (goal - self.slide).abs() > 0.001
    }

    pub fn secs(&self) -> f32 {
        self.t0.elapsed().as_secs_f32()
    }
}

// ---------- Plein écran et Discord dans Chrome ----------

/// Une appli (jeu, vidéo) occupe tout l'écran principal, barre des tâches comprise ?
pub fn fullscreen_app() -> bool {
    unsafe {
        let h = GetForegroundWindow();
        if h.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        if pid == GetCurrentProcessId() {
            return false;
        }
        let class = class_of(h);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Windows.UI.Core.CoreWindow") {
            return false;
        }
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(h, &mut r) == 0 {
            return false;
        }
        let (sw, sh) = crate::sys::screen_size();
        r.left <= 0 && r.top <= 0 && r.right >= sw && r.bottom >= sh
    }
}

unsafe fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}

unsafe extern "system" fn enum_titles(h: HWND, lp: LPARAM) -> i32 {
    let v = &mut *(lp as *mut Vec<String>);
    if IsWindowVisible(h) != 0 && class_of(h) == "Chrome_WidgetWin_1" {
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
        if n > 0 {
            v.push(String::from_utf16_lossy(&buf[..n]));
        }
    }
    1
}

/// Non-lus Discord lus dans le titre d'une fenêtre Chrome / Edge « (3) Discord | … ».
/// Ne marche que si l'onglet Discord est l'onglet affiché de sa fenêtre. None = pas d'onglet Discord visible.
pub fn chrome_discord_unread() -> Option<u32> {
    let mut v: Vec<String> = Vec::new();
    unsafe { EnumWindows(Some(enum_titles), &mut v as *mut _ as LPARAM) };
    v.iter().filter(|t| t.contains("Discord")).map(|t| discord_count(t)).max()
}

pub fn discord_count(title: &str) -> u32 {
    let t = title.trim_start();
    if let Some(rest) = t.strip_prefix('(') {
        if let Some((n, _)) = rest.split_once(')') {
            return n.trim().trim_end_matches('+').parse().unwrap_or(0);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sortie_vers_le_haut() {
        let c = IleCfg::default();
        let mut s = Island::new(std::ptr::null_mut());
        s.gone = true;
        let mut n = 0;
        while s.step_slide(&c) {
            n += 1;
        }
        assert_eq!(s.slide, 1.0);
        assert!(n > 3 && n < 40);
        s.gone = false;
        while s.step_slide(&c) {}
        assert_eq!(s.slide, 0.0);
    }

    #[test]
    fn nettoyage_discord() {
        let t = "\u{2068}Mormadable\u{2069} (\u{2068}#announcements\u{2069}, Serveur)";
        assert_eq!(crate::sys::clean(t), "Mormadable (#announcements, Serveur)");
        assert!(crate::sys::discord_is_server(t));
        assert!(!crate::sys::discord_is_server("\u{2068}Lucas\u{2069}"));
    }

    #[test]
    fn titre_discord() {
        assert_eq!(discord_count("(3) Discord | Amis - Google Chrome"), 3);
        assert_eq!(discord_count("Discord | #général - Google Chrome"), 0);
    }

    #[test]
    fn ordre_des_modules() {
        let mut c = IleCfg::default();
        c.o_ai = 0.0;
        c.m_discord = false;
        let m = modules(&c);
        assert_eq!(m[0], Module::Ai);
        assert!(!m.contains(&Module::Discord));
    }

    #[test]
    fn minuteurs() {
        let mut k = Clocks::default();
        k.add_timer(60, "");
        assert_eq!(k.timers[0].name, "1:00");
        k.toggle_timer(0);
        assert!(k.timers[0].end.is_none());
        assert!(k.finished().is_empty());
        assert_eq!(fmt_dur(3725.0), "1:02:05");
    }
}
