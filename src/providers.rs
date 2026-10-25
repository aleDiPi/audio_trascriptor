//! Servizi AI (Groq, OpenAI, Anthropic): ordine di priorita', passaggio automatico al successivo
//! quando uno esaurisce i crediti (o la chiave non vale piu') e stato da mostrare nelle Impostazioni.
//!
//! Nessuno dei tre espone via API il credito residuo di una chiave normale: lo stato mostra quindi
//! cio' che si sa davvero (chiave, esaurimento rilevato, chiamate e token dall'avvio) e il link
//! alla pagina del servizio dove il credito si legge.

use crate::usage::ApiError;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provider {
    Groq,
    OpenAi,
    Anthropic,
}

pub const ALL: [Provider; 3] = [Provider::Groq, Provider::OpenAi, Provider::Anthropic];

/// Pausa dopo un esaurimento crediti o una chiave rifiutata: si riprova dopo un'ora.
const PAUSE_LONG: u64 = 3600;

impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Provider::Groq => "groq",
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Groq => "Groq",
            Provider::OpenAi => "OpenAI",
            Provider::Anthropic => "Anthropic",
        }
    }

    /// Nome della variabile in .env che contiene la chiave.
    pub fn key_env(self) -> &'static str {
        match self {
            Provider::Groq => "GROQ_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::Anthropic => "ANTHROPIC_API_KEY",
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            Provider::Groq => "https://api.groq.com/openai/v1",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Anthropic => "https://api.anthropic.com/v1",
        }
    }

    /// Pagina dove si legge il credito residuo.
    pub fn billing_url(self) -> &'static str {
        match self {
            Provider::Groq => "https://console.groq.com/settings/billing",
            Provider::OpenAi => "https://platform.openai.com/settings/organization/billing/overview",
            Provider::Anthropic => "https://console.anthropic.com/settings/billing",
        }
    }

    /// Pagina dove si crea la chiave API.
    pub fn keys_url(self) -> &'static str {
        match self {
            Provider::Groq => "https://console.groq.com/keys",
            Provider::OpenAi => "https://platform.openai.com/api-keys",
            Provider::Anthropic => "https://console.anthropic.com/settings/keys",
        }
    }

    /// Anthropic non ha un servizio di trascrizione audio.
    pub fn supports_stt(self) -> bool {
        self != Provider::Anthropic
    }

    pub fn parse(s: &str) -> Option<Provider> {
        let s = s.trim().to_ascii_lowercase();
        ALL.into_iter().find(|p| p.id() == s)
    }
}

/// Ordine di priorita' da config (`providers`): nomi sconosciuti e doppioni ignorati.
/// Se non resta nulla si usa solo Groq.
pub fn parse_order(list: &[String]) -> Vec<Provider> {
    let mut out: Vec<Provider> = Vec::new();
    for s in list {
        if let Some(p) = Provider::parse(s) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    if out.is_empty() {
        out.push(Provider::Groq);
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Failure {
    /// Crediti o quota finiti: serve ricaricare, quindi pausa lunga
    OutOfCredit,
    /// Chiave rifiutata (401/403)
    BadKey,
    /// Limite di richieste al minuto/giorno
    RateLimited,
    /// Rete o errore del server
    Transient,
    Other,
}

/// Capisce dalla risposta d'errore perche' il servizio non ha risposto.
pub fn classify(e: &ApiError) -> Failure {
    let m = e.msg.to_ascii_lowercase();
    let credit_words = ["insufficient_quota", "exceeded your current quota", "credit balance is too low", "insufficient credit", "billing"];
    if e.status == 402 || (matches!(e.status, 400 | 403 | 429) && credit_words.iter().any(|w| m.contains(w))) {
        return Failure::OutOfCredit;
    }
    match e.status {
        401 | 403 => Failure::BadKey,
        429 => Failure::RateLimited,
        0 | 500..=599 => Failure::Transient,
        _ => Failure::Other,
    }
}

/// Secondi di pausa prima di riprovare il servizio dopo l'errore.
pub fn pause_secs(f: Failure, retry_after: Option<u64>) -> u64 {
    match f {
        Failure::OutOfCredit | Failure::BadKey => PAUSE_LONG,
        Failure::RateLimited => retry_after.unwrap_or(60),
        Failure::Transient => 30,
        Failure::Other => 0,
    }
}

#[derive(Default)]
pub struct State {
    /// (servizio, fino a quando e' in pausa, motivo)
    paused: Vec<(Provider, u64, Failure)>,
    /// (servizio, chiamate, token)
    used: Vec<(Provider, u32, u64)>,
}

impl State {
    pub fn pause(&mut self, p: Provider, until: u64, why: Failure) {
        self.paused.retain(|x| x.0 != p);
        self.paused.push((p, until, why));
    }

    pub fn paused(&self, p: Provider, now: u64) -> Option<(u64, Failure)> {
        self.paused.iter().find(|x| x.0 == p && x.1 > now).map(|x| (x.1 - now, x.2))
    }

    pub fn record(&mut self, p: Provider, tokens: u64) {
        match self.used.iter_mut().find(|x| x.0 == p) {
            Some(x) => {
                x.1 += 1;
                x.2 += tokens;
            }
            None => self.used.push((p, 1, tokens)),
        }
    }

    fn counters(&self, p: Provider) -> (u32, u64) {
        self.used.iter().find(|x| x.0 == p).map_or((0, 0), |x| (x.1, x.2))
    }
}

/// Prova i servizi in ordine: salta quelli senza chiave o in pausa, e dopo un errore mette in
/// pausa quello che ha fallito e passa al successivo. Se tutti sono in pausa riprova comunque.
/// Restituisce il risultato e il servizio che ha risposto.
pub fn run_chain<T>(
    order: &[Provider],
    usable: impl Fn(Provider) -> bool,
    state: &mut State,
    now: u64,
    mut call: impl FnMut(Provider) -> Result<T, ApiError>,
) -> Result<(T, Provider), String> {
    let with_key: Vec<Provider> = order.iter().copied().filter(|p| usable(*p)).collect();
    if with_key.is_empty() {
        return Err("nessun servizio AI con la chiave configurata (sezione API o Riserva AI)".into());
    }
    let mut candidates: Vec<Provider> = with_key.iter().copied().filter(|p| state.paused(*p, now).is_none()).collect();
    if candidates.is_empty() {
        candidates = with_key;
    }
    let mut errors: Vec<String> = Vec::new();
    for p in candidates {
        match call(p) {
            Ok(v) => return Ok((v, p)),
            Err(e) => {
                let why = classify(&e);
                let secs = pause_secs(why, e.retry_after);
                if secs > 0 {
                    state.pause(p, now + secs, why);
                }
                crate::config::log(&format!("{}: {} ({}): provo il servizio successivo", p.name(), e, describe(why)));
                errors.push(format!("{}: {}", p.name(), e));
            }
        }
    }
    Err(format!("nessun servizio AI ha risposto ({})", errors.join(" | ")))
}

pub fn describe(f: Failure) -> &'static str {
    match f {
        Failure::OutOfCredit => "crediti esauriti",
        Failure::BadKey => "chiave rifiutata",
        Failure::RateLimited => "limite di richieste",
        Failure::Transient => "servizio non raggiungibile",
        Failure::Other => "errore",
    }
}

static STATE: Mutex<State> = Mutex::new(State { paused: Vec::new(), used: Vec::new() });

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `run_chain` con lo stato globale e l'ora reale.
pub fn run<T>(
    order: &[Provider],
    usable: impl Fn(Provider) -> bool,
    call: impl FnMut(Provider) -> Result<T, ApiError>,
) -> Result<(T, Provider), String> {
    // il lock non resta preso durante le chiamate di rete: si copia lo stato e si riporta dopo
    let mut local = {
        let st = STATE.lock().unwrap();
        State { paused: st.paused.clone(), used: Vec::new() }
    };
    let r = run_chain(order, usable, &mut local, now(), call);
    STATE.lock().unwrap().paused = local.paused;
    r
}

pub fn record_use(p: Provider, tokens: u64) {
    STATE.lock().unwrap().record(p, tokens);
}

/// Testo per le Impostazioni: un blocco per servizio, nell'ordine di priorita'.
pub fn report(order: &[Provider], key: impl Fn(Provider) -> Option<String>) -> String {
    report_at(&STATE.lock().unwrap(), now(), order, key)
}

pub fn report_at(state: &State, now: u64, order: &[Provider], key: impl Fn(Provider) -> Option<String>) -> String {
    let mut s = String::from(
        "Ordine di priorita': se un servizio finisce i crediti o rifiuta la chiave passo in automatico al successivo e lo riprovo dopo un'ora.\n",
    );
    for p in ALL {
        let pos = order.iter().position(|x| *x == p);
        let k = key(p);
        s.push_str(&format!(
            "\n{}{}\n",
            p.name(),
            match pos {
                Some(i) => format!("  (priorita' {})", i + 1),
                None => "  (non in elenco: non viene usato)".to_string(),
            }
        ));
        let stato = match (&k, state.paused(p, now)) {
            (None, _) => "nessuna chiave".to_string(),
            (Some(_), Some((left, why))) => format!("IN PAUSA, {} (ancora {} min)", describe(why), left.div_ceil(60)),
            (Some(_), None) => "pronto".to_string(),
        };
        s.push_str(&format!(
            "  chiave: {}  |  stato: {}\n",
            k.map_or("-".to_string(), |k| crate::config::mask_key(&k)),
            stato
        ));
        let (calls, tokens) = state.counters(p);
        s.push_str(&format!("  dall'avvio: {calls} chiamate, {tokens} token\n"));
        s.push_str(&format!(
            "  credito residuo: {} non lo fornisce via API, si legge qui: {}\n",
            p.name(),
            p.billing_url()
        ));
        if !p.supports_stt() {
            s.push_str("  solo correzione del testo (non trascrive l'audio)\n");
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(status: u16, msg: &str) -> ApiError {
        ApiError { provider: "X", status, retry_after: None, msg: msg.into() }
    }

    #[test]
    fn order_parses_ignores_unknown_and_duplicates() {
        let v = |a: &[&str]| parse_order(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(v(&["OpenAI", " groq ", "openai", "boh"]), vec![Provider::OpenAi, Provider::Groq]);
        assert_eq!(v(&[]), vec![Provider::Groq]);
        assert_eq!(v(&["x"]), vec![Provider::Groq]);
    }

    #[test]
    fn classifies_credit_exhaustion_for_each_service() {
        let openai = r#"{"error":{"code":"insufficient_quota","message":"You exceeded your current quota"}}"#;
        let anthropic = r#"{"error":{"type":"invalid_request_error","message":"Your credit balance is too low to access the Anthropic API."}}"#;
        assert_eq!(classify(&err(429, openai)), Failure::OutOfCredit);
        assert_eq!(classify(&err(400, anthropic)), Failure::OutOfCredit);
        assert_eq!(classify(&err(402, "")), Failure::OutOfCredit);
    }

    #[test]
    fn classifies_other_failures() {
        assert_eq!(classify(&err(429, "rate limit reached")), Failure::RateLimited);
        assert_eq!(classify(&err(401, "bad key")), Failure::BadKey);
        assert_eq!(classify(&err(503, "")), Failure::Transient);
        assert_eq!(classify(&err(0, "rete")), Failure::Transient);
        assert_eq!(classify(&err(400, "bad request")), Failure::Other);
        assert_eq!(pause_secs(Failure::OutOfCredit, None), 3600);
        assert_eq!(pause_secs(Failure::RateLimited, Some(9)), 9);
        assert_eq!(pause_secs(Failure::RateLimited, None), 60);
        assert_eq!(pause_secs(Failure::Other, None), 0);
    }

    #[test]
    fn moves_to_next_service_when_credit_runs_out_and_remembers() {
        let order = [Provider::Groq, Provider::OpenAi, Provider::Anthropic];
        let mut st = State::default();
        let mut calls = Vec::new();
        let (v, used) = run_chain(&order, |_| true, &mut st, 1000, |p| {
            calls.push(p);
            if p == Provider::Groq {
                Err(err(429, "insufficient_quota"))
            } else {
                Ok(5)
            }
        })
        .unwrap();
        assert_eq!((v, used), (5, Provider::OpenAi));
        assert_eq!(calls, vec![Provider::Groq, Provider::OpenAi]);
        assert!(st.paused(Provider::Groq, 1000).is_some());
        // la volta dopo Groq non viene nemmeno chiamato
        calls.clear();
        let _ = run_chain(&order, |_| true, &mut st, 1100, |p| {
            calls.push(p);
            Ok(())
        });
        assert_eq!(calls, vec![Provider::OpenAi]);
        // passata l'ora Groq torna disponibile
        calls.clear();
        let _ = run_chain(&order, |_| true, &mut st, 1000 + 3600, |p| {
            calls.push(p);
            Ok(())
        });
        assert_eq!(calls, vec![Provider::Groq]);
    }

    #[test]
    fn skips_services_without_key_and_reports_when_none_usable() {
        let order = [Provider::Groq, Provider::Anthropic];
        let mut st = State::default();
        let (_, used) = run_chain(&order, |p| p == Provider::Anthropic, &mut st, 0, |_| Ok(())).unwrap();
        assert_eq!(used, Provider::Anthropic);
        let r: Result<((), Provider), String> = run_chain(&order, |_| false, &mut st, 0, |_| Ok(()));
        assert!(r.unwrap_err().contains("nessun servizio AI con la chiave"));
    }

    #[test]
    fn all_failing_lists_every_error_and_all_paused_still_retries() {
        let order = [Provider::Groq, Provider::OpenAi];
        let mut st = State::default();
        let r: Result<((), Provider), String> = run_chain(&order, |_| true, &mut st, 0, |_| Err(err(503, "giu'")));
        let e = r.unwrap_err();
        assert!(e.contains("Groq") && e.contains("OpenAI"));
        // ora entrambi sono in pausa (30 s): riprova comunque invece di arrendersi
        let mut n = 0;
        let r = run_chain(&order, |_| true, &mut st, 1, |_| {
            n += 1;
            Ok(())
        });
        assert!(r.is_ok());
        assert_eq!(n, 1);
    }

    #[test]
    fn report_shows_order_state_and_billing_link() {
        let order = [Provider::OpenAi, Provider::Groq];
        let mut st = State::default();
        st.record(Provider::OpenAi, 120);
        st.pause(Provider::Groq, 5000, Failure::OutOfCredit);
        let key = |p: Provider| if p == Provider::Anthropic { None } else { Some(format!("k-{}-abcd", p.id())) };
        let r = report_at(&st, 4000, &order, key);
        assert!(r.contains("OpenAI  (priorita' 1)"));
        assert!(r.contains("Groq  (priorita' 2)"));
        assert!(r.contains("Anthropic  (non in elenco"));
        assert!(r.contains("IN PAUSA, crediti esauriti (ancora 17 min)"));
        assert!(r.contains("1 chiamate, 120 token"));
        assert!(r.contains("nessuna chiave"));
        assert!(r.contains("https://platform.openai.com/settings/organization/billing"));
    }

    #[test]
    fn only_anthropic_cannot_transcribe() {
        assert!(Provider::Groq.supports_stt() && Provider::OpenAi.supports_stt() && !Provider::Anthropic.supports_stt());
    }
}
