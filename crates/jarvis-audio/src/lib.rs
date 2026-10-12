//! Saida de audio residente. `CpalSaida` mantem um stream CoreAudio aberto e
//! ativo (elimina os 1,1 a 1,7 s que o afplay gasta abrindo o Bluetooth);
//! `FakeSaida` e a versao deterministica para testes.
//! `Pcm24k` e `FalaId` vem de `jarvis-protocolo`.

use std::time::Instant;

pub use jarvis_protocolo::{FalaId, Manter, Pcm24k};
use tokio::sync::broadcast;

mod cpal_saida;
mod fake;
pub mod nucleo;
mod ouvinte;
pub mod resample;

pub use cpal_saida::CpalSaida;
pub use fake::FakeSaida;

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
    /// Pode bloquear (backpressure) se o buffer de ~2 s estiver cheio.
    fn tocar(&self, fala: FalaId, pcm: Pcm24k, ultimo: bool);
    /// Para tudo com um fade curto.
    fn calar(&self, fade_ms: u32);
    fn eventos(&self) -> broadcast::Receiver<EventoSaida>;
}
