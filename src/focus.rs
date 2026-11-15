//! Finestra di destinazione dell'incolla: si ricorda quella in primo piano quando parte la registrazione e,
//! al momento di incollare, si controlla che sia ancora lei. Logica indipendente dal sistema operativo
//! (le funzioni di sistema arrivano come parametri), cosi' si puo' provare con i test.

/// Identificatore della finestra (HWND su Windows, id X11 su Linux). 0 = sconosciuta.
pub type Win = u64;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Outcome {
    /// La finestra giusta e' gia' in primo piano (o la destinazione e' sconosciuta): si incolla.
    Ready,
    /// Il focus era altrove ed e' stato riportato sulla finestra iniziale: si incolla.
    Restored,
    /// La finestra non esiste piu' o non si riesce a riportarla in primo piano: NON si incolla.
    Lost,
}

/// Quante verifiche (a distanza di `pause`) dopo aver chiesto il cambio di finestra.
const CHECKS: u32 = 10;

pub fn ensure(
    target: Win,
    current: impl Fn() -> Win,
    exists: impl Fn(Win) -> bool,
    activate: impl Fn(Win),
    pause: impl Fn(),
) -> Outcome {
    if target == 0 || current() == target {
        return Outcome::Ready;
    }
    if !exists(target) {
        return Outcome::Lost;
    }
    activate(target);
    for _ in 0..CHECKS {
        pause();
        if current() == target {
            return Outcome::Restored;
        }
    }
    Outcome::Lost
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn same_window_pastes_without_touching_focus() {
        let r = ensure(7, || 7, |_| panic!("exists"), |_| panic!("activate"), || {});
        assert_eq!(r, Outcome::Ready);
    }

    #[test]
    fn unknown_target_keeps_old_behaviour() {
        assert_eq!(ensure(0, || 9, |_| false, |_| panic!("activate"), || {}), Outcome::Ready);
    }

    #[test]
    fn closed_window_is_lost_and_never_activated() {
        assert_eq!(ensure(7, || 9, |_| false, |_| panic!("activate"), || {}), Outcome::Lost);
    }

    #[test]
    fn focus_is_restored_when_window_exists() {
        let cur = Cell::new(9u64);
        let r = ensure(7, || cur.get(), |_| true, |w| cur.set(w), || {});
        assert_eq!(r, Outcome::Restored);
    }

    #[test]
    fn slow_window_manager_is_waited_for() {
        let cur = Cell::new(9u64);
        let calls = Cell::new(0);
        let r = ensure(7, || cur.get(), |_| true, |_| {}, || {
            calls.set(calls.get() + 1);
            if calls.get() == 4 {
                cur.set(7);
            }
        });
        assert_eq!(r, Outcome::Restored);
    }

    #[test]
    fn refused_activation_is_lost() {
        let calls = Cell::new(0);
        let r = ensure(7, || 9, |_| true, |_| {}, || calls.set(calls.get() + 1));
        assert_eq!(r, Outcome::Lost);
        assert_eq!(calls.get(), CHECKS);
    }
}
