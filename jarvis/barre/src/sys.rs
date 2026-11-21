//! Petits services Windows : registre, fenêtres ouvertes, mémoire, presse-papiers, journal.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, SetProcessWorkingSetSize};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ---------- Chiffrement Windows (DPAPI : lisible seulement par ta session) ----------

use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData,
};

fn dpapi(data: &[u8], protect: bool) -> Option<Vec<u8>> {
    unsafe {
        let inb = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
        let ok = if protect {
            CryptProtectData(&inb, null(), null(), null(), null(), 1, &mut out)
        } else {
            CryptUnprotectData(&inb, null_mut(), null(), null(), null(), 1, &mut out)
        };
        if ok == 0 || out.pbData.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as _);
        Some(v)
    }
}

pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    dpapi(data, true)
}

pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    dpapi(data, false)
}

pub fn random(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    unsafe {
        BCryptGenRandom(null_mut(), v.as_mut_ptr(), n as u32, 2);
    }
    v
}

pub fn open_url(url: &str) {
    let f = wide(url);
    unsafe {
        ShellExecuteW(null_mut(), wide("open").as_ptr(), f.as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}

/// Copie un texte dans le presse-papiers.
pub fn copy_text(s: &str) {
    let w = wide(s);
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return;
        }
        EmptyClipboard();
        let h = GlobalAlloc(GMEM_MOVEABLE, w.len() * 2);
        if !h.is_null() {
            let p = GlobalLock(h) as *mut u16;
            if p.is_null() {
                GlobalFree(h);
            } else {
                std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
                GlobalUnlock(h);
                if SetClipboardData(13, h).is_null() {
                    GlobalFree(h); // CF_UNICODETEXT refusé : on libère nous-mêmes
                }
            }
        }
        CloseClipboard();
    }
}

/// Retire les caractères invisibles que Windows met autour des noms (U+2066 à U+2069, U+200E, U+200F).
pub fn clean(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}')).collect::<String>().trim().to_string()
}

/// Message d'un serveur Discord (titre « Pseudo (#salon, Serveur) ») et non message privé.
pub fn discord_is_server(title: &str) -> bool {
    clean(title).contains("(#")
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn copy_wide(dst: &mut [u16], s: &str) {
    let mut i = 0;
    for c in s.encode_utf16() {
        if i + 1 >= dst.len() {
            break;
        }
        dst[i] = c;
        i += 1;
    }
    dst[i] = 0;
}

pub fn log(msg: &str) {
    use std::io::Write;
    let p = crate::config::dir().join("givre.log");
    if std::fs::metadata(&p).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(&p);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
    }
}

// ---------- Registre ----------

fn reg_string(key: &str, val: &str) -> Option<String> {
    let (k, v) = (wide(key), wide(val));
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), RRF_RT_REG_SZ, null_mut(), buf.as_mut_ptr() as _, &mut len)
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..n]))
}

fn reg_dword(key: &str, val: &str) -> Option<u32> {
    let (k, v) = (wide(key), wide(val));
    let mut d: u32 = 0;
    let mut len = 4u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), RRF_RT_REG_DWORD, null_mut(), &mut d as *mut u32 as _, &mut len)
    };
    (r == ERROR_SUCCESS).then_some(d)
}

pub fn apps_light_theme() -> bool {
    reg_dword("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize", "AppsUseLightTheme")
        .map(|v| v != 0)
        .unwrap_or(false)
}

/// (WallpaperStyle, TileWallpaper) : 10 remplir, 6 ajuster, 2 étirer, 0 centrer/mosaïque, 22 étendre.
pub fn wallpaper_style() -> (u32, bool) {
    let s = reg_string("Control Panel\\Desktop", "WallpaperStyle").and_then(|s| s.trim().parse().ok()).unwrap_or(10);
    let t = reg_string("Control Panel\\Desktop", "TileWallpaper").map(|s| s.trim() == "1").unwrap_or(false);
    (s, t)
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
/// Une seule entrée « Jarvis » (comme core/demarrage.py) : Jarvis.exe lance la voix, qui lance cette barre.
const RUN_VALUE: &str = "Jarvis";
/// Ancienne entrée : la barre seule. (« Givre » n'est plus touché : c'est une appli à part.)
const OLD_RUN_VALUES: [&str; 1] = ["JarvisBarre"];

pub fn autostart_enabled() -> bool {
    reg_string(RUN_KEY, RUN_VALUE).is_some()
}

/// `jarvis` = Jarvis.exe s'il est trouvé ; sinon on démarre cette barre seule.
pub fn set_autostart(on: bool, jarvis: Option<std::path::PathBuf>) {
    let k = wide(RUN_KEY);
    unsafe {
        for old in OLD_RUN_VALUES {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), wide(old).as_ptr());
        }
        let v = wide(RUN_VALUE);
        if on {
            let Some(exe) = jarvis.or_else(|| std::env::current_exe().ok()) else { return };
            let data = wide(&format!("\"{}\"", exe.display()));
            RegSetKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), REG_SZ, data.as_ptr() as _, (data.len() * 2) as u32);
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr());
        }
    }
}

/// Passage à l'appli unique : si la barre seule ou Givre démarraient avec Windows, c'est remplacé par
/// une seule entrée « Jarvis » (plus de deuxième icône au démarrage). Rien à faire sinon.
pub fn autostart_one_app(jarvis: Option<std::path::PathBuf>) {
    if OLD_RUN_VALUES.iter().any(|v| reg_string(RUN_KEY, v).is_some()) {
        log("démarrage avec Windows : anciennes entrées (barre, Givre) remplacées par « Jarvis »");
        set_autostart(true, jarvis.or_else(|| reg_string(RUN_KEY, RUN_VALUE).map(|s| s.trim_matches('"').into())));
    }
}

// ---------- Fenêtres ouvertes ----------

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

const IGNORED: [&str; 14] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "NotifyIconOverflowWindow",
    "ApplicationManager_ImmersiveShellWindow",
    "EdgeUiInputTopWndClass",
    "Shell_InputSwitchTopLevelWindow",
    "ForegroundStaging",
    "MultitaskingViewFrame",
    "Windows.Internal.Shell.TabProxyWindow",
];

/// Vrai si cette fenêtre est une « vraie » fenêtre ouverte (non réduite) qui cache le bureau.
unsafe fn counts(h: HWND) -> bool {
    if IsWindowVisible(h) == 0 || IsIconic(h) != 0 {
        return false;
    }
    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
    let app = ex & WS_EX_APPWINDOW != 0;
    if (ex & WS_EX_TOOLWINDOW != 0 || ex & WS_EX_NOACTIVATE != 0) && !app {
        return false;
    }
    if ex & WS_EX_TRANSPARENT != 0 && ex & WS_EX_LAYERED != 0 {
        return false;
    }
    let owner = GetWindow(h, GW_OWNER);
    if !owner.is_null() && IsWindowVisible(owner) != 0 {
        return false;
    }
    let mut cloaked: u32 = 0;
    if DwmGetWindowAttribute(h, DWMWA_CLOAKED as _, &mut cloaked as *mut u32 as _, 4) == 0 && cloaked != 0 {
        return false;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, &mut pid);
    if pid == GetCurrentProcessId() {
        return false;
    }
    let cls = class_name(h);
    if IGNORED.contains(&cls.as_str()) {
        return false;
    }
    let mut r: RECT = std::mem::zeroed();
    GetWindowRect(h, &mut r);
    if r.right - r.left < 60 || r.bottom - r.top < 60 {
        return false;
    }
    let (sw, sh) = screen_size();
    !(r.right <= 0 || r.bottom <= 0 || r.left >= sw || r.top >= sh)
}

/// Rectangle visible d'une fenêtre (sans les bordures invisibles de Windows 10/11).
unsafe fn visible_rect(h: HWND) -> RECT {
    let mut r: RECT = std::mem::zeroed();
    if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS as _, &mut r as *mut RECT as _, size_of::<RECT>() as u32) != 0 {
        GetWindowRect(h, &mut r);
    }
    r
}

/// Vrai si le rectangle `r` recouvre entièrement la zone de travail `wa` (à 2 pixels près).
pub fn covers(r: &RECT, wa: &RECT) -> bool {
    const TOL: i32 = 2;
    r.left <= wa.left + TOL && r.top <= wa.top + TOL && r.right >= wa.right - TOL && r.bottom >= wa.bottom - TOL
}

unsafe extern "system" fn enum_cb(h: HWND, lp: LPARAM) -> i32 {
    if !counts(h) {
        return 1;
    }
    let st = &mut *(lp as *mut (RECT, Option<String>));
    if !covers(&visible_rect(h), &st.0) {
        return 1;
    }
    st.1 = Some(class_name(h));
    0
}

/// Une fenêtre d'appli ouverte (même réduite) : poignée, programme (sans .exe, en minuscules), titre.
pub struct Fenetre {
    pub h: isize,
    pub exe: String,
    pub titre: String,
}

/// Vrai pour une fenêtre d'appli qu'on voit dans la barre des tâches (réduite ou non).
unsafe fn est_appli(h: HWND) -> bool {
    if IsWindowVisible(h) == 0 {
        return false;
    }
    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TOOLWINDOW != 0 && ex & WS_EX_APPWINDOW == 0 {
        return false;
    }
    if !GetWindow(h, GW_OWNER).is_null() {
        return false;
    }
    let mut cloaked: u32 = 0;
    if DwmGetWindowAttribute(h, DWMWA_CLOAKED as _, &mut cloaked as *mut u32 as _, 4) == 0 && cloaked != 0 {
        return false;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, &mut pid);
    pid != GetCurrentProcessId() && !IGNORED.contains(&class_name(h).as_str())
}

fn titre_fenetre(h: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn exe_fenetre(h: HWND) -> String {
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if p.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, 0, buf.as_mut_ptr(), &mut n);
        CloseHandle(p);
        if ok == 0 {
            return String::new();
        }
        let chemin = String::from_utf16_lossy(&buf[..n as usize]);
        std::path::Path::new(&chemin).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
    }
}

unsafe extern "system" fn enum_applis(h: HWND, lp: LPARAM) -> i32 {
    if est_appli(h) {
        let titre = titre_fenetre(h);
        if !titre.trim().is_empty() {
            (*(lp as *mut Vec<Fenetre>)).push(Fenetre { h: h as isize, exe: exe_fenetre(h), titre });
        }
    }
    1
}

/// Les fenêtres d'applis ouvertes, de la plus en avant à la plus en arrière (réduites comprises).
pub fn fenetres_ouvertes() -> Vec<Fenetre> {
    let mut v: Vec<Fenetre> = Vec::new();
    unsafe {
        EnumWindows(Some(enum_applis), &mut v as *mut _ as LPARAM);
    }
    v
}

/// Met une fenêtre devant les autres (et la ressort si elle était réduite).
pub fn mettre_devant(h: isize) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_MENU};
    let h = h as HWND;
    unsafe {
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        // Windows ne laisse une appli en arrière-plan passer une fenêtre devant qu'après une touche : un « Alt » invisible
        keybd_event(VK_MENU as u8, 0, 0, 0);
        keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
        BringWindowToTop(h);
        SetForegroundWindow(h) != 0
    }
}

/// Classe de la première fenêtre ouverte qui recouvre TOUT le bureau (zone de travail de l'écran
/// principal). Une fenêtre qui n'en cache qu'une partie ne compte pas. None = bureau visible.
pub fn covering_window() -> Option<String> {
    let mut st: (RECT, Option<String>) = (work_area(), None);
    unsafe {
        EnumWindows(Some(enum_cb), &mut st as *mut _ as LPARAM);
    }
    st.1
}

unsafe extern "system" fn find_desktop(h: HWND, lp: LPARAM) -> i32 {
    if !FindWindowExW(h, null_mut(), wide("SHELLDLL_DefView").as_ptr(), null()).is_null() {
        *(lp as *mut HWND) = h;
        return 0;
    }
    1
}

/// Fenêtre de premier niveau qui affiche le bureau (icônes + fond d'écran).
fn desktop_window() -> HWND {
    let mut d: HWND = null_mut();
    unsafe {
        EnumWindows(Some(find_desktop), &mut d as *mut HWND as LPARAM);
        if d.is_null() {
            d = FindWindowW(wide("Progman").as_ptr(), null());
        }
    }
    d
}

/// Place la fenêtre juste au-dessus du bureau, sous toutes les fenêtres normales, sans l'activer :
/// comme un widget de bureau (jamais « toujours au premier plan »). Après Win+D, le bureau passe
/// devant : le widget le suit et reste visible.
pub unsafe fn pin_to_desktop(h: HWND) {
    let f = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSENDCHANGING;
    let desk = desktop_window();
    if desk.is_null() {
        SetWindowPos(h, HWND_BOTTOM, 0, 0, 0, 0, f);
        return;
    }
    // fenêtre directement au-dessus du bureau, en sautant nos propres widgets
    let mut above = GetWindow(desk, GW_HWNDPREV);
    while !above.is_null() && (above == h || is_ours(above)) {
        above = GetWindow(above, GW_HWNDPREV);
    }
    // rien au-dessus du bureau, ou seulement des fenêtres « toujours au premier plan » (barre des
    // tâches…) : haut de la pile normale, qui reste sous ces fenêtres-là
    if above.is_null() || GetWindowLongPtrW(above, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 {
        SetWindowPos(h, HWND_TOP, 0, 0, 0, 0, f);
    } else {
        SetWindowPos(h, above, 0, 0, 0, 0, f);
    }
}

fn is_ours(h: HWND) -> bool {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(h, &mut pid);
        pid == GetCurrentProcessId()
    }
}

/// Remonte la fenêtre au-dessus des autres sans l'activer (mode édition seulement).
pub unsafe fn raise(h: HWND) {
    let f = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, f);
    SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, f);
}

// ---------- Écran, mémoire, divers ----------

pub fn screen_size() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

pub fn work_area() -> RECT {
    let mut r: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as _, 0) == 0 {
            let (w, h) = screen_size();
            r = RECT { left: 0, top: 0, right: w, bottom: h };
        }
    }
    r
}

pub fn dpi() -> f32 {
    (unsafe { GetDpiForSystem() }).max(96) as f32 / 96.0
}

/// Rend à Windows la mémoire qui ne sert pas (appelé quand les widgets sont cachés).
pub fn trim_memory() {
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

pub fn mem_text() -> String {
    unsafe {
        let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) == 0 {
            return String::new();
        }
        format!("RAM utilisée : {:.1} Mo", pmc.WorkingSetSize as f64 / 1_048_576.0).replace('.', ",")
    }
}

pub fn clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return None;
        }
        let mut out = None;
        let h = GetClipboardData(13);
        if !h.is_null() {
            let p = GlobalLock(h as _) as *const u16;
            if !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 && n < 4096 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(h as _);
            }
        }
        CloseClipboard();
        out
    }
}

/// Image du presse-papiers (capture Win+Maj+S) en niveaux de gris : (largeur, hauteur, pixels).
pub fn clipboard_image() -> Option<(usize, usize, Vec<u8>)> {
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return None;
        }
        let mut out = None;
        let h = GetClipboardData(8); // CF_DIB
        if !h.is_null() {
            let p = GlobalLock(h as _) as *const u8;
            if !p.is_null() {
                let size = windows_sys::Win32::System::Memory::GlobalSize(h as _);
                out = parse_dib(std::slice::from_raw_parts(p, size));
                GlobalUnlock(h as _);
            }
        }
        CloseClipboard();
        out
    }
}

pub fn parse_dib(b: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let u32_at = |o: usize| Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?));
    let hs = u32_at(0)? as usize;
    let w = u32_at(4)? as i32;
    let hh = u32_at(8)? as i32;
    let bpp = u16::from_le_bytes(b.get(14..16)?.try_into().ok()?) as usize;
    let comp = u32_at(16)?;
    let used = u32_at(32)? as usize;
    if w <= 0 || hh == 0 || !(bpp == 24 || bpp == 32) || w > 10000 || hh.abs() > 10000 {
        return None;
    }
    let (w, h) = (w as usize, hh.unsigned_abs() as usize);
    let off = hs + if comp == 3 && hs == 40 { 12 } else { 0 } + used * 4;
    let stride = (w * bpp).div_ceil(32) * 4;
    let px = bpp / 8;
    let mut luma = vec![0u8; w * h];
    for y in 0..h {
        let sy = if hh > 0 { h - 1 - y } else { y };
        let row = b.get(off + sy * stride..off + sy * stride + w * px)?;
        for x in 0..w {
            let (bb, g, r) = (row[x * px] as u32, row[x * px + 1] as u32, row[x * px + 2] as u32);
            luma[y * w + x] = ((r * 299 + g * 587 + bb * 114) / 1000) as u8;
        }
    }
    Some((w, h, luma))
}

pub fn open_path(p: &std::path::Path) {
    let f = wide(&p.display().to_string());
    unsafe {
        ShellExecuteW(null_mut(), wide("open").as_ptr(), f.as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}

/// Menus clic droit en mode sombre/clair (fonction non documentée d'uxtheme, sans risque si absente).
pub fn set_menu_theme(dark: bool) {
    unsafe {
        let lib = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if lib.is_null() {
            return;
        }
        if let Some(f) = GetProcAddress(lib, 135usize as *const u8) {
            let f: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            f(if dark { 2 } else { 3 });
        }
        if let Some(f) = GetProcAddress(lib, 136usize as *const u8) {
            let f: unsafe extern "system" fn() = std::mem::transmute(f);
            f();
        }
    }
}

// ---------- Petits sons doux (synthétisés, jamais un son système) ----------

fn wav(notes: &[(f32, f32, f32)]) -> Vec<u8> {
    // (fréquence de départ, fréquence d'arrivée, durée en s)
    const RATE: u32 = 22050;
    let mut pcm: Vec<i16> = Vec::new();
    for &(f0, f1, dur) in notes {
        let n = (dur * RATE as f32) as usize;
        let mut ph = 0.0f32;
        for i in 0..n {
            let t = i as f32 / n as f32;
            let f = f0 + (f1 - f0) * t;
            ph += f / RATE as f32 * std::f32::consts::TAU;
            let env = (i as f32 / (0.006 * RATE as f32)).min(1.0) * (-4.5 * t).exp();
            let v = (ph.sin() * 0.75 + (2.0 * ph).sin() * 0.12) * env * 0.11;
            pcm.push((v * i16::MAX as f32) as i16);
        }
    }
    let data_len = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}

/// Son doux : 0 = ouverture de l'île, 1 = notification.
/// Les petits sons de la barre (jarvis/idees/sons-et-effets-barre.md) : du verre doux, très court, jamais fort.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Son {
    /// Idée 1 : un « tic » de verre, un peu plus aigu ou plus grave à chaque fois.
    Clic,
    /// Idée 2 : deux notes qui montent (allumer), qui descendent (éteindre), une qui descend (fermer).
    Allume,
    Eteint,
    Ferme,
    /// Idée 3 : un « fffft » feutré pour changer d'onglet.
    Onglet,
    /// Idée 4 : un tiroir doux quand la grande île se déplie, et l'inverse.
    Deplie,
    Replie,
    /// Idée 5 : un « ding » cristallin quand on coche une tâche.
    Coche,
    /// Idée 8 : une erreur gentille (deux notes graves).
    Erreur,
    /// Idée 10 : la fin d'un minuteur, un carillon qui monte doucement.
    Carillon,
}

fn hasard() -> f32 {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(7);
    let mut x = n ^ 0x9E37_79B9;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    (x % 10_000) as f32 / 10_000.0
}

/// Notes en verre : une fondamentale et deux harmoniques « pas justes » (2,76 et 5,4), comme un verre qu'on frappe.
fn verre(notes: &[(f32, f32, f32)], volume: f32, souffle: bool) -> Vec<i16> {
    const RATE: f32 = 22050.0;
    let mut pcm = Vec::new();
    if souffle {
        let n = (0.075 * RATE) as usize;
        let (mut graine, mut lisse) = (12345u32, 0.0f32);
        for i in 0..n {
            graine = graine.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let b = ((graine >> 16) & 0x7fff) as f32 / 16384.0 - 1.0;
            lisse += (b - lisse) * 0.18;
            let t = i as f32 / n as f32;
            let env = (t * std::f32::consts::PI).sin().powi(2);
            pcm.push((lisse * env * 0.22 * volume * i16::MAX as f32) as i16);
        }
        return pcm;
    }
    for &(f0, f1, dur) in notes {
        let n = (dur * RATE) as usize;
        let mut ph = 0.0f32;
        for i in 0..n {
            let t = i as f32 / n as f32;
            ph += (f0 + (f1 - f0) * t) / RATE * std::f32::consts::TAU;
            let env = (i as f32 / (0.003 * RATE)).min(1.0) * (-6.5 * t).exp();
            let v = ph.sin() * 0.62 + (2.76 * ph).sin() * 0.22 + (5.4 * ph).sin() * 0.07;
            pcm.push((v * env * 0.16 * volume * i16::MAX as f32) as i16);
        }
    }
    pcm
}

fn en_wav(pcm: &[i16]) -> Vec<u8> {
    const RATE: u32 = 22050;
    let data_len = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for v in pcm {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

/// Joue un son de la barre au volume donné (0 à 1). PlaySoundW coupe le son d'avant : parfait pour des clics.
pub fn son(s: Son, volume: f32) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    let v = volume.clamp(0.0, 1.0);
    if v < 0.02 {
        return;
    }
    let clic = 1950.0 * (0.93 + 0.14 * hasard());
    let notes: Vec<(f32, f32, f32)> = match s {
        Son::Clic => vec![(clic, clic * 0.99, 0.05)],
        Son::Allume => vec![(1319.0, 1319.0, 0.05), (1760.0, 1760.0, 0.08)],
        Son::Eteint => vec![(1760.0, 1760.0, 0.05), (1319.0, 1319.0, 0.08)],
        Son::Ferme => vec![(1568.0, 1175.0, 0.08)],
        Son::Onglet => Vec::new(),
        Son::Deplie => vec![(392.0, 784.0, 0.14)],
        Son::Replie => vec![(784.0, 392.0, 0.12)],
        Son::Coche => vec![(2637.0, 2637.0, 0.14)],
        Son::Erreur => vec![(330.0, 330.0, 0.09), (262.0, 262.0, 0.15)],
        Son::Carillon => vec![(523.0, 523.0, 0.16), (659.0, 659.0, 0.16), (784.0, 784.0, 0.16), (1047.0, 1047.0, 0.32)],
    };
    let wav = en_wav(&verre(&notes, v, s == Son::Onglet));
    // le son joue depuis la mémoire : on garde les derniers en vie le temps qu'ils jouent
    static GARDES: std::sync::Mutex<Vec<Vec<u8>>> = std::sync::Mutex::new(Vec::new());
    let mut g = GARDES.lock().unwrap_or_else(|e| e.into_inner());
    g.push(wav);
    if g.len() > 3 {
        g.remove(0);
    }
    if let Some(b) = g.last() {
        unsafe { PlaySoundW(b.as_ptr() as _, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
    }
}

pub fn soft_sound(kind: usize) {
    use std::sync::OnceLock;
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    static SOUNDS: OnceLock<[Vec<u8>; 2]> = OnceLock::new();
    let s = SOUNDS.get_or_init(|| [wav(&[(620.0, 880.0, 0.09)]), wav(&[(880.0, 880.0, 0.08), (1175.0, 1175.0, 0.12)])]);
    let b = &s[kind.min(1)];
    unsafe { PlaySoundW(b.as_ptr() as _, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT) };
}
