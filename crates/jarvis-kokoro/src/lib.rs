//! Cliente do Kokoro (HTTP em 127.0.0.1:7349, WAV s16le 24 kHz mono).
//! Esqueleto da F0: so o contrato. A implementacao vem na F1.

use std::future::Future;

pub use jarvis_protocolo::Pcm24k;

pub type Result<T> = anyhow::Result<T>;

/// Fonte de audio sintetizado. Uma chamada por frase, para tocar a frase N
/// enquanto a N+1 e sintetizada.
pub trait Sintetizador: Send + Sync {
    fn sintetizar(&self, frase: &str, voz: &str, vel: f32) -> impl Future<Output = Result<Pcm24k>> + Send;
    fn saudavel(&self) -> impl Future<Output = bool> + Send;
}
