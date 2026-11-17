//! Fenêtre des paramètres, dessinée comme les widgets (style Réglages de Mac / iPhone).

use tiny_skia::Pixmap;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{DIB_RGB_COLORS, HDC, SetDIBitsToDevice};

use crate::config::Config;
use crate::gfx::{self, Fonts, Rgba, Theme, WHITE, Wt};
use crate::widgets::Ctx;

pub const WIN_W: f32 = 860.0;
pub const WIN_H: f32 = 600.0;
const SIDE: f32 = 224.0;
const TOP: f32 = 68.0;
const PAGES: [&str; 6] = ["Apparence", "Widgets", "Agenda", "Comptes", "IA", "Général"];
pub const ACCENTS: [[u8; 3]; 8] = [
    [52, 199, 89],
    [0, 199, 190],
    [10, 132, 255],
    [94, 92, 230],
    [191, 90, 242],
    [255, 55, 95],
    [255, 159, 10],
    [142, 142, 147],
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum F {
    Theme,
    Weight,
    Scale,
    Blur,
    BlurAmt,
    Opacity,
    Border,
    Shadow,
    Radius,
    Cont,
    Accent,
    AccentAuto,
    ClockOn,
    Secs,
    H24,
    Date,
    Week,
    WeatherOn,
    City,
    Hourly,
    Daily,
    CalOn,
    Agenda,
    CalHol,
    CountOn,
    Acad,
    ExamName,
    ExamDate,
    HideWin,
    Autostart,
    Grid,
    Locked,
    Edit,
    Reset,
    Folder,
    Quit,
    MailOn,
    MusicOn,
    DiscordOn,
    PrinterOn,
    AiOn,
    NotifyMail,
    NotifyDiscord,
    DcDmOnly,
    DcMentions,
    DcWin,
    DcKeywords,
    DcPeople,
    MsClient,
    DcId,
    DcSecret,
    PrinterIp,
    EvTitle,
    EvDate,
    EvTime,
    EvDur,
    EvCreate,
    GAdd,
    GDel(u8),
    MsAdd,
    MsDel,
    DcAdd,
    PnNext,
    PnHw,
    PnGrades,
    PnInfo,
    PnNGrade,
    PnNHw,
    PnNCancel,
    PnGoogle,
    PnPin,
    PnPaste,
    PnLogin,
    PnSync,
    PnDel,
    /// Services d'IA : (position dans la liste) interrupteur, « En premier », « Retirer ».
    IaOn(u8),
    IaTop(u8),
    IaDel(u8),
    IaCle,
    IaUrl,
    IaModele,
    IaChercher,
    IaLmStudio,
    IaGemini,
    IaGroq,
}

#[derive(Clone, Copy)]
enum K {
    Head,
    Toggle,
    Slider(f32, f32, &'static str, f32),
    Seg(&'static [&'static str]),
    Swatch,
    Text(&'static str),
    Button(&'static str),
    /// Un service d'IA : interrupteur, « En premier », « Retirer ».
    Ia,
    Note,
}

struct Row {
    k: K,
    f: Option<F>,
    label: String,
    y: f32,
    h: f32,
    lines: Vec<String>,
}

/// État de Pronote affiché dans Paramètres › Comptes.
#[derive(Clone, Default)]
pub struct PnInfo {
    pub connected: bool,
    pub who: String,
    pub state: String,
    pub qr: bool,
}

pub struct Info {
    pub pn: PnInfo,
    pub edit: bool,
    pub mem: String,
    pub status: String,
    pub acc: crate::data::AccountsInfo,
    pub discord: String,
    pub winnotif: String,
    pub ia: crate::ia::Vue,
}

pub struct Settings {
    pub hwnd: HWND,
    /// Dernier texte validé (Entrée) dans une case : sert à la clé d'API.
    pub texte: String,
    page: usize,
    scroll: f32,
    rows: Vec<Row>,
    bottom: f32,
    focus: Option<F>,
    edit: Vec<char>,
    caret: usize,
    pub drag: Option<F>,
    buf: Vec<u8>,
    size: (i32, i32),
}

impl Settings {
    pub fn new(hwnd: HWND) -> Self {
        Settings {
            hwnd,
            page: 0,
            scroll: 0.0,
            rows: Vec::new(),
            bottom: 0.0,
            focus: None,
            edit: Vec::new(),
            caret: 0,
            drag: None,
            texte: String::new(),
            buf: Vec::new(),
            size: (0, 0),
        }
    }
    pub fn page(&self) -> usize {
        self.page
    }
    pub fn has_focus(&self) -> bool {
        self.focus.is_some()
    }
    pub fn set_page(&mut self, p: usize) {
        self.page = p.min(PAGES.len() - 1);
        self.scroll = 0.0;
    }
}

// ---------- Lecture / écriture des réglages ----------

fn get_b(c: &Config, f: F) -> bool {
    match f {
        F::Blur => c.blur,
        F::Border => c.border,
        F::Cont => c.continuous,
        F::AccentAuto => c.accent_auto,
        F::ClockOn => c.clock.on,
        F::Secs => c.clock.seconds,
        F::H24 => c.clock.h24,
        F::Date => c.clock.date,
        F::Week => c.clock.week,
        F::WeatherOn => c.weather.on,
        F::Hourly => c.weather.hourly,
        F::Daily => c.weather.daily,
        F::CalOn => c.calendar.on,
        F::Agenda => c.calendar.agenda,
        F::CalHol => c.calendar.holidays,
        F::CountOn => c.countdown.on,
        F::HideWin => c.hide_when_windows,
        F::Autostart => c.autostart,
        F::Locked => c.locked,
        F::MailOn => c.mail.on,
        F::MusicOn => c.music,
        F::DiscordOn => c.discord.on,
        F::PrinterOn => c.printer.on,
        F::AiOn => c.mail.ai,
        F::NotifyMail => c.mail.notify,
        F::NotifyDiscord => c.discord.notify,
        F::DcDmOnly => c.discord.dm_only,
        F::DcMentions => c.discord.mentions,
        F::DcWin => c.discord.win_notifs,
        F::PnNext => c.pronote.next,
        F::PnHw => c.pronote.homework,
        F::PnGrades => c.pronote.grades,
        F::PnInfo => c.pronote.info,
        F::PnNGrade => c.pronote.n_grade,
        F::PnNHw => c.pronote.n_hw,
        F::PnNCancel => c.pronote.n_cancel,
        F::PnGoogle => c.pronote.google,
        _ => false,
    }
}

fn set_b(c: &mut Config, f: F, v: bool) {
    let r = match f {
        F::Blur => &mut c.blur,
        F::Border => &mut c.border,
        F::Cont => &mut c.continuous,
        F::AccentAuto => &mut c.accent_auto,
        F::ClockOn => &mut c.clock.on,
        F::Secs => &mut c.clock.seconds,
        F::H24 => &mut c.clock.h24,
        F::Date => &mut c.clock.date,
        F::Week => &mut c.clock.week,
        F::WeatherOn => &mut c.weather.on,
        F::Hourly => &mut c.weather.hourly,
        F::Daily => &mut c.weather.daily,
        F::CalOn => &mut c.calendar.on,
        F::Agenda => &mut c.calendar.agenda,
        F::CalHol => &mut c.calendar.holidays,
        F::CountOn => &mut c.countdown.on,
        F::HideWin => &mut c.hide_when_windows,
        F::Autostart => &mut c.autostart,
        F::Locked => &mut c.locked,
        F::MailOn => &mut c.mail.on,
        F::MusicOn => &mut c.music,
        F::DiscordOn => &mut c.discord.on,
        F::PrinterOn => &mut c.printer.on,
        F::AiOn => &mut c.mail.ai,
        F::NotifyMail => &mut c.mail.notify,
        F::NotifyDiscord => &mut c.discord.notify,
        F::DcDmOnly => &mut c.discord.dm_only,
        F::DcMentions => &mut c.discord.mentions,
        F::DcWin => &mut c.discord.win_notifs,
        F::PnNext => &mut c.pronote.next,
        F::PnHw => &mut c.pronote.homework,
        F::PnGrades => &mut c.pronote.grades,
        F::PnInfo => &mut c.pronote.info,
        F::PnNGrade => &mut c.pronote.n_grade,
        F::PnNHw => &mut c.pronote.n_hw,
        F::PnNCancel => &mut c.pronote.n_cancel,
        F::PnGoogle => &mut c.pronote.google,
        _ => return,
    };
    *r = v;
}

fn get_f(c: &Config, f: F) -> f32 {
    match f {
        F::Scale => c.scale,
        F::BlurAmt => c.blur_amount,
        F::Opacity => c.opacity,
        F::Shadow => c.shadow,
        F::Radius => c.radius,
        F::Grid => c.grid,
        _ => 0.0,
    }
}

fn set_f(c: &mut Config, f: F, v: f32) {
    match f {
        F::Scale => c.scale = (v * 20.0).round() / 20.0,
        F::BlurAmt => c.blur_amount = (v * 100.0).round() / 100.0,
        F::Opacity => c.opacity = (v * 100.0).round() / 100.0,
        F::Shadow => c.shadow = (v * 100.0).round() / 100.0,
        F::Radius => c.radius = v.round(),
        F::Grid => c.grid = v.round(),
        _ => {}
    }
}

fn get_i(c: &Config, f: F) -> usize {
    match f {
        F::Theme => c.theme,
        F::Weight => c.clock_weight,
        _ => 0,
    }
}

fn set_i(c: &mut Config, f: F, v: usize) {
    match f {
        F::Theme => c.theme = v,
        F::Weight => c.clock_weight = v,
        _ => {}
    }
}

fn get_t(c: &Config, f: F) -> String {
    match f {
        F::City => c.weather.city.clone(),
        F::Acad => c.countdown.academie.clone(),
        F::ExamName => c.countdown.exam_name.clone(),
        F::ExamDate => c.countdown.exam_date.clone(),
        F::IaUrl => crate::ia::adresse_saisie(),
        F::IaModele => crate::ia::modele_premier(),
        F::MsClient => c.mail.ms_client.clone(),
        F::DcId => c.discord.id.clone(),
        F::DcSecret => c.discord.secret.clone(),
        F::DcKeywords => c.discord.keywords.clone(),
        F::DcPeople => c.discord.people.clone(),
        F::PrinterIp => c.printer.ip.clone(),
        F::EvTitle => c.draft.title.clone(),
        F::EvDate => c.draft.date.clone(),
        F::EvTime => c.draft.time.clone(),
        F::EvDur => c.draft.dur.clone(),
        F::PnPin => c.pn_pin.clone(),
        _ => String::new(),
    }
}

fn set_t(c: &mut Config, f: F, v: String) {
    match f {
        F::City => c.weather.city = v,
        F::Acad => c.countdown.academie = v,
        F::ExamName => c.countdown.exam_name = v,
        F::ExamDate => c.countdown.exam_date = v,
        F::IaUrl => crate::ia::regler_adresse(&v),
        F::IaModele => crate::ia::regler_modele_premier(&v),
        F::MsClient => c.mail.ms_client = v,
        F::DcId => c.discord.id = v,
        F::DcSecret => c.discord.secret = v,
        F::DcKeywords => c.discord.keywords = v,
        F::DcPeople => c.discord.people = v,
        F::PrinterIp => c.printer.ip = v,
        F::EvTitle => c.draft.title = v,
        F::EvDate => c.draft.date = v,
        F::EvTime => c.draft.time = v,
        F::EvDur => c.draft.dur = v,
        F::PnPin => c.pn_pin = v,
        _ => {}
    }
}

// ---------- Contenu des pages ----------

struct B(Vec<(K, Option<F>, String)>);

impl B {
    fn head(&mut self, t: &str) {
        self.0.push((K::Head, None, t.into()));
    }
    fn row(&mut self, k: K, f: F, t: &str) {
        self.0.push((k, Some(f), t.into()));
    }
    fn note(&mut self, t: &str) {
        self.0.push((K::Note, None, t.into()));
    }
}

const PCT: K = K::Slider(0.0, 1.0, " %", 100.0);

fn spec(page: usize, info: &Info) -> Vec<(K, Option<F>, String)> {
    let mut b = B(Vec::new());
    match page {
        0 => {
            b.head("Style");
            b.row(K::Seg(&["Sombre", "Clair", "Auto"]), F::Theme, "Thème");
            b.row(K::Seg(&["Fins", "Normaux", "Gras"]), F::Weight, "Chiffres de l'heure");
            b.row(K::Slider(0.7, 1.6, " %", 100.0), F::Scale, "Taille des widgets");
            b.head("Verre");
            b.row(K::Toggle, F::Blur, "Verre flouté");
            b.row(PCT, F::BlurAmt, "Intensité du flou");
            b.row(PCT, F::Opacity, "Teinte du verre");
            b.row(PCT, F::Shadow, "Ombre");
            b.row(K::Toggle, F::Border, "Bordure fine");
            b.head("Forme");
            b.row(K::Slider(0.0, 40.0, " px", 1.0), F::Radius, "Arrondi des coins");
            b.row(K::Toggle, F::Cont, "Coins continus (style iPhone)");
            b.head("Couleur");
            b.row(K::Swatch, F::Accent, "Couleur d'accent");
            b.row(K::Toggle, F::AccentAuto, "Prendre la couleur du fond d'écran");
        }
        1 => {
            b.head("Horloge");
            b.row(K::Toggle, F::ClockOn, "Afficher l'horloge");
            b.row(K::Toggle, F::Secs, "Secondes");
            b.row(K::Toggle, F::H24, "Format 24 h");
            b.row(K::Toggle, F::Date, "Date");
            b.row(K::Toggle, F::Week, "Numéro de semaine");
            b.head("Météo");
            b.row(K::Toggle, F::WeatherOn, "Afficher la météo");
            b.row(K::Text("Ville"), F::City, "Ville (Entrée pour valider)");
            b.row(K::Toggle, F::Hourly, "Heure par heure");
            b.row(K::Toggle, F::Daily, "Prochains jours");
            b.head("Calendrier");
            b.row(K::Toggle, F::CalOn, "Afficher le calendrier");
            b.row(K::Toggle, F::Agenda, "Liste « À venir »");
            b.row(K::Toggle, F::CalHol, "Surligner les vacances");
            b.head("Vacances et examen");
            b.row(K::Toggle, F::CountOn, "Afficher le compte à rebours");
            b.row(K::Text("Paris"), F::Acad, "Académie");
            b.row(K::Text("Brevet"), F::ExamName, "Nom de l'examen");
            b.row(K::Text("JJ/MM/AAAA"), F::ExamDate, "Date de l'examen");
            b.head("Mails");
            b.row(K::Toggle, F::MailOn, "Afficher les mails");
            b.row(K::Toggle, F::NotifyMail, "Notification pour les mails urgents");
            b.head("Pronote");
            b.row(K::Toggle, F::PnNext, "Prochain cours");
            b.row(K::Toggle, F::PnHw, "Devoirs");
            b.row(K::Toggle, F::PnGrades, "Notes");
            b.row(K::Toggle, F::PnInfo, "Annonces et messages");
            b.row(K::Toggle, F::PnNGrade, "Notification : nouvelle note");
            b.row(K::Toggle, F::PnNHw, "Notification : nouveau devoir");
            b.row(K::Toggle, F::PnNCancel, "Notification : cours annulé ou prof absent");
            b.head("Musique, Discord, imprimante");
            b.row(K::Toggle, F::MusicOn, "Musique en cours");
            b.row(K::Toggle, F::DiscordOn, "Notifications Discord");
            b.row(K::Toggle, F::NotifyDiscord, "Pop-up Jarvis pour Discord");
            b.row(K::Toggle, F::DcWin, "Lire les notifications Discord de Windows");
            b.row(K::Toggle, F::DcDmOnly, "Seulement les messages privés (MP)");
            b.row(K::Toggle, F::DcMentions, "Garder aussi les mentions sur les serveurs");
            b.row(K::Text("brevet, urgent, ce soir"), F::DcKeywords, "Mots-clés importants (vide = tous)");
            b.row(K::Text("pseudo1, pseudo2"), F::DcPeople, "Personnes importantes (vide = toutes)");
            b.row(K::Toggle, F::PrinterOn, "Imprimante 3D");
            b.row(K::Text("192.168.1.50"), F::PrinterIp, "Adresse IP de l'imprimante");
        }
        2 => {
            b.head("Nouvel événement");
            b.note("Clique sur un jour du calendrier pour remplir la date. L'événement est ajouté à ton premier compte Google.");
            b.row(K::Text("Titre"), F::EvTitle, "Titre");
            b.row(K::Text("JJ/MM/AAAA"), F::EvDate, "Date");
            b.row(K::Text("vide = toute la journée"), F::EvTime, "Heure (HH:MM)");
            b.row(K::Text("60"), F::EvDur, "Durée en minutes");
            b.row(K::Button("Créer"), F::EvCreate, "Ajouter à Google Agenda");
        }
        3 => {
            let a = &info.acc;
            b.head("Pronote");
            let p = &info.pn;
            if p.connected {
                b.note(&format!("Connecté : {}", p.who));
                if !p.state.is_empty() {
                    b.note(&p.state);
                }
                b.row(K::Button("Actualiser"), F::PnSync, "Actualiser maintenant");
                b.row(K::Toggle, F::PnGoogle, "Copier cours et devoirs dans Google Agenda");
                b.row(K::Button("Déconnecter"), F::PnDel, "Se déconnecter de Pronote");
            } else {
                b.note("1. Ouvre Pronote dans ton navigateur et affiche le QR code pour l'appli mobile (icône QR code en haut de la page). Choisis un code à 4 chiffres.");
                b.note("2. Fais Win + Maj + S et capture le QR code. 3. Clique « Coller », tape ton code puis « Se connecter ». Le QR code n'est valable que quelques minutes.");
                b.row(K::Button("Coller"), F::PnPaste, if p.qr { "QR code lu, prêt" } else { "Capture du QR code" });
                b.row(K::Text("4 chiffres"), F::PnPin, "Code à 4 chiffres");
                b.row(K::Button("Se connecter"), F::PnLogin, "Se connecter à Pronote");
            }
            b.head("Google (Agenda + Gmail)");
            if !a.google_json {
                b.note("Il manque le fichier google.json dans le dossier des réglages : suis le guide « Google » du fichier LISEZMOI.md (10 min).");
                b.row(K::Button("Ouvrir"), F::Folder, "Dossier des réglages");
            }
            for (i, e) in a.google.iter().enumerate() {
                b.row(K::Button("Retirer"), F::GDel(i as u8), e);
            }
            b.row(K::Button("Ajouter"), F::GAdd, "Ajouter un compte Google");
            b.head("Hotmail / Outlook");
            b.row(K::Text("ID d'application Azure"), F::MsClient, "ID d'application Microsoft");
            match &a.ms {
                Some(e) => b.row(K::Button("Retirer"), F::MsDel, e),
                None => b.row(K::Button("Connecter"), F::MsAdd, "Connecter Hotmail"),
            }
            b.head("Discord");
            b.note(&format!("Notifications Windows : {}", if info.winnotif.is_empty() { "désactivées" } else { &info.winnotif }));
            b.note(&format!("Connexion Discord (facultative) : {}", if info.discord.is_empty() { "à configurer" } else { &info.discord }));
            b.row(K::Text("ID d'application"), F::DcId, "ID d'application Discord");
            b.row(K::Text("Secret client"), F::DcSecret, "Secret client");
            b.row(K::Button("Autoriser"), F::DcAdd, "Connecter à Discord");
        }
        4 => {
            b.head("Ce que Jarvis utilise");
            b.note(&info.ia.en_service);
            b.head("Gratuit (proposé en premier)");
            for l in info.ia.lignes.iter().filter(|l| l.gratuit) {
                b.row(K::Ia, F::IaOn(l.index), &format!("{} · {}", l.nom, l.detail));
            }
            b.row(K::Button("Chercher"), F::IaChercher, "Chercher les IA gratuites sur ce PC");
            b.row(K::Button("Installer LM Studio"), F::IaLmStudio, "LM Studio : l'IA gratuite qui tourne sur ce PC");
            b.row(K::Button("Clé Gemini gratuite"), F::IaGemini, "Clé gratuite Google Gemini (ouvre le site)");
            b.row(K::Button("Clé Groq gratuite"), F::IaGroq, "Clé gratuite Groq (ouvre le site)");
            b.head("Payant (facultatif)");
            let payants: Vec<_> = info.ia.lignes.iter().filter(|l| !l.gratuit).collect();
            if payants.is_empty() {
                b.note("Aucun service payant pour l'instant. Ajoute une clé ci-dessous si tu en as une.");
            }
            for l in payants {
                b.row(K::Ia, F::IaOn(l.index), &format!("{} · {}", l.nom, l.detail));
            }
            b.head("Ajouter une clé d'API");
            b.note("Colle n'importe quelle clé (OpenAI, Anthropic, Groq, Gemini, OpenRouter…) : Jarvis devine le service et vérifie la clé. Elle est chiffrée et n'est plus affichée.");
            b.row(K::Text("Coller la clé, puis Entrée"), F::IaCle, "Clé d'API");
            b.row(K::Text("https://…/v1"), F::IaUrl, "Adresse du service (seulement pour une clé inconnue)");
            b.row(K::Text("vide = automatique"), F::IaModele, "Modèle de l'IA en premier");
            b.head("Mails");
            b.row(K::Toggle, F::AiOn, "Résumé et tri des mails par l'IA");
        }
        _ => {
            b.head("Comportement");
            b.row(K::Toggle, F::HideWin, "Pause quand une fenêtre recouvre tout le bureau (économise la mémoire)");
            b.row(K::Toggle, F::Autostart, "Lancer Jarvis au démarrage de Windows");
            b.head("Disposition");
            b.note("Glisse un widget pour le déplacer. Ctrl + molette sur un widget pour changer sa taille.");
            b.row(K::Toggle, F::Locked, "Verrouiller la disposition");
            b.row(K::Button("Modifier"), F::Edit, "Afficher les contours et tailles");
            b.row(K::Slider(0.0, 32.0, " px", 1.0), F::Grid, "Aimantation à la grille");
            b.row(K::Button("Réinitialiser"), F::Reset, "Disposition par défaut");
            b.head("Jarvis");
            b.note(&format!("Version 0.1  ·  {}", info.mem));
            b.row(K::Button("Ouvrir"), F::Folder, "Dossier des réglages");
            b.row(K::Button("Quitter"), F::Quit, "Quitter Jarvis");
        }
    }
    b.0
}

fn wrap(fonts: &Fonts, s: &str, size: f32, max: f32) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for w in s.split_whitespace() {
        let cand = if line.is_empty() { w.to_string() } else { format!("{line} {w}") };
        if !line.is_empty() && fonts.measure(Wt::Regular, size, &cand, 0.0) > max {
            out.push(std::mem::take(&mut line));
            line = w.to_string();
        } else {
            line = cand;
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn layout(st: &mut Settings, fonts: &Fonts, info: &Info) {
    let note_w = WIN_W - SIDE - 48.0 - 32.0;
    let mut y = TOP + 4.0;
    st.rows.clear();
    for (k, f, label) in spec(st.page, info) {
        let (h, lines) = match k {
            K::Head => (if st.rows.is_empty() { 34.0 } else { 50.0 }, Vec::new()),
            K::Note => {
                let l = wrap(fonts, &label, 13.5, note_w);
                (l.len() as f32 * 20.0 + 22.0, l)
            }
            _ => (46.0, Vec::new()),
        };
        st.rows.push(Row { k, f, label, y, h, lines });
        y += h;
    }
    st.bottom = y + 28.0;
    st.scroll = st.scroll.clamp(0.0, (st.bottom - WIN_H).max(0.0));
}

fn slider_span(xr: f32) -> (f32, f32) {
    (xr - 270.0, xr - 62.0)
}

fn swatch_x(xr: f32, i: usize) -> f32 {
    xr - 11.0 - (ACCENTS.len() - 1 - i) as f32 * 30.0
}

const XR: f32 = WIN_W - 40.0;

#[allow(clippy::too_many_arguments)]
fn control(c: &mut Ctx, st: &Settings, cfg: &Config, k: K, f: F, y: f32, info: &Info) {
    let t = c.t;
    let xr = XR;
    match k {
        K::Toggle => {
            let on = get_b(cfg, f);
            let tx = xr - 42.0;
            c.rr(tx, y + 11.0, 42.0, 24.0, 12.0, if on { t.accent } else { t.fg.a(0.16) });
            c.circle(if on { tx + 30.0 } else { tx + 12.0 }, y + 23.0, 10.0, WHITE);
        }
        K::Slider(min, max, unit, mul) => {
            let (a, b) = slider_span(xr);
            let v = get_f(cfg, f);
            let p = ((v - min) / (max - min)).clamp(0.0, 1.0);
            c.rr(a, y + 21.0, b - a, 4.0, 2.0, t.track);
            c.rr(a, y + 21.0, (b - a) * p, 4.0, 2.0, t.accent);
            let kx = a + (b - a) * p;
            c.circle(kx, y + 23.0, 10.5, Rgba(0.0, 0.0, 0.0, 0.2));
            c.circle(kx, y + 23.0, 9.5, WHITE);
            c.text_r(Wt::Regular, 13.5, xr, y + 28.0, t.fg2, &format!("{}{}", (v * mul).round() as i32, unit));
        }
        K::Seg(opts) => {
            let sw = 88.0;
            let x = xr - sw * opts.len() as f32;
            let sel = get_i(cfg, f);
            c.rr(x, y + 8.0, sw * opts.len() as f32, 30.0, 8.0, t.fg.a(0.08));
            c.rr(x + sw * sel as f32 + 2.0, y + 10.0, sw - 4.0, 26.0, 6.5, t.accent);
            for (i, o) in opts.iter().enumerate() {
                let on = i == sel;
                let (wt, col) = if on { (Wt::Semibold, WHITE) } else { (Wt::Regular, t.fg) };
                c.text_c(wt, 13.0, x + sw * (i as f32 + 0.5), y + 27.5, col, o);
            }
        }
        K::Swatch => {
            for (i, col) in ACCENTS.iter().enumerate() {
                let cx = swatch_x(xr, i);
                c.circle(cx, y + 23.0, 10.5, Rgba::from(*col));
                if !cfg.accent_auto && cfg.accent == *col {
                    c.ring(cx, y + 23.0, 14.0, t.fg, 2.0);
                }
            }
        }
        K::Text(ph) => {
            let (bx, bw) = (xr - 240.0, 240.0);
            let foc = st.focus == Some(f);
            c.rr(bx, y + 8.0, bw, 30.0, 7.0, t.fg.a(0.07));
            c.rr_stroke(bx, y + 8.0, bw, 30.0, 7.0, if foc { t.accent } else { t.fg.a(0.16) }, if foc { 2.0 } else { 1.0 });
            let txt: String = if foc { st.edit.iter().collect() } else { get_t(cfg, f) };
            let txt = if (f == F::DcSecret || f == F::PnPin) && !foc && !txt.is_empty() { "•".repeat(txt.chars().count().min(24)) } else { txt };
            if txt.is_empty() && !foc {
                c.text(Wt::Regular, 14.0, bx + 10.0, y + 28.0, t.fg3, ph);
            } else {
                let shown = c.fit(Wt::Regular, 14.0, &txt, bw - 20.0);
                c.text(Wt::Regular, 14.0, bx + 10.0, y + 28.0, t.fg, &shown);
            }
            if foc {
                let pre: String = st.edit[..st.caret.min(st.edit.len())].iter().collect();
                let cx = bx + 10.0 + c.measure(Wt::Regular, 14.0, &pre).min(bw - 20.0);
                c.line(cx, y + 14.0, cx, y + 32.0, 1.5, t.accent);
            }
        }
        K::Ia => {
            // ligne d'un service : interrupteur à droite, puis « En premier » et « Retirer »
            let Some(l) = info.ia.lignes.iter().find(|l| F::IaOn(l.index) == f) else { return };
            let tx = xr - 42.0;
            c.rr(tx, y + 11.0, 42.0, 24.0, 12.0, if l.on { t.accent } else { t.fg.a(0.16) });
            c.circle(if l.on { tx + 30.0 } else { tx + 12.0 }, y + 23.0, 10.0, WHITE);
            let (bx, bw) = (xr - 236.0, 100.0);
            if l.premier {
                c.rr(bx, y + 8.0, bw, 30.0, 8.0, t.accent);
                c.text_c(Wt::Semibold, 13.0, bx + bw / 2.0, y + 27.5, WHITE, "Premier");
            } else {
                c.rr(bx, y + 8.0, bw, 30.0, 8.0, t.fg.a(0.09));
                c.text_c(Wt::Semibold, 13.0, bx + bw / 2.0, y + 27.5, t.fg, "En premier");
            }
            let (dx, dw) = (xr - 128.0, 74.0);
            c.rr(dx, y + 8.0, dw, 30.0, 8.0, t.fg.a(0.09));
            c.text_c(Wt::Semibold, 13.0, dx + dw / 2.0, y + 27.5, t.fg, "Retirer");
        }
        K::Button(l) => {
            let l = if f == F::Edit && info.edit { "Terminer" } else { l };
            let bw = c.measure(Wt::Semibold, 13.0, l) + 30.0;
            let red = Rgba::rgb8(255, 69, 58);
            let (bg, fg) = if f == F::Quit {
                (red.a(0.15), red)
            } else if f == F::Edit && info.edit {
                (t.accent, WHITE)
            } else {
                (t.fg.a(0.09), t.fg)
            };
            c.rr(xr - bw, y + 8.0, bw, 30.0, 8.0, bg);
            c.text_c(Wt::Semibold, 13.0, xr - bw / 2.0, y + 27.5, fg, l);
        }
        _ => {}
    }
}

/// Dessine toute la fenêtre des paramètres.
pub fn draw(st: &mut Settings, cfg: &Config, fonts: &Fonts, t: &Theme, dpi: f32, info: &Info) -> Option<Pixmap> {
    layout(st, fonts, info);
    let mut pm = Pixmap::new((WIN_W * dpi).round() as u32, (WIN_H * dpi).round() as u32)?;
    let bg = if t.dark { Rgba::rgb8(30, 30, 32) } else { Rgba::rgb8(242, 242, 247) };
    let side = if t.dark { Rgba::rgb8(40, 40, 43) } else { Rgba::rgb8(228, 228, 233) };
    let group = if t.dark { Rgba(1.0, 1.0, 1.0, 0.055) } else { WHITE };
    pm.fill(bg.color());
    let st: &Settings = st;
    let mut c = Ctx::new(&mut pm, fonts, t, dpi, 0.0, 0.0);
    let x0 = SIDE + 24.0;
    let gw = WIN_W - SIDE - 48.0;
    let xl = x0 + 16.0;

    // cartes qui regroupent les lignes
    let mut i = 0;
    while i < st.rows.len() {
        if matches!(st.rows[i].k, K::Head) {
            i += 1;
            continue;
        }
        let start = i;
        while i < st.rows.len() && !matches!(st.rows[i].k, K::Head) {
            i += 1;
        }
        let gy = st.rows[start].y - st.scroll;
        let gh = st.rows[i - 1].y + st.rows[i - 1].h - st.rows[start].y;
        c.rr(x0, gy, gw, gh, 10.0, group);
    }
    for (ri, r) in st.rows.iter().enumerate() {
        let y = r.y - st.scroll;
        if y + r.h < TOP - 10.0 || y > WIN_H {
            continue;
        }
        if ri > 0 && !matches!(r.k, K::Head) && !matches!(st.rows[ri - 1].k, K::Head) {
            c.hline(xl, x0 + gw, y, t.sep);
        }
        match r.k {
            K::Head => {
                c.text(Wt::Semibold, 13.0, x0 + 4.0, y + r.h - 12.0, t.fg2, &r.label);
            }
            K::Note => {
                for (li, l) in r.lines.iter().enumerate() {
                    c.text(Wt::Regular, 13.5, xl, y + 26.0 + li as f32 * 20.0, t.fg2, l);
                }
            }
            k => {
                c.text(Wt::Regular, 14.0, xl, y + 28.0, t.fg, &r.label);
                if let Some(f) = r.f {
                    control(&mut c, st, cfg, k, f, y, info);
                }
            }
        }
    }

    // titre de la page (masque ce qui défile dessous)
    c.rr(SIDE, 0.0, WIN_W - SIDE, TOP, 0.0, bg);
    c.text(Wt::Semibold, 24.0, x0 + 4.0, 46.0, t.fg, PAGES[st.page]);

    // barre latérale
    c.rr(0.0, 0.0, SIDE, WIN_H, 0.0, side);
    c.text(Wt::Semibold, 22.0, 24.0, 46.0, t.fg, "Jarvis");
    c.text(Wt::Regular, 12.5, 24.0, 66.0, t.fg2, "Widgets de bureau");
    for (i, p) in PAGES.iter().enumerate() {
        let y = 96.0 + i as f32 * 40.0;
        if i == st.page {
            c.rr(12.0, y, SIDE - 24.0, 34.0, 8.0, t.accent);
            c.text(Wt::Semibold, 14.0, 26.0, y + 22.5, WHITE, p);
        } else {
            c.text(Wt::Regular, 14.0, 26.0, y + 22.5, t.fg, p);
        }
    }
    if !info.status.is_empty() {
        let lines = wrap(fonts, &info.status, 12.5, SIDE - 44.0);
        let y0 = WIN_H - 50.0 - lines.len() as f32 * 18.0;
        for (li, l) in lines.iter().enumerate() {
            c.text(Wt::Semibold, 12.5, 24.0, y0 + li as f32 * 18.0, t.accent, l);
        }
    }
    c.text(Wt::Regular, 12.5, 24.0, WIN_H - 22.0, t.fg3, &info.mem);
    drop(c);
    Some(pm)
}

/// Garde l'image prête à être affichée (format Windows BGRA).
pub fn store(st: &mut Settings, pm: &Pixmap) {
    st.size = (pm.width() as i32, pm.height() as i32);
    st.buf.resize(pm.data().len(), 0);
    gfx::rgba_to_bgra(pm.data(), &mut st.buf);
}

pub unsafe fn blit(st: &Settings, hdc: HDC) {
    if st.buf.is_empty() {
        return;
    }
    let (w, h) = st.size;
    let bmi = gfx::bmi32(w, h);
    SetDIBitsToDevice(hdc, 0, 0, w as u32, h as u32, 0, 0, 0, h as u32, st.buf.as_ptr() as _, &bmi, DIB_RGB_COLORS);
}

// ---------- Souris et clavier ----------

fn row_at(st: &Settings, x: f32, y: f32) -> Option<usize> {
    if x < SIDE || y < TOP {
        return None;
    }
    let cy = y + st.scroll;
    st.rows.iter().position(|r| cy >= r.y && cy < r.y + r.h)
}

pub fn commit(st: &mut Settings, cfg: &mut Config) -> Option<F> {
    let f = st.focus.take()?;
    let s = st.edit.iter().collect::<String>().trim().to_string();
    st.edit.clear();
    st.texte = s.clone();
    if s != get_t(cfg, f) {
        set_t(cfg, f, s);
        Some(f)
    } else {
        None
    }
}

/// Clic (coordonnées logiques). Renvoie les réglages modifiés / boutons appuyés.
pub fn down(st: &mut Settings, cfg: &mut Config, x: f32, y: f32) -> Vec<F> {
    let mut out = Vec::new();
    let hit = row_at(st, x, y);
    let hit_f = hit.and_then(|i| st.rows[i].f);
    if st.focus.is_some() && hit_f != st.focus {
        if let Some(f) = commit(st, cfg) {
            out.push(f);
        }
    }
    if x < SIDE {
        if y >= 96.0 {
            let i = ((y - 96.0) / 40.0) as usize;
            if i < PAGES.len() && i != st.page {
                st.set_page(i);
                st.focus = None;
            }
        }
        return out;
    }
    let Some(i) = hit else { return out };
    let (k, ry) = (st.rows[i].k, st.rows[i].y - st.scroll);
    let Some(f) = st.rows[i].f else { return out };
    match k {
        K::Toggle => {
            set_b(cfg, f, !get_b(cfg, f));
            out.push(f);
        }
        K::Slider(min, max, _, _) => {
            let (a, b) = slider_span(XR);
            if x >= a - 14.0 && x <= b + 14.0 {
                set_f(cfg, f, min + ((x - a) / (b - a)).clamp(0.0, 1.0) * (max - min));
                st.drag = Some(f);
                out.push(f);
            }
        }
        K::Seg(opts) => {
            let sw = 88.0;
            let x0 = XR - sw * opts.len() as f32;
            if x >= x0 {
                let j = ((x - x0) / sw) as usize;
                if j < opts.len() && j != get_i(cfg, f) {
                    set_i(cfg, f, j);
                    out.push(f);
                }
            }
        }
        K::Swatch => {
            for (j, col) in ACCENTS.iter().enumerate() {
                let cx = swatch_x(XR, j);
                if (x - cx).abs() <= 14.0 && (y - (ry + 23.0)).abs() <= 14.0 {
                    cfg.accent = *col;
                    cfg.accent_auto = false;
                    out.push(F::Accent);
                }
            }
        }
        K::Text(_) => {
            if st.focus != Some(f) {
                st.focus = Some(f);
                st.edit = get_t(cfg, f).chars().collect();
            }
            st.caret = st.edit.len();
        }
        K::Ia => {
            if let F::IaOn(i) = f {
                // la zone de droite : interrupteur ; au milieu : « En premier » ; à gauche de « Retirer » : « Retirer »
                if x >= XR - 42.0 {
                    out.push(F::IaOn(i));
                } else if (XR - 236.0..XR - 136.0).contains(&x) {
                    out.push(F::IaTop(i));
                } else if (XR - 128.0..XR - 54.0).contains(&x) {
                    out.push(F::IaDel(i));
                }
            }
        }
        K::Button(_) => {
            if x >= XR - 170.0 {
                out.push(f);
            }
        }
        _ => {}
    }
    out
}

pub fn drag(st: &mut Settings, cfg: &mut Config, x: f32) -> Option<F> {
    let f = st.drag?;
    let r = st.rows.iter().find(|r| r.f == Some(f))?;
    if let K::Slider(min, max, _, _) = r.k {
        let (a, b) = slider_span(XR);
        let v = min + ((x - a) / (b - a)).clamp(0.0, 1.0) * (max - min);
        let before = get_f(cfg, f);
        set_f(cfg, f, v);
        if (get_f(cfg, f) - before).abs() > 1e-4 {
            return Some(f);
        }
    }
    None
}

pub fn wheel(st: &mut Settings, d: i16) {
    st.scroll = (st.scroll - d as f32 / 120.0 * 50.0).clamp(0.0, (st.bottom - WIN_H).max(0.0));
}

pub fn on_char(st: &mut Settings, cfg: &mut Config, ch: u32) -> Option<F> {
    st.focus?;
    match ch {
        8 => {
            if st.caret > 0 {
                st.caret -= 1;
                st.edit.remove(st.caret);
            }
            None
        }
        13 => commit(st, cfg),
        27 => {
            st.focus = None;
            st.edit.clear();
            None
        }
        22 => {
            if let Some(t) = crate::sys::clipboard_text() {
                for c in t.chars().filter(|c| !c.is_control()) {
                    st.edit.insert(st.caret, c);
                    st.caret += 1;
                }
            }
            None
        }
        c if c >= 32 => {
            if let Some(c) = char::from_u32(c) {
                st.edit.insert(st.caret, c);
                st.caret += 1;
            }
            None
        }
        _ => None,
    }
}

pub fn on_key(st: &mut Settings, vk: u32) {
    if st.focus.is_none() {
        return;
    }
    match vk {
        0x25 => st.caret = st.caret.saturating_sub(1),
        0x27 => st.caret = (st.caret + 1).min(st.edit.len()),
        0x24 => st.caret = 0,
        0x23 => st.caret = st.edit.len(),
        0x2E => {
            if st.caret < st.edit.len() {
                st.edit.remove(st.caret);
            }
        }
        _ => {}
    }
}
