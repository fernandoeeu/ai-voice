use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

use crate::VERSAO;

/// Limite de uma linha, sem o `\n` final.
pub const MAX_LINHA: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum ErroCodec {
    #[error("linha com {0} bytes passa do limite de {MAX_LINHA}")]
    LinhaGrande(usize),
    #[error("versao de protocolo desconhecida: {0:?}")]
    VersaoDesconhecida(Option<u64>),
    #[error("json invalido: {0}")]
    Json(#[from] serde_json::Error),
}

/// Serializa uma mensagem em uma linha terminada em `\n`.
pub fn codificar<T: Serialize>(msg: &T) -> Result<String, ErroCodec> {
    let mut s = serde_json::to_string(msg)?;
    if s.len() > MAX_LINHA {
        return Err(ErroCodec::LinhaGrande(s.len()));
    }
    s.push('\n');
    Ok(s)
}

/// Le uma linha (com ou sem `\n`/`\r\n` final). Rejeita linha grande demais
/// e `v` diferente de 1 antes de tentar casar o formato.
pub fn decodificar<T: DeserializeOwned>(linha: &str) -> Result<T, ErroCodec> {
    let linha = linha.trim_end_matches(['\n', '\r']);
    if linha.len() > MAX_LINHA {
        return Err(ErroCodec::LinhaGrande(linha.len()));
    }
    let valor: serde_json::Value = serde_json::from_str(linha)?;
    match valor.get("v").and_then(serde_json::Value::as_u64) {
        Some(v) if v == u64::from(VERSAO) => Ok(serde_json::from_value(valor)?),
        outra => Err(ErroCodec::VersaoDesconhecida(outra)),
    }
}
