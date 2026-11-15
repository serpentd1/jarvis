#![windows_subsystem = "windows"]
//! Installer-Jarvis.exe — installe Jarvis + la barre Givre dans %LOCALAPPDATA%\Programs\Jarvis,
//! sans droits administrateur. Une fenêtre simple : options, barre de progression, journal.
//!
//!   Installer-Jarvis.exe                    installation normale
//!   Installer-Jarvis.exe --desinstaller     désinstallation (lancé depuis « Applications installées »)
//!   Installer-Jarvis.exe --auto --dossier D --sans-raccourcis   (tests : sans clic, sans toucher au système)

mod etapes;

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, GetSysColorBrush, SetBkMode, COLOR_WINDOW, DEFAULT_CHARSET, FW_BOLD, FW_NORMAL, HDC, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, BST_CHECKED, EM_REPLACESEL, EM_SETSEL, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES,
    INITCOMMONCONTROLSEX, PBM_SETPOS, PBM_SETRANGE32,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WM_JOURNAL: u32 = WM_APP + 1;
pub const WM_ETAT: u32 = WM_APP + 2;
pub const WM_PROGRES: u32 = WM_APP + 3;
pub const WM_FINI: u32 = WM_APP + 4;

const ID_INSTALLER: i32 = 1;
const ID_FERMER: i32 = 2;
const ID_DEMARRAGE: i32 = 3;
const ID_BUREAU: i32 = 4;
const ID_OLLAMA: i32 = 5;
const ID_LANCER: i32 = 6;
const ID_EXTENSION: i32 = 7;

static FENETRE: AtomicIsize = AtomicIsize::new(0);
static EN_COURS: AtomicBool = AtomicBool::new(false);
static OPTIONS: Mutex<Option<etapes::Options>> = Mutex::new(None);
static CONTROLES: Mutex<Vec<(i32, isize)>> = Mutex::new(Vec::new());

pub fn large(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn boite(texte: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let h = FENETRE.load(Ordering::Relaxed) as HWND;
    unsafe { MessageBoxW(h, large(texte).as_ptr(), large("Jarvis").as_ptr(), style) }
}

/// Envoie un message à la fenêtre depuis le fil d'installation (le texte voyage dans une Box).
pub fn poster(msg: u32, w: usize, texte: Option<String>) {
    let h = FENETRE.load(Ordering::Relaxed) as HWND;
    let l = texte.map(|t| Box::into_raw(Box::new(t)) as isize).unwrap_or(0);
    if h.is_null() || unsafe { PostMessageW(h, msg, w, l) } == 0 {
        if l != 0 {
            drop(unsafe { Box::from_raw(l as *mut String) });
        }
    }
}

fn controle(id: i32) -> HWND {
    CONTROLES.lock().unwrap().iter().find(|(i, _)| *i == id).map(|(_, h)| *h as HWND).unwrap_or(std::ptr::null_mut())
}

fn coche(id: i32) -> bool {
    unsafe { SendMessageW(controle(id), BM_GETCHECK, 0, 0) as u32 == BST_CHECKED }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = |nom: &str| args.iter().any(|x| x == nom);
    let valeur = |nom: &str| args.iter().position(|x| x == nom).and_then(|i| args.get(i + 1)).cloned();

    if a("--desinstaller") {
        etapes::desinstaller();
        return;
    }
    let mut options = etapes::Options::par_defaut();
    if let Some(d) = valeur("--dossier") {
        options.dossier = d.into();
    }
    options.raccourcis = !a("--sans-raccourcis");
    options.auto = a("--auto");
    options.relancer = a("--relancer");
    *OPTIONS.lock().unwrap() = Some(options);
    fenetre();
}

fn fenetre() {
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_PROGRESS_CLASS | ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&icc);
        let instance = GetModuleHandleW(std::ptr::null());
        let classe = large("JarvisInstalleur");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(procedure),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: LoadIconW(instance, 1 as *const u16),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            lpszMenuName: std::ptr::null(),
            lpszClassName: classe.as_ptr(),
        };
        RegisterClassW(&wc);
        let e = GetDpiForSystem() as i32;
        let s = |v: i32| v * e / 96;
        let (largeur, hauteur) = (s(640), s(560));
        let x = (GetSystemMetrics(SM_CXSCREEN) - largeur) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - hauteur) / 2;
        let h = CreateWindowExW(
            0,
            classe.as_ptr(),
            large("Installer Jarvis").as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
            x,
            y,
            largeur,
            hauteur,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        FENETRE.store(h as isize, Ordering::Relaxed);
        construire(h, instance, &s);
        if OPTIONS.lock().unwrap().as_ref().is_some_and(|o| o.auto) {
            demarrer_installation();
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(h, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::process::exit(etapes::CODE_SORTIE.load(Ordering::Relaxed));
    }
}

unsafe fn construire(h: HWND, instance: *mut core::ffi::c_void, s: &dyn Fn(i32) -> i32) {
    let police = |taille: i32, gras: bool| {
        CreateFontW(-s(taille), 0, 0, 0, if gras { FW_BOLD } else { FW_NORMAL } as i32, 0, 0, 0,
                    DEFAULT_CHARSET as u32, 0, 0, 5, 0, large("Segoe UI").as_ptr())
    };
    let (titre, normal) = (police(22, true), police(13, false));
    let ajouter = |classe: &str, texte: &str, style: u32, x: i32, y: i32, l: i32, ht: i32, id: i32, gros: bool| {
        let c = CreateWindowExW(
            0, large(classe).as_ptr(), large(texte).as_ptr(), WS_CHILD | WS_VISIBLE | style,
            s(x), s(y), s(l), s(ht), h, id as usize as HMENU, instance, std::ptr::null(),
        );
        SendMessageW(c, WM_SETFONT, if gros { titre } else { normal } as usize, 1);
        CONTROLES.lock().unwrap().push((id, c as isize));
        c
    };
    ajouter("STATIC", "Jarvis", 0, 24, 16, 580, 34, 100, true);
    ajouter("STATIC",
        "Assistant vocal 100 % local et gratuit (« Hey Jarvis ») + la barre Givre en haut de l'écran.\n\
         Tout s'installe dans ton dossier utilisateur, sans droits administrateur. Il faut Internet \
         et environ 8 Go d'espace (modèles de voix et d'IA).",
        0, 24, 56, 580, 60, 101, false);
    let cases = [
        (ID_DEMARRAGE, "Démarrer Jarvis avec Windows"),
        (ID_BUREAU, "Créer un raccourci sur le Bureau"),
        (ID_OLLAMA, "Installer Ollama (le moteur d'IA, gratuit) s'il n'est pas déjà là"),
    ];
    for (i, (id, texte)) in cases.iter().enumerate() {
        let c = ajouter("BUTTON", texte, (BS_AUTOCHECKBOX | WS_TABSTOP as i32) as u32, 24, 124 + i as i32 * 26, 580, 24, *id, false);
        SendMessageW(c, BM_SETCHECK, BST_CHECKED as usize, 0);
    }
    let barre = ajouter("msctls_progress32", "", 0, 24, 212, 580, 18, 9, false);
    SendMessageW(barre, PBM_SETRANGE32, 0, 1000);
    ajouter("STATIC", "Prêt à installer.", 0, 24, 236, 580, 22, 10, false);
    ajouter("EDIT", "",
        (WS_BORDER | WS_VSCROLL) | (ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32,
        24, 262, 580, 200, 8, false);
    ajouter("BUTTON", "Installer", (BS_DEFPUSHBUTTON | WS_TABSTOP as i32) as u32, 384, 476, 106, 32, ID_INSTALLER, false);
    ajouter("BUTTON", "Fermer", (BS_PUSHBUTTON | WS_TABSTOP as i32) as u32, 498, 476, 106, 32, ID_FERMER, false);
    ajouter("BUTTON", "Lancer Jarvis", (BS_PUSHBUTTON | WS_TABSTOP as i32) as u32, 24, 476, 120, 32, ID_LANCER, false);
    ajouter("BUTTON", "Extension Chrome…", (BS_PUSHBUTTON | WS_TABSTOP as i32) as u32, 152, 476, 150, 32, ID_EXTENSION, false);
    ShowWindow(controle(ID_LANCER), SW_HIDE);
    ShowWindow(controle(ID_EXTENSION), SW_HIDE);
}

fn demarrer_installation() {
    if EN_COURS.swap(true, Ordering::Relaxed) {
        return;
    }
    let mut options = OPTIONS.lock().unwrap().clone().unwrap_or_else(etapes::Options::par_defaut);
    options.demarrage = coche(ID_DEMARRAGE) && options.raccourcis;
    options.bureau = coche(ID_BUREAU) && options.raccourcis;
    options.ollama = coche(ID_OLLAMA);
    unsafe {
        for id in [ID_INSTALLER, ID_DEMARRAGE, ID_BUREAU, ID_OLLAMA] {
            EnableWindow(controle(id), 0);
        }
    }
    *OPTIONS.lock().unwrap() = Some(options.clone());
    std::thread::spawn(move || etapes::installer(&options));
}

unsafe extern "system" fn procedure(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match (w & 0xFFFF) as i32 {
                ID_INSTALLER => demarrer_installation(),
                ID_FERMER => {
                    PostMessageW(h, WM_CLOSE, 0, 0);
                }
                ID_LANCER => {
                    if let Some(o) = OPTIONS.lock().unwrap().as_ref() {
                        etapes::lancer_jarvis(&o.dossier);
                    }
                    PostMessageW(h, WM_CLOSE, 0, 0);
                }
                ID_EXTENSION => {
                    if let Some(o) = OPTIONS.lock().unwrap().as_ref() {
                        etapes::aide_extension(&o.dossier);
                    }
                }
                _ => {}
            }
            0
        }
        WM_JOURNAL | WM_ETAT => {
            let texte = *Box::from_raw(l as *mut String);
            if msg == WM_ETAT {
                SetWindowTextW(controle(10), large(&texte).as_ptr());
            } else {
                let edit = controle(8);
                let longueur = GetWindowTextLengthW(edit) as usize;
                if longueur > 60_000 {
                    SendMessageW(edit, EM_SETSEL, 0, 20_000);
                    SendMessageW(edit, EM_REPLACESEL, 0, large("").as_ptr() as isize);
                }
                let fin = GetWindowTextLengthW(edit) as usize;
                SendMessageW(edit, EM_SETSEL, fin, fin as isize);
                SendMessageW(edit, EM_REPLACESEL, 0, large(&format!("{texte}\r\n")).as_ptr() as isize);
            }
            0
        }
        WM_PROGRES => {
            SendMessageW(controle(9), PBM_SETPOS, w, 0);
            0
        }
        WM_FINI => {
            EN_COURS.store(false, Ordering::Relaxed);
            let auto = OPTIONS.lock().unwrap().as_ref().is_some_and(|o| o.auto);
            if auto {
                PostMessageW(h, WM_CLOSE, 0, 0);
            } else if w == 1 {
                ShowWindow(controle(ID_INSTALLER), SW_HIDE);
                ShowWindow(controle(ID_LANCER), SW_SHOW);
                ShowWindow(controle(ID_EXTENSION), SW_SHOW);
            } else {
                EnableWindow(controle(ID_INSTALLER), 1);
                SetWindowTextW(controle(ID_INSTALLER), large("Réessayer").as_ptr());
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            SetBkMode(w as HDC, TRANSPARENT as i32);
            GetSysColorBrush(COLOR_WINDOW) as LRESULT
        }
        WM_CLOSE => {
            if EN_COURS.load(Ordering::Relaxed)
                && boite("L'installation est en cours. Arrêter quand même ?", MB_YESNO | MB_ICONQUESTION) != IDYES
            {
                return 0;
            }
            DestroyWindow(h);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(h, msg, w, l),
    }
}
