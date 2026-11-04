//! Su Linux non c'e' il bollino a schermo durante la dettatura: lo stato si vede nell'icona della tray
//! (e si sente dai suoni). Le funzioni restano per avere la stessa interfaccia di Windows.

pub fn start() {}

pub fn show_recording(_icon: Option<isize>) {}

pub fn show_processing() {}

pub fn hide() {}
