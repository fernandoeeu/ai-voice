//! Ator da fila de fala (F4). Esqueleto da F0: so as mensagens do ator.

use jarvis_protocolo::{Esperar, Evento, FalaId};
use tokio::sync::{mpsc, oneshot};

/// Pedido para a fila.
#[derive(Debug)]
pub struct PedidoFala {
    pub texto: String,
    pub voz: Option<String>,
    pub velocidade: Option<f32>,
    pub origem: Option<String>,
    pub esperar: Esperar,
    pub prazo_fila_s: Option<u32>,
    /// Eventos da fala (`aceito`, `primeiro_som`, `fim`, `erro`).
    pub eventos: mpsc::UnboundedSender<Evento>,
}

/// Mensagens que o ator da fila recebe.
#[derive(Debug)]
pub enum ComandoFila {
    Falar(PedidoFala, oneshot::Sender<FalaId>),
    CalarAtual,
    CalarTudo,
}

/// Alca para o ator.
#[derive(Debug, Clone)]
pub struct Fila {
    pub tx: mpsc::Sender<ComandoFila>,
}
