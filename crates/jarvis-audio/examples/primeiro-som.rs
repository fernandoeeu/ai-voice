//! Mede de `tocar()` ate `hora_saida` (timestamp de saida do CoreAudio) com
//! manter=sempre e manter=nunca. Tom de 200 ms em volume baixo (0,02).
//! Uso: primeiro-som [N=12]
use std::time::{Duration, Instant};

use jarvis_audio::{CpalSaida, EventoSaida, FalaId, Manter, Pcm24k, Saida};

fn tom() -> Pcm24k {
    let n = 24_000 / 5;
    Pcm24k::new(
        (0..n)
            .map(|i| {
                (0.02 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 24_000.0).sin() * 32767.0)
                    as i16
            })
            .collect(),
    )
}

fn percentil(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

fn aguardar(
    rx: &mut tokio::sync::broadcast::Receiver<EventoSaida>,
    alvo: FalaId,
    esvaziar: bool,
) -> Option<Instant> {
    let limite = Instant::now() + Duration::from_secs(10);
    while Instant::now() < limite {
        match rx.try_recv() {
            Ok(EventoSaida::PrimeiroSom { fala, hora_saida }) if fala == alvo && !esvaziar => {
                return Some(hora_saida);
            }
            Ok(EventoSaida::Esvaziou { fala }) if fala == alvo && esvaziar => {
                return Some(Instant::now());
            }
            Ok(_) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    None
}

fn medir(manter: Manter, n: u64) -> Vec<f64> {
    let saida = CpalSaida::novo(manter, None).expect("abrir saida");
    let mut rx = saida.eventos();
    println!("[{manter:?}] dispositivo: {:?}", saida.dispositivo());
    std::thread::sleep(Duration::from_millis(1500)); // aquece
    let mut ms = Vec::new();
    for i in 1..=n {
        let t0 = Instant::now();
        saida.tocar(FalaId(i), tom(), true);
        match aguardar(&mut rx, FalaId(i), false) {
            Some(h) => ms.push(h.saturating_duration_since(t0).as_secs_f64() * 1000.0),
            None => println!("  fala {i}: sem primeiro som"),
        }
        aguardar(&mut rx, FalaId(i), true);
        std::thread::sleep(Duration::from_millis(1200)); // fone volta a dormir no modo nunca
    }
    ms
}

fn main() {
    let n = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(12);
    for manter in [Manter::Sempre, Manter::Nunca] {
        let mut ms = medir(manter, n);
        if ms.is_empty() {
            continue;
        }
        let lista: Vec<String> = ms.iter().map(|v| format!("{v:.0}")).collect();
        println!(
            "[{manter:?}] n={} p50={:.1} ms p95={:.1} ms  amostras={}",
            ms.len(),
            percentil(&mut ms.clone(), 0.5),
            percentil(&mut ms, 0.95),
            lista.join(",")
        );
    }
}
