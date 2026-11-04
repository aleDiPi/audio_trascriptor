//! Icona in basso a destra mentre la scorciatoia e' attiva: mostra l'icona dell'app dove verra' incollato il
//! testo, con un alone che si allarga col volume del microfono. Finestra "a strati" sempre in primo piano,
//! senza focus e trasparente ai clic: non cambia l'app di destinazione. Quando e' nascosta non usa CPU
//! (il timer parte solo mentre e' visibile) ne' memoria grafica (la bitmap si crea e si libera a ogni uso).

use crate::levels;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const SIZE: i32 = 160;
const DISC_R: f32 = 31.0; // disco scuro sotto l'icona
const HALO_MIN: f32 = 3.0;
const HALO_MAX: f32 = 44.0;
const ICON_PX: i32 = 40;
const TIMER: usize = 1;
const FRAME_MS: u32 = 33;

const WM_SHOW_REC: u32 = WM_APP + 1;
const WM_SHOW_PROC: u32 = WM_APP + 2;
const WM_HIDE: u32 = WM_APP + 3;

static HWND_SELF: AtomicIsize = AtomicIsize::new(0);
static STATE: AtomicU8 = AtomicU8::new(0); // 0 nascosta, 1 registrazione, 2 elaborazione
static ICON: AtomicIsize = AtomicIsize::new(0); // HICON posseduta dall'icona (da distruggere)

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Avvia il thread della finestra (resta in attesa di messaggi: nessun consumo da ferma).
pub fn start() {
    std::thread::spawn(|| unsafe {
        // L'app e' "per monitor DPI" (testo nitido nelle impostazioni); questa finestra ha misure fisse in pixel
        // e resta quindi non consapevole del DPI: Windows la scala come prima.
        windows_sys::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_UNAWARE);
        let class = w("TrascrittoreOverlay");
        let hinst = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            class.as_ptr(),
            w("Trascrittore").as_ptr(),
            WS_POPUP,
            0,
            0,
            SIZE,
            SIZE,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        );
        if hwnd.is_null() {
            crate::config::log("recording icon: window not created");
            return;
        }
        HWND_SELF.store(hwnd as isize, Ordering::Release);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

fn post(msg: u32, lparam: isize) {
    let h = HWND_SELF.load(Ordering::Acquire);
    if h != 0 {
        unsafe { PostMessageW(h as HWND, msg, 0, lparam) };
    }
}

/// Mostra l'icona in modalita' registrazione. `icon` e' una HICON copiata (la finestra la distrugge).
pub fn show_recording(icon: Option<isize>) {
    if !crate::config::overlay_enabled() {
        if let Some(i) = icon {
            unsafe { DestroyIcon(i as HICON) };
        }
        return;
    }
    post(WM_SHOW_REC, icon.unwrap_or(0));
}

pub fn show_processing() {
    post(WM_SHOW_PROC, 0);
}

pub fn hide() {
    post(WM_HIDE, 0);
}

struct Frame {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    dist: Vec<f32>, // distanza di ogni pixel dal centro, calcolata una volta
    smooth: levels::Smoother,
    origin: POINT,
}

static mut FRAME: Option<Frame> = None;

unsafe fn frame() -> Option<&'static mut Frame> {
    (*std::ptr::addr_of_mut!(FRAME)).as_mut()
}

unsafe fn create_frame(hwnd: HWND) {
    free_frame();
    let screen = GetDC(null_mut());
    let dc = CreateCompatibleDC(screen);
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = SIZE;
    bi.bmiHeader.biHeight = -SIZE; // dall'alto in basso
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut std::ffi::c_void = null_mut();
    let bmp = CreateDIBSection(dc, &bi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    ReleaseDC(null_mut(), screen);
    if bmp.is_null() || bits.is_null() {
        DeleteDC(dc);
        return;
    }
    let old = SelectObject(dc, bmp);
    let c = SIZE as f32 / 2.0 - 0.5;
    let mut dist = Vec::with_capacity((SIZE * SIZE) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            dist.push((dx * dx + dy * dy).sqrt());
        }
    }
    // in basso a destra dell'area di lavoro del monitor dove si trova l'app in primo piano
    let mon = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    let origin = if GetMonitorInfoW(mon, &mut mi) != 0 {
        POINT { x: mi.rcWork.right - SIZE - 12, y: mi.rcWork.bottom - SIZE - 12 }
    } else {
        POINT { x: 100, y: 100 }
    };
    let _ = hwnd;
    FRAME = Some(Frame { dc, bmp, old, bits: bits as *mut u32, dist, smooth: levels::Smoother::new(), origin });
}

unsafe fn free_frame() {
    if let Some(f) = (*std::ptr::addr_of_mut!(FRAME)).take() {
        SelectObject(f.dc, f.old);
        DeleteObject(f.bmp);
        DeleteDC(f.dc);
    }
}

fn take_icon() -> Option<HICON> {
    let i = ICON.swap(0, Ordering::AcqRel);
    (i != 0).then_some(i as HICON)
}

/// Un fotogramma: alone (pixel premoltiplicati), disco, icona dell'app; poi lo manda a Windows.
unsafe fn render(hwnd: HWND) {
    let Some(f) = frame() else { return };
    let state = STATE.load(Ordering::Relaxed);
    let t = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 100_000).unwrap_or(0)) as f32 / 1000.0;
    let target = if state == 1 { levels::unit(levels::take_peak()) } else { 0.5 + 0.5 * (t * 4.0).sin() };
    let u = f.smooth.step(target);
    let halo = levels::radius(u, HALO_MIN, HALO_MAX);
    // registrazione: rosso; elaborazione: ambra che "respira"
    let (r, g, b) = if state == 1 { (235.0, 64.0, 64.0) } else { (240.0, 170.0, 40.0) };
    let px = std::slice::from_raw_parts_mut(f.bits, (SIZE * SIZE) as usize);
    for (i, d) in f.dist.iter().enumerate() {
        let d = *d;
        let (mut a, mut cr, mut cg, mut cb) = (0.0f32, r, g, b);
        if d <= DISC_R + halo {
            // alone: pieno vicino al disco, sfuma verso l'esterno
            let k = ((DISC_R + halo - d) / halo.max(1.0)).clamp(0.0, 1.0);
            a = 0.80 * k * k;
        }
        // disco scuro con bordo morbido
        let disc = (DISC_R + 0.5 - d).clamp(0.0, 1.0);
        if disc > 0.0 {
            a = a * (1.0 - disc) + disc;
            cr = cr * (1.0 - disc) + 30.0 * disc;
            cg = cg * (1.0 - disc) + 30.0 * disc;
            cb = cb * (1.0 - disc) + 34.0 * disc;
        }
        let (pr, pg, pb) = ((cr * a) as u32, (cg * a) as u32, (cb * a) as u32);
        px[i] = (((a * 255.0) as u32) << 24) | (pr << 16) | (pg << 8) | pb;
    }
    let icon = ICON.load(Ordering::Acquire);
    if icon != 0 {
        let o = (SIZE - ICON_PX) / 2;
        DrawIconEx(f.dc, o, o, icon as HICON, ICON_PX, ICON_PX, 0, null_mut(), DI_NORMAL);
        // GDI non scrive il canale alpha: dentro il disco il fondo e' opaco
        for (i, d) in f.dist.iter().enumerate() {
            if *d <= DISC_R - 1.0 {
                px[i] |= 0xFF00_0000;
            }
        }
    }
    let size = SIZE_ { cx: SIZE, cy: SIZE };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: 1 };
    UpdateLayeredWindow(hwnd, null_mut(), &f.origin, &size, f.dc, &src, 0, &blend, ULW_ALPHA);
}

#[allow(non_camel_case_types)]
type SIZE_ = windows_sys::Win32::Foundation::SIZE;

unsafe fn show(hwnd: HWND, state: u8) {
    STATE.store(state, Ordering::Relaxed);
    if frame().is_none() {
        create_frame(hwnd);
        levels::take_peak(); // scarta il vecchio picco
    }
    render(hwnd);
    if let Some(f) = frame() {
        SetWindowPos(hwnd, HWND_TOPMOST, f.origin.x, f.origin.y, SIZE, SIZE, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
    SetTimer(hwnd, TIMER, FRAME_MS, None);
}

unsafe fn hide_window(hwnd: HWND) {
    KillTimer(hwnd, TIMER);
    STATE.store(0, Ordering::Relaxed);
    ShowWindow(hwnd, SW_HIDE);
    free_frame();
    if let Some(i) = take_icon() {
        DestroyIcon(i);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_SHOW_REC => {
            if let Some(old) = take_icon() {
                DestroyIcon(old);
            }
            ICON.store(lp, Ordering::Release);
            show(hwnd, 1);
            0
        }
        WM_SHOW_PROC => {
            if STATE.load(Ordering::Relaxed) != 0 {
                STATE.store(2, Ordering::Relaxed);
            }
            0
        }
        WM_HIDE => {
            hide_window(hwnd);
            0
        }
        WM_TIMER if wp == TIMER => {
            if STATE.load(Ordering::Relaxed) != 0 {
                render(hwnd);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
