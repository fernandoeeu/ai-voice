//! Regressoes da revisao do jarvis-audio (deadlock na reconstrucao, calar que
//! vazava durante o backpressure e Esvaziou antes do som real).

use std::sync::Arc;
use std::time::Duration;

use jarvis_audio::{FakeSaida, FalaId, Pcm24k, Saida};

fn tom(n: usize) -> Pcm24k {
    Pcm24k::new(
        (0..n)
            .map(|i| ((i as f32 * 0.1).sin() * 16_000.0) as i16)
            .collect(),
    )
}

#[test]
fn reconstruir_com_produtor_em_backpressure_nao_trava() {
    let f = Arc::new(FakeSaida::novo(24_000));
    let p = f.clone();
    std::thread::spawn(move || p.tocar(FalaId(1), tom(24_000 * 5), true)); // 5 s, buffer 2 s
    std::thread::sleep(Duration::from_millis(200));
    let r = f.clone();
    let h = std::thread::spawn(move || r.reconstruir(48_000));
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        h.is_finished(),
        "DEADLOCK: reconstruir preso esperando o produtor (stream parado)"
    );
}

#[test]
fn calar_durante_backpressure_nao_vaza_resto_da_frase() {
    let f = Arc::new(FakeSaida::novo(24_000));
    let p = f.clone();
    let prod = std::thread::spawn(move || p.tocar(FalaId(1), tom(24_000 * 5), true));
    std::thread::sleep(Duration::from_millis(200));
    f.avancar(2_400);
    std::thread::sleep(Duration::from_millis(20));
    f.calar(30);
    std::thread::sleep(Duration::from_millis(50));
    let n0 = f.gravado().len();
    for _ in 0..100 {
        f.avancar(2_400);
        std::thread::sleep(Duration::from_millis(2));
    }
    let g = f.gravado();
    let vazou = g[n0..].iter().filter(|v| v.abs() > 0.01).count();
    let _ = prod.join();
    assert_eq!(
        vazou, 0,
        "{vazou} amostras da fala calada tocaram depois do calar"
    );
}

#[test]
fn bloco_seguinte_de_fala_calada_e_descartado() {
    let f = FakeSaida::novo(24_000);
    f.tocar(FalaId(1), tom(2_400), false);
    f.avancar(1_200);
    f.calar(10);
    assert_eq!(f.pendentes(), 0);
    // o sintetizador ainda manda o resto da fala calada: nada pode tocar
    f.tocar(FalaId(1), tom(2_400), true);
    assert_eq!(f.pendentes(), 0);
    let g = f.avancar(4_800);
    assert!(g.iter().all(|v| *v == 0.0));
    // uma fala nova toca normalmente
    f.tocar(FalaId(2), tom(2_400), true);
    assert_eq!(f.pendentes(), 2_400);
}
