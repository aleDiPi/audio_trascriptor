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
    /// Whisper sul PC (solo trascrizione): ultima risorsa quando i servizi in rete non rispondono
    Local,
}

pub const ALL: [Provider; 4] = [Provider::Groq, Provider::OpenAi, Provider::Anthropic, Provider::Local];

/// Pausa dopo un esaurimento crediti o una chiave rifiutata: si riprova dopo un'ora.
const PAUSE_LONG: u64 = 3600;

impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Provider::Groq => "groq",
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Local => "local",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Groq => "Groq",
            Provider::OpenAi => "OpenAI",
            Provider::Anthropic => "Anthropic",
            Provider::Local => "Locale (Whisper)",
        }
    }

    /// Nome della variabile in .env che contiene la chiave.
    pub fn key_env(self) -> &'static str {
        match self {
            Provider::Groq => "GROQ_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::Local => "",
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            Provider::Groq => "https://api.groq.com/openai/v1",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Local => "",
        }
    }

    /// Pagina dove si legge il credito residuo.
    pub fn billing_url(self) -> &'static str {
        match self {
            Provider::Groq => "https://console.groq.com/settings/billing",
            Provider::OpenAi => "https://platform.openai.com/settings/organization/billing/overview",
            Provider::Anthropic => "https://console.anthropic.com/settings/billing",
            Provider::Local => "",
        }
    }

    /// Pagina dove si crea la chiave API.
    pub fn keys_url(self) -> &'static str {
        match self {
            Provider::Groq => "https://console.groq.com/keys",
            Provider::OpenAi => "https://platform.openai.com/api-keys",
            Provider::Anthropic => "https://console.anthropic.com/settings/keys",
            Provider::Local => "",
        }
    }

    /// Anthropic non ha un servizio di trascrizione audio.
    pub fn supports_stt(self) -> bool {
        self != Provider::Anthropic
    }

    /// Whisper locale trascrive soltanto: non corregge il testo.
    pub fn supports_llm(self) -> bool {
        self != Provider::Local
    }

    /// Servizio che gira sul PC: niente chiave API, niente credito.
    pub fn is_local(self) -> bool {
        self == Provider::Local
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
        return Err("nessun servizio AI pronto (manca la chiave o il modello locale: sezione AI delle Impostazioni)".into());
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

/// Testo riassuntivo: un blocco per servizio, nell'ordine di priorita'.
#[cfg(test)]
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

/// Tutti e tre i servizi per le tessere delle Impostazioni: prima quelli in uso (nell'ordine di
/// priorita'), poi gli altri.
pub fn tile_order(enabled: &[Provider]) -> Vec<Provider> {
    let mut v = enabled.to_vec();
    v.extend(ALL.into_iter().filter(|p| !enabled.contains(p)));
    v
}

/// Sposta l'elemento `from` nella posizione `to` (trascinamento di una tessera).
pub fn move_item<T>(v: &mut Vec<T>, from: usize, to: usize) {
    if from < v.len() && to < v.len() && from != to {
        let x = v.remove(from);
        v.insert(to, x);
    }
}

/// Primo servizio in uso che e' pronto (chiave o modello locale) e sa fare il lavoro richiesto
/// (trascrivere o correggere il testo): e' quello che risponde.
pub fn first_active(order: &[Provider], ready: impl Fn(Provider) -> bool, need_stt: bool) -> Option<Provider> {
    order.iter().copied().find(|p| ready(*p) && if need_stt { p.supports_stt() } else { p.supports_llm() })
}

/// Servizi che possono trascrivere l'audio, nell'ordine di priorita'.
pub fn stt_order(order: &[Provider]) -> Vec<Provider> {
    order.iter().copied().filter(|p| p.supports_stt()).collect()
}

/// Servizi che possono correggere il testo, nell'ordine di priorita'.
pub fn llm_order(order: &[Provider]) -> Vec<Provider> {
    order.iter().copied().filter(|p| p.supports_llm()).collect()
}

impl State {
    /// Stato breve di un servizio per la sua tessera: (pronto, testo).
    pub fn status_at(&self, p: Provider, has_key: bool, now: u64) -> (bool, String) {
        match (has_key, self.paused(p, now)) {
            (false, _) => (false, if p.is_local() { "Modello non scaricato" } else { "Nessuna chiave" }.to_string()),
            (true, Some((left, why))) => (false, format!("In pausa: {} ({} min)", describe(why), left.div_ceil(60))),
            (true, None) => (true, "Pronto".to_string()),
        }
    }

    /// "dall'avvio: 3 chiamate, 450 token"
    pub fn usage_line(&self, p: Provider) -> String {
        let (calls, tokens) = self.counters(p);
        if p.is_local() {
            return format!("Dall'avvio: {calls} trascrizioni.");
        }
        format!("Dall'avvio: {calls} chiamate, {tokens} token. Il credito residuo non e' leggibile via API.")
    }
}

pub fn status(p: Provider, has_key: bool) -> (bool, String) {
    STATE.lock().unwrap().status_at(p, has_key, now())
}

pub fn usage_line(p: Provider) -> String {
    STATE.lock().unwrap().usage_line(p)
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
        assert!(r.unwrap_err().contains("nessun servizio AI pronto"));
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
    fn only_anthropic_cannot_transcribe_and_only_local_cannot_refine() {
        assert!(Provider::Groq.supports_stt() && Provider::OpenAi.supports_stt() && !Provider::Anthropic.supports_stt());
        assert!(Provider::Local.supports_stt() && !Provider::Local.supports_llm());
        assert!(Provider::Groq.supports_llm() && Provider::Anthropic.supports_llm());
    }

    #[test]
    fn stt_and_llm_orders_keep_priority_and_drop_unsuitable_services() {
        let order = [Provider::Anthropic, Provider::Groq, Provider::Local];
        assert_eq!(stt_order(&order), vec![Provider::Groq, Provider::Local]);
        assert_eq!(llm_order(&order), vec![Provider::Anthropic, Provider::Groq]);
    }

    #[test]
    fn local_is_parsed_and_listed() {
        let v = |a: &[&str]| parse_order(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(v(&["groq", "local"]), vec![Provider::Groq, Provider::Local]);
        assert_eq!(Provider::parse("LOCAL"), Some(Provider::Local));
        assert_eq!(tile_order(&[Provider::Groq]).last(), Some(&Provider::Local));
    }

    #[test]
    fn local_is_used_only_when_every_cloud_service_fails_or_is_paused() {
        let order = [Provider::Groq, Provider::OpenAi, Provider::Local];
        // tutto bene in rete: il locale non viene mai chiamato
        let mut st = State::default();
        let mut calls = Vec::new();
        let (_, used) = run_chain(&order, |_| true, &mut st, 0, |p| {
            calls.push(p);
            Ok(())
        })
        .unwrap();
        assert_eq!((used, calls.clone()), (Provider::Groq, vec![Provider::Groq]));
        // Groq e OpenAI senza crediti: si arriva al locale, nello stesso tentativo
        let mut st = State::default();
        let mut calls = Vec::new();
        let (_, used) = run_chain(&order, |_| true, &mut st, 1000, |p| {
            calls.push(p);
            if p == Provider::Local {
                Ok(())
            } else {
                Err(err(429, "insufficient_quota"))
            }
        })
        .unwrap();
        assert_eq!(used, Provider::Local);
        assert_eq!(calls, vec![Provider::Groq, Provider::OpenAi, Provider::Local]);
        // dopo, i servizi in pausa vengono saltati e si va dritti al locale
        calls.clear();
        let _ = run_chain(&order, |_| true, &mut st, 1100, |p| {
            calls.push(p);
            Ok(())
        });
        assert_eq!(calls, vec![Provider::Local]);
        // senza modello scaricato il locale non e' utilizzabile e non viene provato
        calls.clear();
        let r = run_chain(&order, |p| p != Provider::Local, &mut State::default(), 0, |p| {
            calls.push(p);
            Err::<(), _>(err(503, "giu'"))
        });
        assert!(r.is_err());
        assert!(!calls.contains(&Provider::Local));
    }

    #[test]
    fn first_active_skips_local_for_text_correction() {
        let order = [Provider::Local, Provider::Groq];
        assert_eq!(first_active(&order, |_| true, true), Some(Provider::Local));
        assert_eq!(first_active(&order, |_| true, false), Some(Provider::Groq));
        assert_eq!(first_active(&order, |p| p == Provider::Local, false), None);
    }

    #[test]
    fn local_status_asks_for_the_model() {
        let st = State::default();
        assert_eq!(st.status_at(Provider::Local, false, 0).1, "Modello non scaricato");
        assert_eq!(st.status_at(Provider::Local, true, 0), (true, "Pronto".to_string()));
        assert_eq!(st.usage_line(Provider::Local), "Dall'avvio: 0 trascrizioni.");
    }

    #[test]
    fn tile_order_puts_enabled_first_then_the_rest() {
        assert_eq!(tile_order(&[Provider::OpenAi, Provider::Groq]), vec![Provider::OpenAi, Provider::Groq, Provider::Anthropic, Provider::Local]);
        assert_eq!(tile_order(&[Provider::Groq]), vec![Provider::Groq, Provider::OpenAi, Provider::Anthropic, Provider::Local]);
    }

    #[test]
    fn move_item_reorders_and_ignores_bad_indexes() {
        let mut v = vec!['a', 'b', 'c'];
        move_item(&mut v, 0, 2);
        assert_eq!(v, vec!['b', 'c', 'a']);
        move_item(&mut v, 2, 0);
        assert_eq!(v, vec!['a', 'b', 'c']);
        move_item(&mut v, 1, 9);
        move_item(&mut v, 7, 0);
        assert_eq!(v, vec!['a', 'b', 'c']);
    }

    #[test]
    fn first_active_skips_missing_keys_and_non_stt() {
        let order = [Provider::Anthropic, Provider::OpenAi, Provider::Groq];
        let keys = |p: Provider| p != Provider::OpenAi;
        assert_eq!(first_active(&order, keys, false), Some(Provider::Anthropic));
        assert_eq!(first_active(&order, keys, true), Some(Provider::Groq));
        assert_eq!(first_active(&order, |_| false, false), None);
    }

    #[test]
    fn tile_status_and_usage_line() {
        let mut st = State::default();
        assert_eq!(st.status_at(Provider::Groq, false, 0), (false, "Nessuna chiave".to_string()));
        assert_eq!(st.status_at(Provider::Groq, true, 0), (true, "Pronto".to_string()));
        st.pause(Provider::Groq, 5000, Failure::OutOfCredit);
        assert_eq!(st.status_at(Provider::Groq, true, 4000), (false, "In pausa: crediti esauriti (17 min)".to_string()));
        st.record(Provider::Groq, 40);
        assert!(st.usage_line(Provider::Groq).starts_with("Dall'avvio: 1 chiamate, 40 token"));
    }
}
