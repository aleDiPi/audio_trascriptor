//! Client Groq: trascrizione (Whisper), rifinitura (LLM) e prova della chiave.

use crate::usage::{self, ApiError};
use reqwest::blocking::{multipart, Client, RequestBuilder};
use std::time::Duration;

const BASE: &str = "https://api.groq.com/openai/v1";

pub struct Groq {
    client: Client,
    key: String,
}

impl Groq {
    pub fn new(key: String) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Groq { client, key })
    }

    pub fn transcribe(&self, wav: Vec<u8>, model: &str, language: &str) -> Result<String, ApiError> {
        let part = multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| net(e.to_string()))?;
        let mut form = multipart::Form::new()
            .part("file", part)
            .text("model", model.to_string())
            .text("response_format", "json")
            .text("temperature", "0");
        if !language.is_empty() {
            form = form.text("language", language.to_string());
        }
        let body = self.send(model, self.client.post(format!("{BASE}/audio/transcriptions")).multipart(form))?;
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| net(e.to_string()))?;
        Ok(v["text"].as_str().unwrap_or("").trim().to_string())
    }

    /// Prova la chiave: elenca i modelli disponibili nell'account.
    pub fn list_models(&self) -> Result<Vec<String>, ApiError> {
        let body = self.send("", self.client.get(format!("{BASE}/models")))?;
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| net(e.to_string()))?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
            .unwrap_or_default())
    }

    /// `system` e' il prompt completo (vedi `Config::system_prompt`).
    pub fn refine(&self, model: &str, system: &str, text: &str) -> Result<String, ApiError> {
        let mut body = serde_json::json!({
            "model": model,
            "temperature": 0.2,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": format!("<trascrizione>{text}</trascrizione>")}
            ]
        });
        // risposta rapida: serve solo pulizia del testo, non ragionamento
        if model.contains("gpt-oss") {
            body["reasoning_effort"] = "low".into();
        } else if model.contains("qwen") {
            body["reasoning_effort"] = "none".into();
        }
        let resp = self.send(
            model,
            self.client
                .post(format!("{BASE}/chat/completions"))
                .header("Content-Type", "application/json")
                .body(body.to_string()),
        )?;
        let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| net(e.to_string()))?;
        let out = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim();
        if out.is_empty() {
            Err(net("risposta AI vuota".into()))
        } else {
            Ok(out.to_string())
        }
    }

    fn send(&self, model: &str, req: RequestBuilder) -> Result<String, ApiError> {
        let started = std::time::Instant::now();
        let resp = req.bearer_auth(&self.key).send().map_err(|e| net(e.to_string()))?;
        let status = resp.status();
        let headers: Vec<(String, String)> = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
            .collect();
        let retry_after = usage::parse_retry_after(&headers);
        let text = resp.text().map_err(|e| net(e.to_string()))?;
        let tokens = usage::total_tokens(&text);
        usage::record(model, &headers, tokens);
        crate::config::debug(|| {
            format!(
                "Groq {}: HTTP {} in {} ms, {} token",
                if model.is_empty() { "(elenco modelli)" } else { model },
                status.as_u16(),
                started.elapsed().as_millis(),
                tokens
            )
        });
        if status.is_success() {
            Ok(text)
        } else {
            Err(ApiError {
                status: status.as_u16(),
                retry_after,
                msg: text.chars().take(300).collect(),
            })
        }
    }
}

fn net(msg: String) -> ApiError {
    ApiError { status: 0, retry_after: None, msg }
}
