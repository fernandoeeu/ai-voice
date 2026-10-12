//! Cliente HTTP do servidor Kokoro (`tts/server.py`).

use std::time::Duration;

use jarvis_protocolo::Pcm24k;

use crate::{Result, Sintetizador, wav};

pub const URL_PADRAO: &str = "http://127.0.0.1:7349";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ErroKokoro {
    #[error("kokoro respondeu HTTP {status}: {corpo}")]
    Http { status: u16, corpo: String },
    #[error("falha de transporte: {0}")]
    Transporte(#[from] reqwest::Error),
    #[error("WAV invalido: {0}")]
    Wav(#[from] wav::ErroWav),
}

#[derive(Debug, Clone)]
pub struct KokoroHttp {
    base: String,
    http: reqwest::Client,
}

impl KokoroHttp {
    /// `base` sem barra final, por exemplo [`URL_PADRAO`].
    pub fn new(base: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .no_proxy()
            .build()?;
        Ok(Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http,
        })
    }

    /// Vozes disponiveis (`GET /voices`).
    pub async fn vozes(&self) -> Result<Vec<String>> {
        let r = self
            .http
            .get(format!("{}/voices", self.base))
            .send()
            .await
            .map_err(ErroKokoro::from)?;
        let status = r.status();
        let corpo = r.text().await.map_err(ErroKokoro::from)?;
        if !status.is_success() {
            return Err(ErroKokoro::Http {
                status: status.as_u16(),
                corpo,
            }
            .into());
        }
        let j: serde_json::Value = serde_json::from_str(&corpo)?;
        Ok(j["voices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect())
    }
}

impl Sintetizador for KokoroHttp {
    async fn sintetizar(&self, frase: &str, voz: &str, vel: f32) -> Result<Pcm24k> {
        let corpo = serde_json::json!({ "text": frase, "voice": voz, "speed": vel });
        let r = self
            .http
            .post(format!("{}/synthesize", self.base))
            .header("Content-Type", "application/json")
            .body(corpo.to_string())
            .send()
            .await
            .map_err(ErroKokoro::from)?;
        let status = r.status();
        let bytes = r.bytes().await.map_err(ErroKokoro::from)?;
        if !status.is_success() {
            let corpo = String::from_utf8_lossy(&bytes).into_owned();
            return Err(ErroKokoro::Http {
                status: status.as_u16(),
                corpo,
            }
            .into());
        }
        Ok(wav::decodificar(&bytes).map_err(ErroKokoro::from)?)
    }

    async fn saudavel(&self) -> bool {
        match self.http.get(format!("{}/health", self.base)).send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }
}
