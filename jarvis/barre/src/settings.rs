//! Paramètres de Jarvis (comptes, IA, agenda, Pronote…) : description des pages et lecture / écriture
//! des réglages. Ils s'affichent dans la grande application › Paramètres (voir reglages.rs).

use crate::config::Config;

pub const PAGES: [&str; 6] = ["Apparence", "Widgets", "Agenda", "Comptes", "IA", "Général"];
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
    PrinterApp,
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
pub enum K {
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
    pub mem: String,
    pub acc: crate::data::AccountsInfo,
    pub discord: String,
    pub winnotif: String,
    pub ia: crate::ia::Vue,
}

// ---------- Lecture / écriture des réglages ----------

pub fn get_b(c: &Config, f: F) -> bool {
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

pub fn set_b(c: &mut Config, f: F, v: bool) {
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

pub fn get_f(c: &Config, f: F) -> f32 {
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

pub fn set_f(c: &mut Config, f: F, v: f32) {
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

pub fn get_i(c: &Config, f: F) -> usize {
    match f {
        F::Theme => c.theme,
        F::Weight => c.clock_weight,
        _ => 0,
    }
}

pub fn set_i(c: &mut Config, f: F, v: usize) {
    match f {
        F::Theme => c.theme = v,
        F::Weight => c.clock_weight = v,
        _ => {}
    }
}

pub fn get_t(c: &Config, f: F) -> String {
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
        F::PrinterApp => c.printer.app.clone(),
        F::EvTitle => c.draft.title.clone(),
        F::EvDate => c.draft.date.clone(),
        F::EvTime => c.draft.time.clone(),
        F::EvDur => c.draft.dur.clone(),
        F::PnPin => c.pn_pin.clone(),
        _ => String::new(),
    }
}

pub fn set_t(c: &mut Config, f: F, v: String) {
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
        F::PrinterIp => c.printer.ip = v.trim().to_string(),
        F::PrinterApp => c.printer.app = v.trim().trim_matches('"').to_string(),
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

pub fn spec(page: usize, info: &Info) -> Vec<(K, Option<F>, String)> {
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
            b.row(K::Toggle, F::PrinterOn, "Suivre mon imprimante 3D (Elegoo)");
            b.row(K::Text("vide = trouvée toute seule"), F::PrinterIp, "Adresse IP de l'imprimante");
            b.row(K::Text("vide = Elegoo Slicer, OrcaSlicer…"), F::PrinterApp, "Slicer à ouvrir (chemin du .exe)");
        }
        2 => {
            b.head("Nouvel événement");
            b.note("Clique sur un jour du calendrier pour remplir la date. L'événement est ajouté à ton premier compte Google.");
            b.row(K::Text("Titre"), F::EvTitle, "Titre");
            b.row(K::Text("JJ/MM/AAAA"), F::EvDate, "Date");
            b.row(K::Text("vide = toute la journée"), F::EvTime, "Heure (HH:MM)");
            b.row(K::Text("60"), F::EvDur, "Durée en minutes");
            b.row(K::Button("Créer"), F::EvCreate, "Ajouter à Google Agenda");
            b.note("Ton emploi du temps aménagé (PAI), le mode de l'IA et le profil du PC sont dans la grande application › Paramètres.");
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

/// Réglages qui ne servent qu'aux widgets du bureau (ils sont dans Givre maintenant) : pas affichés.
fn widget_seul(f: F) -> bool {
    matches!(
        f,
        F::Weight | F::Scale | F::Blur | F::BlurAmt | F::Opacity | F::Shadow | F::Border | F::Radius | F::Cont
            | F::ClockOn | F::Secs | F::H24 | F::Date | F::Week | F::WeatherOn | F::Hourly | F::Daily
            | F::CalOn | F::Agenda | F::CalHol | F::CountOn | F::HideWin | F::Grid | F::Locked | F::Edit | F::Reset
    )
}

/// Lignes d'une page (avec leur n° dans `spec`), sans les réglages des widgets ni les titres restés vides.
pub fn lignes(page: usize, info: &Info) -> Vec<(usize, K, Option<F>, String)> {
    let v: Vec<_> = spec(page, info)
        .into_iter()
        .enumerate()
        .filter(|(_, (k, f, t))| !f.is_some_and(widget_seul) && !(matches!(k, K::Note) && (t.starts_with("Glisse") || t.starts_with("Ton emploi"))))
        .map(|(i, (k, f, t))| (i, k, f, t))
        .collect();
    let mut out = Vec::new();
    for (i, l) in v.iter().enumerate() {
        let vide = matches!(l.1, K::Head) && v.get(i + 1).map_or(true, |n| matches!(n.1, K::Head));
        if !vide {
            out.push(l.clone());
        }
    }
    out
}
