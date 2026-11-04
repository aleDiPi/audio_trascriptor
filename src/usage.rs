//! Utilizzo di Groq: contatori locali, limiti letti dagli header delle risposte e
//! passaggio automatico ai modelli di riserva quando si raggiunge il limite gratuito (HTTP 429).
//! Groq non espone un credito residuo: il piano gratuito ha solo limiti di richieste e token.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct ApiError {
    /// Nome del servizio ("Groq", "OpenAI", "Anthropic")
    pub provider: &'static str,
    /// 0 = errore di rete
    pub status: u16,
    pub retry_after: Option<u64>,
    pub msg: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.status == 0 {
            write!(f, "{}", crate::i18n::tf("err.network", &[("msg", &self.msg)]))
        } else {
            write!(f, "{} HTTP {}: {}", self.provider, self.status, self.msg)
        }
    }
}

#[derive(Default, Clone)]
struct Snapshot {
    model: String,
    calls: u32,
    tokens: u64,
    limit_requests: Option<String>,
    remaining_requests: Option<String>,
    reset_requests: Option<String>,
    limit_tokens: Option<String>,
    remaining_tokens: Option<String>,
    reset_tokens: Option<String>,
    limited_until: u64,
    last_seq: u64,
}

static STATE: Mutex<Vec<Snapshot>> = Mutex::new(Vec::new());
static SEQ: AtomicU64 = AtomicU64::new(1);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn with_entry<R>(model: &str, f: impl FnOnce(&mut Snapshot) -> R) -> R {
    let mut st = STATE.lock().unwrap();
    if let Some(i) = st.iter().position(|s| s.model == model) {
        return f(&mut st[i]);
    }
    st.push(Snapshot { model: model.to_string(), ..Default::default() });
    f(st.last_mut().unwrap())
}

/// Registra una risposta di Groq: contatori locali e header `x-ratelimit-*`.
pub fn record(model: &str, headers: &[(String, String)], tokens: u64) {
    if model.is_empty() {
        return;
    }
    let get = |name: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone());
    with_entry(model, |s| {
        s.calls += 1;
        s.tokens += tokens;
        s.last_seq = SEQ.fetch_add(1, Ordering::Relaxed);
        s.limit_requests = get("x-ratelimit-limit-requests").or(s.limit_requests.take());
        s.remaining_requests = get("x-ratelimit-remaining-requests").or(s.remaining_requests.take());
        s.reset_requests = get("x-ratelimit-reset-requests").or(s.reset_requests.take());
        s.limit_tokens = get("x-ratelimit-limit-tokens").or(s.limit_tokens.take());
        s.remaining_tokens = get("x-ratelimit-remaining-tokens").or(s.remaining_tokens.take());
        s.reset_tokens = get("x-ratelimit-reset-tokens").or(s.reset_tokens.take());
    });
}

/// Secondi d'attesa dall'header `retry-after` (arrotondati per eccesso).
pub fn parse_retry_after(headers: &[(String, String)]) -> Option<u64> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
        .and_then(|(_, v)| v.trim().parse::<f64>().ok())
        .filter(|s| *s >= 0.0)
        .map(|s| s.ceil() as u64)
}

/// Token totali dichiarati nel corpo della risposta (`usage.total_tokens`), 0 se assenti.
/// Anthropic non ha `total_tokens`: si sommano `input_tokens` e `output_tokens`.
pub fn total_tokens(body: &str) -> u64 {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return 0 };
    let u = &v["usage"];
    u["total_tokens"]
        .as_u64()
        .unwrap_or_else(|| u["input_tokens"].as_u64().unwrap_or(0) + u["output_tokens"].as_u64().unwrap_or(0))
}

pub fn mark_limited(model: &str, secs: u64) {
    with_entry(model, |s| s.limited_until = now() + secs);
}

pub fn is_limited(model: &str) -> bool {
    with_entry(model, |s| s.limited_until > now())
}

/// Prova i modelli in ordine, saltando quelli in pausa per limite. Su 429 segna il modello come
/// limitato e passa al successivo; qualsiasi altro errore si ferma subito.
/// Restituisce il risultato e il modello che ha risposto. Se tutti i modelli sono al limite
/// l'errore e' un 429, cosi' chi sta sopra puo' passare a un altro servizio.
pub fn with_fallback<T>(
    models: &[String],
    mut call: impl FnMut(&str) -> Result<T, ApiError>,
) -> Result<(T, String), ApiError> {
    let mut candidates: Vec<&String> = models.iter().filter(|m| !is_limited(m)).collect();
    crate::config::debug(|| format!("candidate models: {candidates:?} (paused by limit: {})", models.len() - candidates.len()));
    if candidates.is_empty() {
        candidates = models.iter().collect(); // tutti in pausa: riprova comunque
    }
    let mut last = crate::i18n::t("err.no_model_configured");
    for m in candidates {
        match call(m) {
            Ok(v) => return Ok((v, m.clone())),
            Err(e) if e.status == 429 => {
                mark_limited(m, e.retry_after.unwrap_or(60));
                crate::config::log(&format!("Groq limit reached on {m}: moving to the next model (pause {} s)", e.retry_after.unwrap_or(60)));
                last = format!("{m}: {e}");
            }
            Err(e) => return Err(e),
        }
    }
    Err(ApiError {
        provider: "Groq",
        status: 429,
        retry_after: None,
        msg: crate::i18n::tf("err.all_models_limited", &[("last", &last)]),
    })
}

fn dash(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("-")
}

pub fn report() -> String {
    report_at(now())
}

pub fn report_at(now: u64) -> String {
    let st = STATE.lock().unwrap();
    let mut s = crate::i18n::t("usage.intro");
    let used: Vec<&Snapshot> = st.iter().filter(|x| x.calls > 0 || x.limited_until > now).collect();
    if used.is_empty() {
        s.push_str(&format!("\n{}", crate::i18n::t("usage.none_yet")));
        return s;
    }
    let newest = used.iter().map(|x| x.last_seq).max().unwrap_or(0);
    for x in used {
        s.push('\n');
        s.push_str(&crate::i18n::tf(
            "usage.model_block",
            &[
                ("model", &x.model),
                ("last", &if x.last_seq == newest && newest > 0 { crate::i18n::t("usage.last_used") } else { String::new() }),
                ("calls", &x.calls.to_string()),
                ("tokens", &x.tokens.to_string()),
                ("req_left", dash(&x.remaining_requests)),
                ("req_limit", dash(&x.limit_requests)),
                ("req_reset", dash(&x.reset_requests)),
                ("tok_left", dash(&x.remaining_tokens)),
                ("tok_limit", dash(&x.limit_tokens)),
                ("tok_reset", dash(&x.reset_tokens)),
            ],
        ));
        if x.limited_until > now {
            s.push_str(&crate::i18n::tf("usage.limit_hit", &[("secs", &(x.limited_until - now).to_string())]));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(status: u16, retry: Option<u64>) -> ApiError {
        ApiError { provider: "Groq", status, retry_after: retry, msg: "x".into() }
    }

    #[test]
    fn falls_back_on_429_and_remembers_the_pause() {
        let models = vec!["t1-a".to_string(), "t1-b".to_string()];
        let (v, used) = with_fallback(&models, |m| if m == "t1-a" { Err(err(429, Some(30))) } else { Ok(7) }).unwrap();
        assert_eq!((v, used.as_str()), (7, "t1-b"));
        assert!(is_limited("t1-a") && !is_limited("t1-b"));
        // la volta dopo il primo modello viene saltato senza essere chiamato
        let mut tried = Vec::new();
        let _ = with_fallback(&models, |m| {
            tried.push(m.to_string());
            Ok(())
        });
        assert_eq!(tried, vec!["t1-b"]);
    }

    #[test]
    fn other_errors_do_not_cascade() {
        let models = vec!["t2-a".to_string(), "t2-b".to_string()];
        let mut calls = 0;
        let r: Result<(u8, String), ApiError> = with_fallback(&models, |_| {
            calls += 1;
            Err(err(401, None))
        });
        assert!(r.unwrap_err().to_string().contains("401"));
        assert_eq!(calls, 1);
    }

    #[test]
    fn all_limited_reports_error() {
        let models = vec!["t3-a".to_string()];
        let r: Result<(u8, String), ApiError> = with_fallback(&models, |_| Err(err(429, None)));
        let e = r.unwrap_err();
        assert_eq!(e.status, 429);
        assert!(e.to_string().contains("limit reached on all models"));
        assert!(is_limited("t3-a"));
    }

    #[test]
    fn records_headers_and_reports() {
        let h = vec![
            ("x-ratelimit-limit-requests".to_string(), "14400".to_string()),
            ("X-RateLimit-Remaining-Requests".to_string(), "14399".to_string()),
            ("x-ratelimit-remaining-tokens".to_string(), "7900".to_string()),
        ];
        record("t4-model", &h, 120);
        record("t4-model", &[], 30); // senza header: i valori precedenti restano
        let r = report_at(now());
        assert!(r.contains("t4-model"));
        assert!(r.contains("14399 left of 14400"));
        assert!(r.contains("2 calls, 150 tokens"));
    }

    #[test]
    fn retry_after_rounds_up_and_ignores_garbage() {
        let h = |v: &str| vec![("Retry-After".to_string(), v.to_string())];
        assert_eq!(parse_retry_after(&h("7")), Some(7));
        assert_eq!(parse_retry_after(&h(" 2.1 ")), Some(3));
        assert_eq!(parse_retry_after(&h("abc")), None);
        assert_eq!(parse_retry_after(&h("-5")), None);
        assert_eq!(parse_retry_after(&[]), None);
    }

    #[test]
    fn total_tokens_from_body() {
        assert_eq!(total_tokens(r#"{"usage":{"total_tokens":321}}"#), 321);
        assert_eq!(total_tokens(r#"{"text":"ciao"}"#), 0);
        assert_eq!(total_tokens("non json"), 0);
        assert_eq!(total_tokens(r#"{"usage":{"input_tokens":100,"output_tokens":20}}"#), 120);
    }
}
