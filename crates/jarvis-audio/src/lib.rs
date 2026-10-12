//! Saida de audio residente (CoreAudio na F2). Esqueleto da F0: so o contrato.
//! `Pcm24k` e `FalaId` vem de `jarvis-protocolo`.

use std::time::Instant;

pub use jarvis_protocolo::{FalaId, Pcm24k};
use tokio::sync::broadcast;

/// Eventos emitidos pela saida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoSaida {
    /// Primeira amostra da fala saiu para o dispositivo (marca o "primeiro som").
    PrimeiroSom { fala: FalaId, hora_saida: Instant },
    /// O buffer da fala esvaziou (fim do som).
    Esvaziou { fala: FalaId },
    /// O dispositivo padrao mudou (fone conectou/desconectou).
    DispositivoMudou { nome: String, taxa: u32 },
    /// Falha na saida.
    Erro { mensagem: String },
}

pub trait Saida: Send + Sync {
    /// Enfileira um bloco da fala; `ultimo` marca o ultimo bloco (para `Esvaziou`).
    fn tocar(&self, fala: FalaId, pcm: Pcm24k, ultimo: bool);
    /// Para tudo com um fade curto.
    fn calar(&self, fade_ms: u32);
    fn eventos(&self) -> broadcast::Receiver<EventoSaida>;
}
