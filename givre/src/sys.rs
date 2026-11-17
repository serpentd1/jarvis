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
const RUN_VALUE: &str = "Givre";

pub fn autostart_enabled() -> bool {
    reg_string(RUN_KEY, RUN_VALUE).is_some()
}

pub fn set_autostart(on: bool) {
    let (k, v) = (wide(RUN_KEY), wide(RUN_VALUE));
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let data = wide(&format!("\"{}\"", exe.display()));
            RegSetKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), REG_SZ, data.as_ptr() as _, (data.len() * 2) as u32);
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr());
        }
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
