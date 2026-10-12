use jarvis_audio::{EventoSaida, FakeSaida, FalaId, Pcm24k, Saida};

fn tom(freq: f32, n: usize) -> Pcm24k {
    Pcm24k::new(
        (0..n)
            .map(|i| {
                (0.5 * (2.0 * std::f32::consts::PI * freq * i as f32 / 24_000.0).sin() * 32767.0)
                    as i16
            })
            .collect(),
    )
}

fn eventos(rx: &mut tokio::sync::broadcast::Receiver<EventoSaida>) -> Vec<EventoSaida> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

#[test]
fn primeiro_som_e_esvaziou_na_ordem() {
    let f = FakeSaida::novo(48_000);
    let mut rx = f.eventos();
    f.avancar(480); // silencio antes da fala
    assert!(eventos(&mut rx).is_empty());
    f.tocar(FalaId(1), tom(440.0, 2_400), false); // 100 ms
    f.tocar(FalaId(1), tom(440.0, 2_400), true);
    f.avancar(4_800); // 100 ms tocados: ainda falta metade
    let ev = eventos(&mut rx);
    assert_eq!(ev.len(), 1, "{ev:?}");
    let EventoSaida::PrimeiroSom { fala, hora_saida } = ev[0] else {
        panic!("{ev:?}")
    };
    assert_eq!(fala, FalaId(1));
    // 480 frames de silencio = 10 ms, mais a latencia de 10 ms
    let esperado = f.hora_base() + std::time::Duration::from_millis(10) + f.latencia();
    let d = if hora_saida > esperado {
        hora_saida - esperado
    } else {
        esperado - hora_saida
    };
    assert!(d < std::time::Duration::from_micros(100), "{d:?}");
    f.avancar(4_799); // falta 1 amostra
    assert!(eventos(&mut rx).is_empty(), "Esvaziou cedo demais");
    f.avancar(1);
    assert_eq!(
        eventos(&mut rx),
        vec![EventoSaida::Esvaziou { fala: FalaId(1) }]
    );
    f.avancar(480);
    assert!(eventos(&mut rx).is_empty());
}

#[test]
fn esvaziou_so_no_bloco_ultimo() {
    let f = FakeSaida::novo(24_000);
    let mut rx = f.eventos();
    f.tocar(FalaId(7), tom(300.0, 1_000), false);
    f.avancar(5_000); // buffer secou mas a fala nao terminou
    let ev = eventos(&mut rx);
    assert!(
        matches!(ev.as_slice(), [EventoSaida::PrimeiroSom { .. }]),
        "{ev:?}"
    );
    f.tocar(FalaId(7), tom(300.0, 1_000), true);
    f.avancar(1_000);
    assert_eq!(
        eventos(&mut rx),
        vec![EventoSaida::Esvaziou { fala: FalaId(7) }]
    );
}

#[test]
fn duas_falas_encadeadas() {
    let f = FakeSaida::novo(24_000);
    let mut rx = f.eventos();
    f.tocar(FalaId(1), tom(300.0, 500), true);
    f.tocar(FalaId(2), tom(300.0, 500), true);
    f.avancar(1_000);
    let ev = eventos(&mut rx);
    let tipos: Vec<_> = ev
        .iter()
        .map(|e| match e {
            EventoSaida::PrimeiroSom { fala, .. } => ('p', fala.0),
            EventoSaida::Esvaziou { fala } => ('e', fala.0),
            _ => ('?', 0),
        })
        .collect();
    assert_eq!(tipos, vec![('p', 1), ('e', 1), ('p', 2), ('e', 2)]);
}

#[test]
fn calar_faz_fade_sem_estalo_e_limpa_o_buffer() {
    let f = FakeSaida::novo(48_000);
    let mut rx = f.eventos();
    f.tocar(FalaId(1), tom(440.0, 12_000), true); // 0,5 s
    f.avancar(2_000);
    f.calar(8);
    f.avancar(4_800);
    let g = f.gravado();
    // delta maximo entre amostras vizinhas: a senoide de 440 Hz a 0,5 de pico
    // varia no maximo 2*pi*440/48000*0,5 ~ 0,029 por amostra; o fade nao pode passar disso
    let max_delta = g
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0f32, f32::max);
    assert!(max_delta < 0.035, "estalo: delta {max_delta}");
    // termina em silencio e o buffer foi limpo
    assert!(g[g.len() - 100..].iter().all(|v| *v == 0.0));
    assert_eq!(f.pendentes(), 0);
    // fade durou ~8 ms (384 frames): depois de 2000+384+margem nao ha mais som
    assert!(g[2_000 + 400..].iter().all(|v| *v == 0.0));
    assert!(g[2_000..2_000 + 384].iter().any(|v| v.abs() > 0.01));
    // calar descarta a fala: nada de Esvaziou
    assert!(
        !eventos(&mut rx)
            .iter()
            .any(|e| matches!(e, EventoSaida::Esvaziou { .. }))
    );
    // e a saida continua utilizavel
    f.tocar(FalaId(2), tom(440.0, 480), true);
    f.avancar(960);
    assert!(eventos(&mut rx).contains(&EventoSaida::Esvaziou { fala: FalaId(2) }));
}

#[test]
fn reconstrucao_preserva_o_buffer() {
    let f = FakeSaida::novo(48_000);
    let mut rx = f.eventos();
    f.tocar(FalaId(1), tom(440.0, 24_000), true); // 1 s -> 48000 amostras a 48 kHz
    f.avancar(10_000);
    let antes = f.pendentes();
    assert_eq!(antes, 38_000);
    let preservadas = f.reconstruir(44_100); // fone novo a 44,1 kHz
    let esperado = (38_000f64 * 44_100.0 / 48_000.0) as i64;
    assert!(
        (preservadas as i64 - esperado).abs() < 8,
        "{preservadas} vs {esperado}"
    );
    assert_eq!(f.pendentes() as usize, preservadas);
    eventos(&mut rx);
    // consome o resto na taxa nova: o Esvaziou sai uma unica vez e no fim
    let resto = f.avancar(preservadas - 1);
    assert!(
        !eventos(&mut rx)
            .iter()
            .any(|e| matches!(e, EventoSaida::Esvaziou { .. }))
    );
    f.avancar(1);
    let ev = eventos(&mut rx);
    assert_eq!(
        ev.iter()
            .filter(|e| matches!(e, EventoSaida::Esvaziou { .. }))
            .count(),
        1
    );
    // o conteudo continua sendo a senoide de 440 Hz (agora a 44,1 kHz)
    let miolo = &resto[1_000..resto.len() - 1_000];
    let cruz = miolo
        .windows(2)
        .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
        .count();
    let freq = cruz as f64 / (miolo.len() as f64 / 44_100.0);
    assert!((freq - 440.0).abs() < 5.0, "freq {freq}");
}

#[test]
fn bloco_em_outra_taxa_e_reamostrado_na_producao() {
    let f = FakeSaida::novo(48_000);
    f.tocar(FalaId(1), tom(440.0, 24_000), true);
    assert!((f.pendentes() as i64 - 48_000).abs() <= 2);
}
