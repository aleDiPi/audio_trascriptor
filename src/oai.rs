//! Client per le API compatibili OpenAI (Groq e OpenAI): trascrizione, rifinitura (LLM) e prova della chiave.

use crate::providers::Provider;
use crate::usage::{self, ApiError};
use reqwest::blocking::{multipart, Client, RequestBuilder};
use std::time::Duration;

pub struct Oai {
    client: Client,
    key: String,
    provider: Provider,
}

impl Oai {
    pub fn new(provider: Provider, key: String) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Oai { client, key, provider })
    }

    pub fn groq(key: String) -> Result<Self, String> {
        Self::new(Provider::Groq, key)
    }

    pub fn openai(key: String) -> Result<Self, String> {
        Self::new(Provider::OpenAi, key)
    }

    fn base(&self) -> &'static str {
        self.provider.base_url()
    }

    pub fn transcribe(&self, wav: Vec<u8>, model: &str, language: &str) -> Result<String, ApiError> {
        let part = multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| self.net(e.to_string()))?;
        let mut form = multipart::Form::new()
            .part("file", part)
            .text("model", model.to_string())
            .text("response_format", "json")
            .text("temperature", "0");
        if !language.is_empty() {
            form = form.text("language", language.to_string());
        }
        let body = self.send(model, self.client.post(format!("{}/audio/transcriptions", self.base())).multipart(form))?;
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| self.net(e.to_string()))?;
        Ok(v["text"].as_str().unwrap_or("").trim().to_string())
    }

    /// Prova la chiave: elenca i modelli disponibili nell'account.
    pub fn list_models(&self) -> Result<Vec<String>, ApiError> {
        let body = self.send("", self.client.get(format!("{}/models", self.base())))?;
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| self.net(e.to_string()))?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
            .unwrap_or_default())
    }

    /// `system` e' il prompt completo (vedi `Config::system_prompt`).
    pub fn refine(&self, model: &str, system: &str, text: &str) -> Result<String, ApiError> {
        let body = crate::config::refine_request(model, system, text);
        let resp = self.send(
            model,
            self.client
                .post(format!("{}/chat/completions", self.base()))
                .header("Content-Type", "application/json")
                .body(body.to_string()),
        )?;
        let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| self.net(e.to_string()))?;
        let out = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim();
        if out.is_empty() {
            Err(self.net("risposta AI vuota".into()))
        } else {
            Ok(out.to_string())
        }
    }

    fn net(&self, msg: String) -> ApiError {
        ApiError { provider: self.provider.name(), status: 0, retry_after: None, msg }
    }

    fn send(&self, model: &str, req: RequestBuilder) -> Result<String, ApiError> {
        let started = std::time::Instant::now();
        let resp = req.bearer_auth(&self.key).send().map_err(|e| self.net(e.to_string()))?;
        let status = resp.status();
        let headers: Vec<(String, String)> = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
            .collect();
        let retry_after = usage::parse_retry_after(&headers);
        let text = resp.text().map_err(|e| self.net(e.to_string()))?;
        let tokens = usage::total_tokens(&text);
        if self.provider == Provider::Groq {
            usage::record(model, &headers, tokens);
        }
        crate::providers::record_use(self.provider, tokens);
        crate::config::debug(|| {
            format!(
                "{} {}: HTTP {} in {} ms, {} token",
                self.provider.name(),
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
                provider: self.provider.name(),
                status: status.as_u16(),
                retry_after,
                msg: text.chars().take(300).collect(),
            })
        }
    }
}
