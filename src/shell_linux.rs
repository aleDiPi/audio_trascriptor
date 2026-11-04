//! Icona nella area di notifica (AppIndicator/StatusNotifier, come in Cinnamon) e ciclo GTK del thread principale.
//! Il ciclo dorme finche' non arriva un evento: nessuna CPU a riposo.

use crate::app::{icon, idle_tip};
use crate::{config, gui, platform, Event, Ui};
use std::cell::RefCell;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

struct Tray {
    tray: TrayIcon,
    ui_rx: Receiver<(Ui, String)>,
    ev_tx: Sender<Event>,
    m_auto: CheckMenuItem,
    m_dbg: CheckMenuItem,
    ids: Ids,
}

struct Ids {
    set: MenuId,
    auto: MenuId,
    dbg: MenuId,
    log: MenuId,
    cfg: MenuId,
    reload: MenuId,
    quit: MenuId,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// Funzione che sveglia il ciclo principale da un altro thread: esegue l'aggiornamento dell'icona sul thread GTK.
pub fn waker() -> Arc<dyn Fn() + Send + Sync> {
    Arc::new(|| {
        gtk::glib::MainContext::default().invoke(|| TRAY.with(|t| {
            if let Some(t) = t.borrow().as_ref() {
                refresh(t);
            }
        }));
    })
}

fn refresh(t: &Tray) {
    while let Ok((state, text)) = t.ui_rx.try_recv() {
        let _ = t.tray.set_icon(Some(icon(state)));
        let tip = if state == Ui::Idle { idle_tip(&config::load().hotkey) } else { text };
        t.tray.set_title(Some(tip.clone())); // AppIndicator non ha tooltip: il titolo compare nel menu/etichetta
        let _ = t.tray.set_tooltip(Some(tip));
    }
}

fn on_menu(id: &MenuId) {
    TRAY.with(|t| {
        let t = t.borrow();
        let Some(t) = t.as_ref() else { return };
        let i = &t.ids;
        if *id == i.set {
            gui::show();
        } else if *id == i.quit {
            gtk::main_quit();
        } else if *id == i.auto {
            platform::set_autostart(t.m_auto.is_checked());
        } else if *id == i.dbg {
            config::set_debug(t.m_dbg.is_checked());
        } else if *id == i.log {
            config::open_log();
        } else if *id == i.cfg {
            platform::open(config::dir());
        } else if *id == i.reload {
            let _ = t.ev_tx.send(Event::Reload);
        }
    });
}

pub fn run(cfg: &config::Config, ev_tx: Sender<Event>, ui_rx: Receiver<(Ui, String)>) {
    if let Err(e) = gtk::init() {
        config::log(&format!("impossibile avviare GTK (serve una sessione grafica): {e}"));
        return;
    }
    let m_auto = CheckMenuItem::new("Avvia all'accesso", true, platform::autostart_enabled(), None);
    let m_set = MenuItem::new("Impostazioni...", true, None);
    let m_cfg = MenuItem::new("Apri cartella configurazione", true, None);
    let m_dbg = CheckMenuItem::new("Modalita' debug (log dettagliato)", true, cfg.debug, None);
    let m_log = MenuItem::new("Apri log", true, None);
    let m_reload = MenuItem::new("Ricarica configurazione", true, None);
    let m_quit = MenuItem::new("Esci", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[&m_set, &m_auto, &m_dbg, &m_log, &m_cfg, &m_reload, &PredefinedMenuItem::separator(), &m_quit]);
    let tray = match TrayIconBuilder::new().with_menu(Box::new(menu)).with_title(idle_tip(&cfg.hotkey)).with_icon(icon(Ui::Idle)).build() {
        Ok(t) => t,
        Err(e) => {
            config::log(&format!("impossibile creare l'icona nell'area di notifica: {e}"));
            return;
        }
    };
    let ids = Ids {
        set: m_set.id().clone(),
        auto: m_auto.id().clone(),
        dbg: m_dbg.id().clone(),
        log: m_log.id().clone(),
        cfg: m_cfg.id().clone(),
        reload: m_reload.id().clone(),
        quit: m_quit.id().clone(),
    };
    TRAY.with(|t| *t.borrow_mut() = Some(Tray { tray, ui_rx, ev_tx, m_auto, m_dbg, ids }));
    // gli eventi del menu arrivano dal ciclo GTK; la gestione torna comunque sul thread principale
    MenuEvent::set_event_handler(Some(|e: MenuEvent| {
        let id = e.id;
        gtk::glib::MainContext::default().invoke(move || on_menu(&id));
    }));

    // avviato dal menu delle applicazioni (non dall'avvio automatico): apre subito le Impostazioni
    if !platform::quiet_start() {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(800));
            gui::show();
        });
    }
    gtk::main();
}
