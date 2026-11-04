//! Icona nella tray e ciclo dei messaggi di Windows (thread principale).

use crate::app::{icon, idle_tip};
use crate::{config, gui, platform, Event, Ui};
use std::ptr::null_mut;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Funzione che sveglia il ciclo principale (da chiamare dal thread principale, prima di `run`).
pub fn waker() -> Arc<dyn Fn() + Send + Sync> {
    let main_tid = unsafe { GetCurrentThreadId() };
    Arc::new(move || unsafe {
        PostThreadMessageW(main_tid, WM_APP, 0, 0);
    })
}

pub fn run(cfg: &config::Config, ev_tx: Sender<Event>, ui_rx: Receiver<(Ui, String)>) {
    let m_auto = CheckMenuItem::new("Avvia con Windows", true, platform::autostart_enabled(), None);
    let m_set = MenuItem::new("Impostazioni...", true, None);
    let m_cfg = MenuItem::new("Apri cartella configurazione", true, None);
    let m_dbg = CheckMenuItem::new("Modalita' debug (log dettagliato)", true, cfg.debug, None);
    let m_log = MenuItem::new("Apri log", true, None);
    let m_reload = MenuItem::new("Ricarica configurazione", true, None);
    let m_quit = MenuItem::new("Esci", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[&m_set, &m_auto, &m_dbg, &m_log, &m_cfg, &m_reload, &PredefinedMenuItem::separator(), &m_quit]);
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(idle_tip(&cfg.hotkey))
        .with_icon(icon(Ui::Idle))
        .build()
        .expect("impossibile creare l'icona nella tray");

    let menu_rx = MenuEvent::receiver();
    let tray_rx = TrayIconEvent::receiver();
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let r = unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) };
        if r <= 0 {
            break;
        }
        if msg.hwnd.is_null() && msg.message == WM_APP {
            while let Ok((state, text)) = ui_rx.try_recv() {
                let _ = tray.set_icon(Some(icon(state)));
                let tip = if state == Ui::Idle { idle_tip(&config::load().hotkey) } else { text };
                let _ = tray.set_tooltip(Some(tip));
            }
            continue;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        while let Ok(e) = tray_rx.try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                gui::show();
            }
        }
        while let Ok(e) = menu_rx.try_recv() {
            if e.id == *m_set.id() {
                gui::show();
            } else if e.id == *m_quit.id() {
                unsafe { PostQuitMessage(0) };
            } else if e.id == *m_auto.id() {
                platform::set_autostart(m_auto.is_checked());
            } else if e.id == *m_dbg.id() {
                config::set_debug(m_dbg.is_checked());
            } else if e.id == *m_log.id() {
                config::open_log();
            } else if e.id == *m_cfg.id() {
                platform::open(config::dir());
            } else if e.id == *m_reload.id() {
                let _ = ev_tx.send(Event::Reload);
            }
        }
    }
}
