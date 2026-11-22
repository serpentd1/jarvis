//! Grande application › Paramètres : TOUS les réglages (grande appli, île, Jarvis, IA, comptes…),
//! rangés en pages avec leur liste à gauche. Remplace les petites fenêtres « Paramètres… » et
//! « Réglages de l'île… » (supprimées) : leurs réglages sont lus dans ile_settings.rs et settings.rs.

use crate::gfx::{Rgba, WHITE, Wt};
use crate::island::*;
use crate::island_big::{cachee, ordre, SECTIONS, SEC_PARAMS};
use crate::island_cards::caps;
use crate::island_draw::*;
use crate::settings::{self, F, K};
use crate::widgets::{Action, Ctx};

/// D'où viennent les lignes d'une page.
pub enum Src {
    /// Un onglet des options de l'île (n° dans ile_settings::TABS).
    Ile(usize),
    /// Une page de settings.rs (n° dans settings::PAGES), sans les réglages des widgets.
    Par(usize),
    /// Quelques réglages de settings.rs, sous un titre.
    ParF(&'static str, &'static [F]),
    /// Réglages de Jarvis (IA, profil, PAI, sons, voix…), voir island_big::jarvis_rows.
    Jarvis,
    /// Sections de la grande appli : montrer / cacher, monter / descendre.
    Sections,
}

pub struct Page {
    pub nom: &'static str,
    /// Titre de groupe affiché au-dessus de la page (vide = même groupe que la précédente).
    pub groupe: &'static str,
    pub src: &'static [Src],
}

pub const PAGES: [Page; 21] = [
    Page { nom: "Grande appli", groupe: "Grande appli", src: &[Src::Ile(14), Src::Sections] },
    Page { nom: "Couleurs et fond", groupe: "", src: &[Src::Ile(15)] },
    Page { nom: "Texte et lisibilité", groupe: "", src: &[Src::Ile(16), Src::Ile(3)] },
    Page { nom: "Animations", groupe: "", src: &[Src::Ile(17), Src::Ile(11)] },
    Page { nom: "Apparence", groupe: "L'île", src: &[Src::Ile(0), Src::ParF("Couleurs de Jarvis", &[F::Theme, F::Accent, F::AccentAuto])] },
    Page { nom: "Effets", groupe: "", src: &[Src::Ile(1)] },
    Page { nom: "Taille et position", groupe: "", src: &[Src::Ile(2)] },
    Page { nom: "Heure et date", groupe: "", src: &[Src::Ile(4)] },
    Page { nom: "Météo", groupe: "", src: &[Src::Ile(5), Src::ParF("Ville", &[F::City])] },
    Page { nom: "Modules", groupe: "", src: &[Src::Ile(6)] },
    Page { nom: "Minuteurs et alarme", groupe: "", src: &[Src::Ile(7)] },
    Page { nom: "Notifications", groupe: "", src: &[Src::Ile(10)] },
    Page { nom: "Options rigolotes", groupe: "", src: &[Src::Ile(12)] },
    Page { nom: "Disposition et onglets", groupe: "", src: &[Src::Ile(13)] },
    Page { nom: "Jarvis et voix", groupe: "Jarvis", src: &[Src::Jarvis] },
    Page { nom: "IA", groupe: "", src: &[Src::Ile(9), Src::Par(4)] },
    Page {
        nom: "Pronote, mails, Discord",
        groupe: "",
        src: &[
            Src::Ile(8),
            Src::ParF("Mails", &[F::MailOn, F::NotifyMail]),
            Src::ParF("Pronote", &[F::PnNext, F::PnHw, F::PnGrades, F::PnInfo, F::PnNGrade, F::PnNHw, F::PnNCancel]),
            Src::ParF(
                "Musique et Discord",
                &[F::MusicOn, F::DiscordOn, F::NotifyDiscord, F::DcWin, F::DcDmOnly, F::DcMentions, F::DcKeywords, F::DcPeople],
            ),
        ],
    },
    Page { nom: "Agenda et vacances", groupe: "", src: &[Src::Par(2), Src::ParF("Vacances et examen", &[F::Acad, F::ExamName, F::ExamDate])] },
    Page { nom: "Comptes", groupe: "", src: &[Src::Par(3)] },
    Page { nom: "Général", groupe: "", src: &[Src::Par(5)] },
    Page { nom: "Imprimante 3D", groupe: "", src: &[Src::ParF("Imprimante 3D", &[F::PrinterOn, F::PrinterIp, F::PrinterApp])] },
];

/// N° de la page « Imprimante 3D » (bouton « Adresse… » de la page Imprimante).
pub const PAGE_IMPRIMANTE: usize = 20;

/// Page qui remplace une page de l'ancienne fenêtre « Paramètres… ».
pub fn page_de_parametres(p: usize) -> usize {
    [4, 16, 17, 18, 15, 19].get(p).copied().unwrap_or(0)
}

/// Page qui contient un onglet de l'ancienne fenêtre « Réglages de l'île… ».
pub fn page_de_ile(tab: usize) -> usize {
    PAGES.iter().position(|p| p.src.iter().any(|s| matches!(s, Src::Ile(t) if *t == tab))).unwrap_or(4)
}

/// La page a-t-elle besoin des infos de settings.rs (comptes, IA…) ?
pub fn besoin_info(page: usize) -> bool {
    PAGES.get(page).is_some_and(|p| p.src.iter().any(|s| matches!(s, Src::Par(_) | Src::ParF(..))))
}

/// Onglets de l'île d'une page (pour « Réinitialiser cette page »).
pub fn onglets(page: usize) -> Vec<usize> {
    PAGES.get(page).map(|p| p.src.iter().filter_map(|s| if let Src::Ile(t) = s { Some(*t) } else { None }).collect()).unwrap_or_default()
}

/// Code d'un bouton d'une ligne de settings.rs : page, ligne (dans `settings::spec`), bouton.
pub fn code_par(page: usize, ligne: usize, bouton: usize) -> u16 {
    ((page << 11) | ((ligne & 127) << 4) | (bouton & 15)) as u16
}

pub fn decode_par(code: u16) -> (usize, usize, usize) {
    let c = code as usize;
    (c >> 11, (c >> 4) & 127, c & 15)
}

/// Une ligne affichée.
pub struct L {
    /// 0 = réglage, 1 = titre, 2 = texte d'aide.
    pub kind: u8,
    pub label: String,
    pub val: String,
    pub toggle: Option<(bool, Action)>,
    /// Boutons (action, texte, mis en valeur).
    pub chips: Vec<(Action, String, bool)>,
    /// Pastilles de couleur : couleur choisie, code de base des boutons.
    pub swatch: Option<(Option<usize>, u16)>,
    /// Saisie de texte liée à la ligne.
    pub input: Option<InputFor>,
}

impl L {
    fn new(kind: u8, label: impl Into<String>) -> L {
        L { kind, label: label.into(), val: String::new(), toggle: None, chips: Vec::new(), swatch: None, input: None }
    }
}

fn titre(v: &mut Vec<L>, t: &str) {
    v.push(L::new(1, t));
}

fn lignes_ile(v: &mut Vec<L>, r: &R, tab: usize) {
    let mut c = r.cfg.ile.clone();
    for (i, o) in crate::ile_settings::opts().iter().enumerate().filter(|(_, o)| o.tab == tab) {
        let mut l = L::new(0, o.label);
        let code = |b: usize| Action::IleN(N_REG_ILE, (i * 4 + b) as u16);
        match &o.k {
            crate::ile_settings::K::B(f) => l.toggle = Some((*f(&mut c), code(0))),
            crate::ile_settings::K::N(..) => {
                l.val = crate::ile_settings::val_txt(o, &mut c);
                l.chips = vec![(code(1), "−".into(), false), (code(2), "+".into(), false)];
            }
            crate::ile_settings::K::C(..) => {
                l.val = crate::ile_settings::val_txt(o, &mut c);
                l.chips = vec![(code(1), "‹".into(), false), (code(2), "›".into(), false)];
            }
        }
        v.push(l);
    }
}

/// Une ligne de settings.rs (n° `idx` dans `settings::spec(page)`).
fn ligne_par(r: &R, info: &settings::Info, page: usize, idx: usize, k: K, f: Option<F>, label: &str) -> L {
    let cfg = r.cfg;
    let a = |b: usize| Action::IleN(N_REG_PAR, code_par(page, idx, b));
    let mut l = L::new(0, label);
    let Some(f) = f else {
        l.kind = if matches!(k, K::Head) { 1 } else { 2 };
        return l;
    };
    match k {
        K::Head => l.kind = 1,
        K::Note => l.kind = 2,
        K::Toggle => l.toggle = Some((settings::get_b(cfg, f), a(0))),
        K::Slider(_, _, unit, mul) => {
            l.val = format!("{}{unit}", (settings::get_f(cfg, f) * mul).round());
            l.chips = vec![(a(0), "−".into(), false), (a(1), "+".into(), false)];
        }
        K::Seg(opts) => {
            let cur = settings::get_i(cfg, f);
            l.chips = opts.iter().enumerate().map(|(j, o)| (a(j), o.to_string(), j == cur)).collect();
        }
        K::Swatch => {
            let cur = if cfg.accent_auto { None } else { settings::ACCENTS.iter().position(|x| *x == cfg.accent) };
            l.swatch = Some((cur, code_par(page, idx, 0)));
        }
        K::Text(ph) => {
            let v = settings::get_t(cfg, f);
            l.val = if v.is_empty() {
                format!("({ph})")
            } else if matches!(f, F::DcSecret | F::PnPin) {
                "•".repeat(v.chars().count().min(12))
            } else {
                v
            };
            l.chips = vec![(a(0), "Modifier".into(), false)];
            l.input = Some(InputFor::Param(f));
        }
        K::Button(b) => l.chips = vec![(a(0), b.to_string(), false)],
        K::Ia => {
            if let Some(s) = info.ia.lignes.iter().find(|x| F::IaOn(x.index) == f) {
                l.toggle = Some((s.on, a(0)));
                l.chips = vec![(a(1), if s.premier { "Premier" } else { "En premier" }.into(), s.premier), (a(2), "Retirer".into(), false)];
            }
        }
    }
    l
}

/// Toutes les lignes d'une page.
pub fn construire(r: &R, page: usize) -> Vec<L> {
    let mut v = Vec::new();
    let Some(p) = PAGES.get(page) else { return v };
    let plusieurs = p.src.len() > 1;
    for s in p.src {
        match s {
            Src::Ile(t) => {
                if plusieurs {
                    titre(&mut v, crate::ile_settings::TABS.get(*t).copied().unwrap_or(""));
                }
                lignes_ile(&mut v, r, *t);
            }
            Src::Par(sp) => match r.info {
                Some(info) => {
                    for (idx, k, f, label) in settings::lignes(*sp, info) {
                        v.push(ligne_par(r, info, *sp, idx, k, f, &label));
                    }
                }
                None => v.push(L::new(2, "Chargement…")),
            },
            Src::ParF(t, fs) => {
                let Some(info) = r.info else { continue };
                titre(&mut v, t);
                for want in fs.iter() {
                    'pages: for sp in 0..settings::PAGES.len() {
                        for (idx, (k, f, label)) in settings::spec(sp, info).into_iter().enumerate() {
                            if f == Some(*want) {
                                v.push(ligne_par(r, info, sp, idx, k, f, &label));
                                break 'pages;
                            }
                        }
                    }
                }
            }
            Src::Jarvis => {
                for (lab, val, toggle, acts) in crate::island_big::jarvis_rows(r) {
                    let mut l = L::new(if acts.is_empty() && toggle.is_none() && val.is_empty() { 2 } else { 0 }, lab);
                    l.val = val;
                    if let Some(on) = toggle {
                        l.toggle = acts.first().map(|a| (on, a.0));
                    } else {
                        for (a, t) in &acts {
                            match a {
                                Action::IleN(N_SET_TEXT, k) => l.input = Some(InputFor::Setting(*k as usize)),
                                Action::IleN(N_PAI_TEXT, k) => l.input = Some(InputFor::Pai(*k as usize)),
                                _ => {}
                            }
                            l.chips.push((*a, t.to_string(), false));
                        }
                    }
                    v.push(l);
                }
            }
            Src::Sections => {
                titre(&mut v, "Sections de la colonne (montrer, ranger)");
                let c = r.c();
                for i in ordre(c) {
                    let mut l = L::new(0, SECTIONS[i].2);
                    if i == SEC_PARAMS {
                        l.val = "toujours là".into();
                    } else {
                        l.toggle = Some((!cachee(c, i), Action::IleN(N_REG_SEC, i as u16)));
                    }
                    l.chips = vec![(Action::IleN(N_REG_UP, i as u16), "Monter".into(), false), (Action::IleN(N_REG_DOWN, i as u16), "Descendre".into(), false)];
                    v.push(l);
                }
            }
        }
    }
    v
}

/// Dessine la page Paramètres de la grande appli.
pub fn page(c: &mut Ctx, r: &R, w: f32, h: f32) {
    let t = c.t;
    let p = r.ui.reg_page.min(PAGES.len() - 1);
    // ---- liste des pages ----
    let (nx, nw) = (SIDE - 10.0, 176.0);
    let groupes = PAGES.iter().filter(|g| !g.groupe.is_empty()).count() as f32;
    let rh = ((h - 6.0 - groupes * 22.0) / PAGES.len() as f32).clamp(17.0, 27.0);
    let fs = (rh * 0.5).clamp(11.0, 13.0);
    let mut y = 0.0;
    for (i, pg) in PAGES.iter().enumerate() {
        if !pg.groupe.is_empty() {
            caps(c, nx + 8.0, y + 15.0, t.fg3, pg.groupe);
            y += 22.0;
        }
        let on = i == p;
        if on {
            c.rr(nx, y, nw, rh - 2.0, 8.0, t.accent.a(0.85));
        }
        let wt = if on { Wt::Semibold } else { Wt::Regular };
        c.text(wt, fs, nx + 10.0, y + (rh - 2.0) / 2.0 + fs * 0.36, if on { WHITE } else { t.fg2 }, pg.nom);
        c.hit(nx, y, nw, rh - 2.0, Action::IleN(N_REG_PAGE, i as u16));
        y += rh;
    }
    c.line(nx + nw + 9.0, 4.0, nx + nw + 9.0, h - 8.0, 1.0, t.sep);
    // ---- réglages de la page ----
    let x0 = nx + nw + 24.0;
    let x1 = w - SIDE;
    let cw = x1 - x0;
    let lignes = construire(r, p);
    let bas = h - 30.0;
    let mut y = 2.0;
    let mut restants = false;
    for l in lignes.iter().skip(r.st.scroll) {
        match l.kind {
            1 => {
                if y + 30.0 > bas {
                    restants = true;
                    break;
                }
                caps(c, x0, y + 18.0, t.fg2, &l.label);
                y += 26.0;
            }
            2 => {
                let ls = c.wrap(Wt::Regular, 12.0, &l.label, cw - 8.0);
                if y + 16.0 * ls.len() as f32 + 6.0 > bas {
                    restants = true;
                    break;
                }
                for s in &ls {
                    c.text(Wt::Regular, 12.0, x0 + 2.0, y + 13.0, t.fg2, s);
                    y += 16.0;
                }
                y += 6.0;
            }
            _ => {
                if y + 36.0 > bas {
                    restants = true;
                    break;
                }
                ligne(c, r, l, x0, x1, y);
                y += 40.0;
            }
        }
    }
    // ---- pied de page ----
    let aide = if !r.status.is_empty() && besoin_info(p) {
        r.status.to_string()
    } else if restants || r.st.scroll > 0 {
        "Molette pour voir la suite. Certains réglages servent au prochain démarrage de Jarvis.".to_string()
    } else {
        "Tout est enregistré tout seul.".to_string()
    };
    let mut xr = x1;
    if !onglets(p).is_empty() {
        let lab = "Réinitialiser cette page";
        let bw = c.measure(Wt::Semibold, 12.0, lab) + 20.0;
        xr -= bw;
        chip(c, xr, h - 26.0, lab, t.fg.a(0.1), t.fg2, Some(Action::IleN(N_REG_RESET, p as u16)));
        xr -= 10.0;
    }
    let aide = c.fit(Wt::Regular, 11.5, &aide, xr - x0);
    c.text(Wt::Regular, 11.5, x0, h - 9.0, t.fg3, &aide);
}

/// Une ligne de réglage : fond, nom à gauche, valeur et boutons à droite.
fn ligne(c: &mut Ctx, r: &R, l: &L, x0: f32, x1: f32, y: f32) {
    let t = c.t;
    let cw = x1 - x0;
    c.rr(x0 - 6.0, y, cw + 12.0, 36.0, 10.0, t.fg.a(0.05));
    let mut x = x1;
    if let Some((on, act)) = l.toggle {
        let (sx, sy) = (x - 40.0, y + 8.0);
        c.rr(sx, sy, 40.0, 20.0, 10.0, if on { t.accent } else { t.track });
        c.circle(if on { sx + 30.0 } else { sx + 10.0 }, sy + 10.0, 8.0, WHITE);
        if l.chips.is_empty() {
            c.hit(x0 - 6.0, y, cw + 12.0, 36.0, act);
        } else {
            c.hit(sx - 6.0, y, 52.0, 36.0, act);
        }
        x -= 50.0;
    }
    for (a, lab, mis) in l.chips.iter().rev() {
        let bw = c.measure(Wt::Semibold, 12.0, lab) + 20.0;
        x -= bw;
        let (bg, fg) = if *mis { (t.accent.a(0.9), WHITE) } else { (t.fg.a(0.12), t.fg) };
        chip(c, x, y + 6.0, lab, bg, fg, Some(*a));
        x -= 6.0;
    }
    if let Some((cur, base)) = l.swatch {
        for j in (0..settings::ACCENTS.len()).rev() {
            let cx = x - 13.0;
            let [rr, gg, bb] = settings::ACCENTS[j];
            c.circle(cx, y + 18.0, 10.0, Rgba::rgb8(rr, gg, bb));
            if cur == Some(j) {
                c.ring(cx, y + 18.0, 12.5, t.fg, 1.6);
            }
            c.hit(cx - 13.0, y + 4.0, 26.0, 28.0, Action::IleN(N_REG_PAR, base + j as u16));
            x -= 27.0;
        }
    }
    let editing = r.st.typing && l.input.is_some_and(|k| k == r.st.input_for);
    let shown = if editing { format!("{}|", r.st.input) } else { l.val.clone() };
    let lab_max = (cw * 0.62).min(x - x0 - 60.0).max(80.0);
    let lab = c.fit(Wt::Regular, 13.0, &l.label, lab_max);
    let lw = c.measure(Wt::Regular, 13.0, &lab);
    c.text(Wt::Regular, 13.0, x0 + 4.0, y + 23.0, t.fg, &lab);
    if !shown.is_empty() {
        let vmax = (x - 8.0 - (x0 + 4.0 + lw + 14.0)).max(40.0);
        let vv = c.fit(Wt::Semibold, 12.5, &shown, vmax);
        c.text_r(Wt::Semibold, 12.5, x - 8.0, y + 23.0, if editing { t.accent } else { t.fg2 }, &vv);
    }
}
