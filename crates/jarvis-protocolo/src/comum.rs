use serde::{Deserialize, Serialize};

/// Identificador de uma fala, unico por vida do daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FalaId(pub u64);

/// Bloco de PCM mono s16le. A taxa vem junto (o Kokoro entrega 24 kHz).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm24k {
    pub amostras: Vec<i16>,
    pub taxa: u32,
}

impl Pcm24k {
    pub const TAXA: u32 = 24_000;

    pub fn new(amostras: Vec<i16>) -> Self {
        Self {
            amostras,
            taxa: Self::TAXA,
        }
    }

    /// Duracao em segundos.
    pub fn duracao_s(&self) -> f64 {
        if self.taxa == 0 {
            0.0
        } else {
            self.amostras.len() as f64 / f64::from(self.taxa)
        }
    }
}
