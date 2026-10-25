//! Client Anthropic (Messages API): rifinitura del testo e prova della chiave.
//! Anthropic non ha un servizio di trascrizione audio, quindi qui c'e' solo l'AI di correzione.

use crate::providers::Provider;
use crate::usage::{self, ApiError};
use reqwest::blocking::{Client, RequestBuilder};
use std::time::Duration;

const VERSION: &str = "2023-06-01";
const MAX_TOKENS: u32 = 2048;

pub struct Anthropic {
    client: Client,
    key: String,
}

/// Corpo della richiesta `POST /v1/messages`.
pub fn request_body(model: &str, system: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": system,
        "messages": [
            {"role": "user", "content": format!("<trascrizione>{text}</trascrizione>")}
        ]
    })
}

/// Testo della risposta: unisce i blocchi `text` del campo `content`.
pub fn parse_reply(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let text: String = v["content"]
        .as_array()?
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

impl Anthropic {
    pub fn new(key: String) -> Result<Self, String> {
        let client = Client::builder().timeout(Duration::from_secs(60)).build().map_err(|e| e.to_string())?;
        Ok(Anthropic { client, key })
    }

    pub fn refine(&self, model: &str, system: &str, text: &str) -> Result<String, ApiError> {
        let body = request_body(model, system, text);
        let url = format!("{}/messages", Provider::Anthropic.base_url());
        let resp = self.send(model, self.client.post(url).header("Content-Type", "application/json").body(body.to_string()))?;
        parse_reply(&resp).ok_or_else(|| net("risposta AI vuota".into()))
    }

    /// Prova la chiave elencando i modelli dell'account.
    pub fn list_models(&self) -> Result<Vec<String>, ApiError> {
        let url = format!("{}/models", Provider::Anthropic.base_url());
        let body = self.send("", self.client.get(url))?;
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| net(e.to_string()))?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
            .unwrap_or_default())
    }

    fn send(&self, model: &str, req: RequestBuilder) -> Result<String, ApiError> {
        let started = std::time::Instant::now();
        let resp = req
            .header("x-api-key", &self.key)
            .header("anthropic-version", VERSION)
            .send()
            .map_err(|e| net(e.to_string()))?;
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<f64>().ok())
            .map(|s| s.ceil() as u64);
        let text = resp.text().map_err(|e| net(e.to_string()))?;
        let tokens = usage::total_tokens(&text);
        crate::providers::record_use(Provider::Anthropic, tokens);
        crate::config::debug(|| {
            format!(
                "Anthropic {}: HTTP {} in {} ms, {} token",
                if model.is_empty() { "(elenco modelli)" } else { model },
                status.as_u16(),
                started.elapsed().as_millis(),
                tokens
            )
        });
        if status.is_success() {
            Ok(text)
        } else {
            Err(ApiError { provider: "Anthropic", status: status.as_u16(), retry_after, msg: text.chars().take(300).collect() })
        }
    }
}

fn net(msg: String) -> ApiError {
    ApiError { provider: "Anthropic", status: 0, retry_after: None, msg }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_has_system_and_wrapped_user_text() {
        let b = request_body("claude-haiku-5-5", "Sii breve", "ciao");
        assert_eq!(b["model"], "claude-haiku-5-5");
        assert_eq!(b["system"], "Sii breve");
        assert_eq!(b["messages"][0]["role"], "user");
        assert_eq!(b["messages"][0]["content"], "<trascrizione>ciao</trascrizione>");
        assert!(b["max_tokens"].as_u64().unwrap() > 0);
        // i modelli piu' recenti rifiutano i parametri di campionamento: non si inviano
        assert!(b.get("temperature").is_none());
    }

    #[test]
    fn reply_joins_text_blocks_and_ignores_others() {
        let body = r#"{"content":[{"type":"text","text":" Ciao"},{"type":"tool_use","id":"x"},{"type":"text","text":" mondo. "}]}"#;
        assert_eq!(parse_reply(body).as_deref(), Some("Ciao mondo."));
        assert_eq!(parse_reply(r#"{"content":[]}"#), None);
        assert_eq!(parse_reply("non json"), None);
    }
}
