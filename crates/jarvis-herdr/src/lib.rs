//! Cliente NDJSON do `~/.config/herdr/herdr.sock` (F7). Formato no fio em
//! `PROTOCOLO.md`. Esqueleto da F0: tipos e o trait `Herdr`.

use std::future::Future;

use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

pub type Result<T> = anyhow::Result<T>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusAgente {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

/// Subconjunto de `AgentInfo` que o Jarvis usa; campos extras sao ignorados.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgenteInfo {
    pub name: Option<String>,
    pub agent: Option<String>,
    pub agent_status: StatusAgente,
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub interactive_ready: bool,
    pub cwd: Option<String>,
    pub state_change_seq: Option<u64>,
}

/// Subscricao de `events.subscribe` (so os tipos que interessam).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum Subscricao {
    #[serde(rename = "pane.agent_status_changed")]
    StatusMudou { pane_id: String, agent_status: Option<StatusAgente> },
}

/// Evento empurrado pelo herdr depois de `events.subscribe`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EventoHerdr {
    pub event: String,
    pub data: serde_json::Value,
}

pub trait Herdr: Send + Sync {
    fn agentes(&self) -> impl Future<Output = Result<Vec<AgenteInfo>>> + Send;
    fn agente(&self, pane: &str) -> impl Future<Output = Result<AgenteInfo>> + Send;
    /// `pane.send_input` com `text`; o steer completo (texto + ctrl+enter) e uma chamada so.
    fn steer(&self, pane: &str, texto: &str) -> impl Future<Output = Result<()>> + Send;
    /// `agent.send_keys`.
    fn teclas(&self, pane: &str, teclas: &[&str]) -> impl Future<Output = Result<()>> + Send;
    /// `agent.prompt`.
    fn prompt(&self, alvo: &str, texto: &str) -> impl Future<Output = Result<()>> + Send;
    fn assinar(&self, subs: Vec<Subscricao>) -> impl Future<Output = Result<BoxStream<'static, Result<EventoHerdr>>>> + Send;
}
