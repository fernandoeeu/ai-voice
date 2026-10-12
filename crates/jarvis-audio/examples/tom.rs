//! Toca um tom curto no dispositivo padrao. Uso: tom [freq_hz] [segundos] [volume 0..1]
//! O volume padrao e baixo (0,03).
use std::time::Duration;

use jarvis_audio::{CpalSaida, EventoSaida, FalaId, Manter, Pcm24k, Saida};

fn main() {
    let a: Vec<f32> = std::env::args().skip(1).filter_map(|v| v.parse().ok()).collect();
    let (freq, seg, vol) = (a.first().copied().unwrap_or(440.0), a.get(1).copied().unwrap_or(0.5), a.get(2).copied().unwrap_or(0.03));
    let saida = CpalSaida::novo(Manter::Sempre, None).expect("abrir saida");
    println!("dispositivo: {:?}", saida.dispositivo());
    let mut rx = saida.eventos();
    let n = (24_000.0 * seg) as usize;
    let pcm = (0..n)
        .map(|i| (vol * (2.0 * std::f32::consts::PI * freq * i as f32 / 24_000.0).sin() * 32767.0) as i16)
        .collect();
    saida.tocar(FalaId(1), Pcm24k::new(pcm), true);
    loop {
        match rx.try_recv() {
            Ok(EventoSaida::Esvaziou { .. }) => break,
            Ok(e) => println!("{e:?}"),
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    std::thread::sleep(Duration::from_millis(300));
}
