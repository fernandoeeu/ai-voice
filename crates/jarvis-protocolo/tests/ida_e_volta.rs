use jarvis_protocolo::*;

fn volta_req(op: Op) {
    let r = Requisicao::nova("1", op);
    let linha = codificar(&r).unwrap();
    assert!(linha.ends_with('\n') && linha.matches('\n').count() == 1);
    assert_eq!(decodificar::<Requisicao>(&linha).unwrap(), r);
}

fn volta_msg(ev: Evento) {
    for id in [Some("1".to_string()), None] {
        let m = Mensagem::nova(id, ev.clone());
        let linha = codificar(&m).unwrap();
        assert_eq!(decodificar::<Mensagem>(&linha).unwrap(), m);
    }
}

#[test]
fn requisicoes_ida_e_volta() {
    volta_req(Op::Falar {
        texto: "oi".into(),
        voz: Some("pf_dora".into()),
        velocidade: Some(1.1),
        origem: Some("teste".into()),
        esperar: Esperar::PrimeiroSom,
        prazo_fila_s: Some(30),
        cancelar_ao_desconectar: Some(true),
    });
    volta_req(Op::Falar {
        texto: "oi".into(),
        voz: None,
        velocidade: None,
        origem: None,
        esperar: Esperar::Fim,
        prazo_fila_s: None,
        cancelar_ao_desconectar: None,
    });
    volta_req(Op::Falar {
        texto: "x".into(),
        voz: None,
        velocidade: None,
        origem: None,
        esperar: Esperar::Aceito,
        prazo_fila_s: None,
        cancelar_ao_desconectar: None,
    });
    volta_req(Op::Calar { escopo: Escopo::Atual });
    volta_req(Op::Calar { escopo: Escopo::Tudo });
    volta_req(Op::Estado);
    volta_req(Op::Assinar);
    volta_req(Op::Ping);
}

#[test]
fn eventos_ida_e_volta() {
    volta_msg(Evento::Aceito { fala: FalaId(7), posicao: 2 });
    volta_msg(Evento::PrimeiroSom { fala: FalaId(7), ms: 120 });
    volta_msg(Evento::Fim { fala: FalaId(7), rc: rc::OK, motor: "kokoro".into(), audio_s: 1.5, chars: 12 });
    for c in [CodigoErro::FilaEsgotou, CodigoErro::Kokoro, CodigoErro::Protocolo] {
        volta_msg(Evento::Erro { codigo: c, mensagem: "m".into() });
    }
    volta_msg(Evento::Pong);
    volta_msg(Evento::Estado(EstadoDaemon { falando: Some(FalaId(1)), fila: 3, kokoro_saudavel: true, dispositivo: Some("AirPods".into()), taxa: Some(48_000), manter: Manter::Janela(30) }));
    volta_msg(Evento::Estado(EstadoDaemon { falando: None, fila: 0, kokoro_saudavel: false, dispositivo: None, taxa: None, manter: Manter::Sempre }));
}

#[test]
fn formato_no_fio() {
    let l = codificar(&Requisicao::nova("a", Op::Ping)).unwrap();
    assert_eq!(l, "{\"v\":1,\"id\":\"a\",\"op\":\"ping\"}\n");
    let l = codificar(&Mensagem::nova(Some("a".into()), Evento::PrimeiroSom { fala: FalaId(3), ms: 9 })).unwrap();
    assert_eq!(l, "{\"v\":1,\"id\":\"a\",\"ev\":\"primeiro_som\",\"fala\":3,\"ms\":9}\n");
}

#[test]
fn rejeita_versao_desconhecida() {
    for l in [r#"{"v":2,"id":"1","op":"ping"}"#, r#"{"id":"1","op":"ping"}"#] {
        assert!(matches!(decodificar::<Requisicao>(l), Err(ErroCodec::VersaoDesconhecida(_))));
    }
}

#[test]
fn rejeita_op_desconhecida_e_json_ruim() {
    assert!(matches!(decodificar::<Requisicao>(r#"{"v":1,"id":"1","op":"xis"}"#), Err(ErroCodec::Json(_))));
    assert!(matches!(decodificar::<Requisicao>("nao e json"), Err(ErroCodec::Json(_))));
}

#[test]
fn rejeita_linha_grande_demais() {
    let texto = "a".repeat(MAX_LINHA);
    let r = Requisicao::nova(
        "1",
        Op::Falar {
            texto: texto.clone(),
            voz: None,
            velocidade: None,
            origem: None,
            esperar: Esperar::Fim,
            prazo_fila_s: None,
            cancelar_ao_desconectar: None,
        },
    );
    assert!(matches!(codificar(&r), Err(ErroCodec::LinhaGrande(_))));
    let linha = format!("{}{}", "x".repeat(MAX_LINHA + 1), "\n");
    assert!(matches!(decodificar::<Requisicao>(&linha), Err(ErroCodec::LinhaGrande(_))));
}

#[test]
fn pcm_duracao() {
    assert!((Pcm24k::new(vec![0; 24_000]).duracao_s() - 1.0).abs() < 1e-9);
}
