//! Ator da fila de fala (F4).
//!
//! Um unico ator tokio, generico sobre [`Sintetizador`], [`Saida`] e [`Guardiao`]:
//! - FIFO; cada fala vira frases (`dividir_frases`, primeira fatia curta) com
//!   lookahead de 1: a frase N+1 e sintetizada enquanto a N e entregue a saida.
//! - O guardiao do lock e tomado quando a fila deixa de estar vazia, em paralelo
//!   com a primeira sintese, fica preso entre frases e entre falas e so e solto
//!   quando a saida avisa `Esvaziou` da ultima fala (fila vazia).
//! - Morte do guardiao (barge-in da Escuta) cala e limpa a fila toda (rc 143).
//! - Prazo da fila esgotado: rc 75. Falha do Kokoro: fallback `say`.
//! - Uma linha por fala no `fala.log`, no formato do `fala-kokoro.sh`.

mod ator;
mod config;
mod execucao;
mod log;

pub use ator::{Estado, Fila, FilaEncerrada, Pedido};
pub use config::Config;

pub use jarvis_audio::Saida;
pub use jarvis_kokoro::Sintetizador;
pub use jarvis_lock::Guardiao;
pub use jarvis_protocolo::{Escopo, Evento, FalaId};
