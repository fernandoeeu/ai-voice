//! Protocolo NDJSON v1 entre clientes e o `jarvisd`, mais os tipos comuns
//! (`FalaId`, `Pcm24k`) usados por kokoro, audio e fila.
//!
//! Decisao: `FalaId` e `Pcm24k` moram aqui para que `jarvis-kokoro` e
//! `jarvis-audio` dependam so deste crate e nao um do outro.
//!
//! Fio: uma mensagem JSON por linha (`\n`), no maximo [`MAX_LINHA`] bytes
//! sem contar o `\n`. Toda mensagem leva `v: 1`.

mod codec;
mod comum;
mod mensagens;

pub use codec::{ErroCodec, MAX_LINHA, codificar, decodificar};
pub use comum::{FalaId, Pcm24k};
pub use mensagens::*;

/// Versao do protocolo aceita por este crate.
pub const VERSAO: u32 = 1;

/// Codigos de saida (`rc`) do evento `fim`.
pub mod rc {
    /// Falou ate o fim.
    pub const OK: i32 = 0;
    /// Erro generico.
    pub const ERRO: i32 = 1;
    /// Prazo da fila esgotado antes de comecar.
    pub const PRAZO_FILA: i32 = 75;
    /// Interrompida por `calar` (SIGTERM, como no jarvis-fala atual).
    pub const INTERROMPIDA: i32 = 143;
}
