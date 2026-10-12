//! Confere que os tipos leem as respostas reais gravadas do herdr.

use jarvis_herdr::{AgenteInfo, StatusAgente};
use serde_json::Value;

fn fixture(nome: &str) -> Value {
    let caminho = format!("{}/tests/fixtures/{nome}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(caminho).unwrap()).unwrap()
}

#[test]
fn ping_tem_protocolo_22() {
    let v = fixture("ping.resposta.json");
    assert_eq!(v["result"]["type"], "pong");
    assert_eq!(v["result"]["protocol"], 22);
}

#[test]
fn agent_get_le_agente() {
    let v = fixture("agent.get.resposta.json");
    assert_eq!(v["result"]["type"], "agent_info");
    let a: AgenteInfo = serde_json::from_value(v["result"]["agent"].clone()).unwrap();
    assert!(!a.pane_id.is_empty());
    assert!(matches!(a.agent_status, StatusAgente::Working | StatusAgente::Idle | StatusAgente::Done | StatusAgente::Blocked));
}

#[test]
fn agent_list_le_agentes() {
    let v = fixture("agent.list.resposta.json");
    assert_eq!(v["result"]["type"], "agent_list");
    let lista: Vec<AgenteInfo> = serde_json::from_value(v["result"]["agents"].clone()).unwrap();
    assert!(!lista.is_empty());
}

#[test]
fn subscribe_e_erro() {
    assert_eq!(fixture("events.subscribe.resposta.json")["result"]["type"], "subscription_started");
    assert_eq!(fixture("erro.resposta.json")["error"]["code"], "invalid_request");
}
