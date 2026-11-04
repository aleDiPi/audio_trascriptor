//! Bollino in basso a destra mentre la scorciatoia e' attiva (come su Windows): icona dell'app dove verra' incollato
//! il testo, con un alone che si allarga col volume del microfono. Finestra X11 senza cornice (override-redirect),
//! sempre in primo piano e trasparente ai clic e al focus: non cambia l'app di destinazione. Con un compositore
//! (XFCE di default) l'alone e' sfumato; senza, la finestra e' ritagliata a cerchio. Da ferma non consuma CPU
//! (il thread attende un comando) ne' memoria grafica (la finestra si crea e si distrugge a ogni uso).

use crate::levels;
use crate::overlay_render as r;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

enum Cmd {
    Recording(Option<r::Icon>),
    Processing,
    Hide,
}

static TX: Mutex<Option<Sender<Cmd>>> = Mutex::new(None);
const FRAME: Duration = Duration::from_millis(33);
const MARGIN: i32 = 12;

/// Avvia il thread del bollino (resta in attesa di comandi: nessun consumo da fermo).
pub fn start() {
    let (tx, rx) = mpsc::channel();
    *TX.lock().unwrap() = Some(tx);
    std::thread::spawn(move || {
        let mut win: Option<Overlay> = None;
        let mut recording = true;
        let mut icon: Option<Vec<u32>> = None;
        let mut smooth = levels::Smoother::new();
        loop {
            let cmd = if win.is_some() {
                match rx.recv_timeout(FRAME) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                match rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                }
            };
            match cmd {
                Some(Cmd::Recording(i)) => {
                    icon = i.map(|i| r::fit(&i, r::ICON_PX));
                    recording = true;
                    if win.is_none() {
                        match Overlay::new() {
                            Ok(o) => {
                                win = Some(o);
                                levels::take_peak(); // scarta il vecchio picco
                                smooth = levels::Smoother::new();
                            }
                            Err(e) => crate::config::log(&format!("recording icon: {e}")),
                        }
                    }
                }
                Some(Cmd::Processing) => recording = false,
                Some(Cmd::Hide) => {
                    win = None; // Drop distrugge la finestra
                    icon = None;
                }
                None => {}
            }
            if let Some(o) = win.as_mut() {
                let t = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 100_000).unwrap_or(0)) as f32 / 1000.0;
                let target = if recording { levels::unit(levels::take_peak()) } else { 0.5 + 0.5 * (t * 4.0).sin() };
                let halo = levels::radius(smooth.step(target), r::HALO_MIN, r::HALO_MAX);
                if let Err(e) = o.draw(halo, recording, icon.as_deref()) {
                    crate::config::log(&format!("recording icon: {e}"));
                    win = None;
                }
            }
        }
    });
}

fn send(c: Cmd) {
    if let Some(tx) = TX.lock().unwrap().as_ref() {
        let _ = tx.send(c);
    }
}

/// Mostra il bollino in modalita' registrazione con l'icona dell'app di destinazione.
pub fn show_recording(icon: Option<r::Icon>) {
    if crate::config::overlay_enabled() {
        send(Cmd::Recording(icon));
    }
}

pub fn show_processing() {
    send(Cmd::Processing);
}

pub fn hide() {
    send(Cmd::Hide);
}

struct Overlay {
    conn: RustConnection,
    win: Window,
    gc: Gcontext,
    cmap: Colormap,
    dist: Vec<f32>,
    px: Vec<u32>,
}

impl Overlay {
    fn new() -> Result<Overlay, String> {
        let (conn, screen_num) = RustConnection::connect(None).map_err(|e| format!("X11 not reachable: {e}"))?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        // visuale a 32 bit con canale alpha (serve un compositore per la trasparenza)
        let (depth, visual) = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .find_map(|d| d.visuals.iter().find(|v| v.class == VisualClass::TRUE_COLOR).map(|v| (32u8, v.visual_id)))
            .ok_or("no 32-bit visual")?;
        let cmap = conn.generate_id().map_err(|e| e.to_string())?;
        conn.create_colormap(ColormapAlloc::NONE, cmap, root, visual).map_err(|e| e.to_string())?;
        let (x, y) = r::corner(work_area(&conn, root, screen), MARGIN);
        let win = conn.generate_id().map_err(|e| e.to_string())?;
        let aux = CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0)
            .override_redirect(1)
            .colormap(cmap)
            .event_mask(EventMask::NO_EVENT);
        let size = r::SIZE as u16;
        conn.create_window(depth, win, root, x as i16, y as i16, size, size, 0, WindowClass::INPUT_OUTPUT, visual, &aux)
            .map_err(|e| e.to_string())?;
        // trasparente ai clic: regione di input vuota; ritaglio circolare per quando manca il compositore
        let _ = conn.shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &[]);
        if let Ok(mask) = circle_mask(&conn, win) {
            let _ = conn.shape_mask(shape::SO::SET, shape::SK::BOUNDING, win, 0, 0, mask);
            let _ = conn.free_pixmap(mask);
        }
        let name = b"Trascrittore";
        let _ = conn.change_property8(PropMode::REPLACE, win, AtomEnum::WM_NAME, AtomEnum::STRING, name);
        if let Some(a) = intern(&conn, "_NET_WM_WINDOW_TYPE") {
            if let Some(t) = intern(&conn, "_NET_WM_WINDOW_TYPE_NOTIFICATION") {
                let _ = conn.change_property32(PropMode::REPLACE, win, a, AtomEnum::ATOM, &[t]);
            }
        }
        let gc = conn.generate_id().map_err(|e| e.to_string())?;
        conn.create_gc(gc, win, &CreateGCAux::new()).map_err(|e| e.to_string())?;
        conn.map_window(win).map_err(|e| e.to_string())?;
        conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE)).map_err(|e| e.to_string())?;
        conn.flush().map_err(|e| e.to_string())?;
        Ok(Overlay { conn, win, gc, cmap, dist: r::distances(), px: vec![0; r::SIZE * r::SIZE] })
    }

    fn draw(&mut self, halo: f32, recording: bool, icon: Option<&[u32]>) -> Result<(), String> {
        r::render(&mut self.px, &self.dist, halo, recording, icon);
        let bytes: Vec<u8> = self.px.iter().flat_map(|p| p.to_le_bytes()).collect();
        let size = r::SIZE as u16;
        self.conn.put_image(ImageFormat::Z_PIXMAP, self.win, self.gc, size, size, 0, 0, 0, 32, &bytes).map_err(|e| e.to_string())?;
        // a ogni fotogramma resta sopra alle altre finestre
        self.conn.configure_window(self.win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE)).map_err(|e| e.to_string())?;
        self.conn.flush().map_err(|e| e.to_string())
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        let _ = self.conn.destroy_window(self.win);
        let _ = self.conn.free_gc(self.gc);
        let _ = self.conn.free_colormap(self.cmap);
        let _ = self.conn.flush();
    }
}

fn intern(conn: &RustConnection, name: &str) -> Option<Atom> {
    Some(conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom).filter(|a| *a != 0)
}

/// Area di lavoro (senza pannelli) della prima scrivania; se manca, tutto lo schermo.
fn work_area(conn: &RustConnection, root: Window, screen: &Screen) -> (i32, i32, i32, i32) {
    let full = (0, 0, screen.width_in_pixels as i32, screen.height_in_pixels as i32);
    let Some(a) = intern(conn, "_NET_WORKAREA") else { return full };
    let Some(reply) = conn.get_property(false, root, a, AtomEnum::CARDINAL, 0, 4).ok().and_then(|c| c.reply().ok()) else { return full };
    match reply.value32().map(|v| v.collect::<Vec<u32>>()) {
        Some(v) if v.len() >= 4 && v[2] > 0 && v[3] > 0 => (v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32),
        _ => full,
    }
}

/// Bitmap a 1 bit con il cerchio piu' grande che l'alone puo' raggiungere.
fn circle_mask(conn: &RustConnection, win: Window) -> Result<Pixmap, String> {
    let size = r::SIZE as u16;
    let pm = conn.generate_id().map_err(|e| e.to_string())?;
    conn.create_pixmap(1, pm, win, size, size).map_err(|e| e.to_string())?;
    let gc = conn.generate_id().map_err(|e| e.to_string())?;
    conn.create_gc(gc, pm, &CreateGCAux::new().foreground(0)).map_err(|e| e.to_string())?;
    conn.poly_fill_rectangle(pm, gc, &[Rectangle { x: 0, y: 0, width: size, height: size }]).map_err(|e| e.to_string())?;
    conn.change_gc(gc, &ChangeGCAux::new().foreground(1)).map_err(|e| e.to_string())?;
    let d = size - 2;
    conn.poly_fill_arc(pm, gc, &[Arc { x: 1, y: 1, width: d, height: d, angle1: 0, angle2: 360 * 64 }]).map_err(|e| e.to_string())?;
    conn.free_gc(gc).map_err(|e| e.to_string())?;
    Ok(pm)
}
