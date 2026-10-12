//! Reamostragem mono f32 com `rubato::Fft` (razao fixa, qualidade maxima).
//! Roda na thread produtora, nunca no callback de audio.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

pub struct Reamostrador {
    de: u32,
    para: u32,
    fft: Option<Fft<f32>>,
}

impl Reamostrador {
    pub fn novo(de: u32, para: u32) -> Self {
        let fft = (de != para && de > 0 && para > 0)
            .then(|| Fft::<f32>::new(de as usize, para as usize, 1024, 1, FixedSync::Both).ok())
            .flatten();
        Self { de, para, fft }
    }

    pub fn taxas(&self) -> (u32, u32) {
        (self.de, self.para)
    }

    /// Converte um clipe inteiro (sem estado entre chamadas).
    pub fn processar(&mut self, entrada: &[f32]) -> Vec<f32> {
        let Some(fft) = self.fft.as_mut() else {
            return entrada.to_vec();
        };
        if entrada.is_empty() {
            return Vec::new();
        }
        let Ok(ad) = InterleavedSlice::new(entrada, 1, entrada.len()) else {
            return Vec::new();
        };
        match fft.process_all(&ad, entrada.len(), None) {
            Ok(saida) => saida.take_data(),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn senoide(freq: f32, taxa: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * freq * i as f32 / taxa as f32).sin())
            .collect()
    }

    #[test]
    fn contagem_de_amostras() {
        for (para, n) in [
            (48_000u32, 24_000usize),
            (44_100, 24_000),
            (48_000, 5_000),
            (96_000, 2_400),
            (16_000, 24_000),
        ] {
            let mut r = Reamostrador::novo(24_000, para);
            let saida = r.processar(&senoide(440.0, 24_000, n));
            let esperado = n as f64 * f64::from(para) / 24_000.0;
            assert!(
                (saida.len() as f64 - esperado).abs() <= 2.0,
                "{para}: {} vs {esperado}",
                saida.len()
            );
        }
    }

    #[test]
    fn senoide_440_preservada() {
        for para in [48_000u32, 44_100] {
            let mut r = Reamostrador::novo(24_000, para);
            let saida = r.processar(&senoide(440.0, 24_000, 24_000));
            // frequencia pelos cruzamentos de zero ascendentes (miolo, sem as bordas)
            let miolo = &saida[para as usize / 10..saida.len() - para as usize / 10];
            let cruz = miolo
                .windows(2)
                .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
                .count();
            let dur = miolo.len() as f64 / f64::from(para);
            let freq = cruz as f64 / dur;
            assert!((freq - 440.0).abs() < 3.0, "freq {freq} em {para}");
            let pico = miolo.iter().fold(0f32, |a, v| a.max(v.abs()));
            assert!((pico - 0.5).abs() < 0.03, "pico {pico}");
        }
    }

    #[test]
    fn mesma_taxa_passa_direto() {
        let mut r = Reamostrador::novo(24_000, 24_000);
        let e = senoide(440.0, 24_000, 100);
        assert_eq!(r.processar(&e), e);
    }
}
