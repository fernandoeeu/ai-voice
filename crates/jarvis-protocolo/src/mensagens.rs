use serde::{Deserialize, Serialize};

use crate::{FalaId, VERSAO};

/// Ate quando o `falar` fica pendente antes de responder `fim`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Esperar {
    Fim,
    PrimeiroSom,
    Aceito,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Escopo {
    Atual,
    Tudo,
}

/// Operacoes cliente -> daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Falar {
        texto: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        voz: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        velocidade: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origem: Option<String>,
        esperar: Esperar,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prazo_fila_s: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cancelar_ao_desconectar: Option<bool>,
    },
    Calar {
        escopo: Escopo,
    },
    Estado,
    Assinar,
    Ping,
}

/// Envelope cliente -> daemon: `{"v":1,"id":"..","op":"..",..}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requisicao {
    pub v: u32,
    pub id: String,
    #[serde(flatten)]
    pub op: Op,
}

impl Requisicao {
    pub fn nova(id: impl Into<String>, op: Op) -> Self {
        Self { v: VERSAO, id: id.into(), op }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodigoErro {
    FilaEsgotou,
    Kokoro,
    Protocolo,
}

/// Instantaneo do daemon, resposta de `estado` e eventos de `assinar`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstadoDaemon {
    pub falando: Option<FalaId>,
    pub fila: u32,
    pub kokoro_saudavel: bool,
    /// Nome do dispositivo de saida atual, se conhecido.
    pub dispositivo: Option<String>,
    /// Taxa do dispositivo de saida em Hz, se conhecida.
    pub taxa: Option<u32>,
    /// Se o daemon esta mantendo o dispositivo acordado (fone sem dormir).
    pub manter: bool,
}

/// Eventos daemon -> cliente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum Evento {
    Aceito { fala: FalaId, posicao: u32 },
    PrimeiroSom { fala: FalaId, ms: u64 },
    Fim { fala: FalaId, rc: i32, motor: String, audio_s: f64, chars: u32 },
    Erro { codigo: CodigoErro, mensagem: String },
    Pong,
    Estado(EstadoDaemon),
}

/// Envelope daemon -> cliente. `id` e o da requisicao respondida; eventos
/// espontaneos (de `assinar`) vao sem `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mensagem {
    pub v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub ev: Evento,
}

impl Mensagem {
    pub fn nova(id: Option<String>, ev: Evento) -> Self {
        Self { v: VERSAO, id, ev }
    }
}
