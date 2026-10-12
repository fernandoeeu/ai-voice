//! Cliente do Kokoro (HTTP em 127.0.0.1:7349, WAV s16le 24 kHz mono).
//!
//! - [`KokoroHttp`]: implementacao de [`Sintetizador`] sobre reqwest (timeout de 10 s).
//! - [`dividir_frases`]: porta fiel do `split_text` do `server.py`, com opcao
//!   de primeira fatia curta para o primeiro som sair mais cedo.
//! - [`wav::decodificar`]: parser WAV que valida 24 kHz, mono e 16 bits.

use std::future::Future;

pub use jarvis_protocolo::Pcm24k;

mod cliente;
mod frases;
pub mod wav;

pub use cliente::{ErroKokoro, KokoroHttp, URL_PADRAO};
pub use frases::{MAX_CHUNK, PRIMEIRA_CURTA, dividir_frases};

pub type Result<T> = anyhow::Result<T>;

/// Fonte de audio sintetizado. Uma chamada por frase, para tocar a frase N
/// enquanto a N+1 e sintetizada.
pub trait Sintetizador: Send + Sync {
    fn sintetizar(
        &self,
        frase: &str,
        voz: &str,
        vel: f32,
    ) -> impl Future<Output = Result<Pcm24k>> + Send;
    fn saudavel(&self) -> impl Future<Output = bool> + Send;
}
