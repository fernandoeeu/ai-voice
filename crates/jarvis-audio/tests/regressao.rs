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
