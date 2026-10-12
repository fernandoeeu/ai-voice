//! `FakeSaida`: saida deterministica sem dispositivo, com relogio manual.
//! Usa o mesmo `Nucleo` do `CpalSaida`; o "dispositivo" e o `avancar`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::nucleo::{Nucleo, Sinal};
use crate::{EventoSaida, FalaId, Pcm24k, Saida};

pub struct FakeSaida {
    nucleo: Arc<Nucleo>,
    tx: broadcast::Sender<EventoSaida>,
    base: Instant,
    frames: Mutex<u64>,
    gravado: Mutex<Vec<f32>>,
    latencia: Duration,
    /// Espelho da taxa do nucleo: ler `nucleo.taxa()` toma o lock do produtor,
    /// que fica preso enquanto `tocar` espera espaco (travaria o `avancar`).
    taxa: AtomicU32,
}

impl FakeSaida {
    pub fn novo(taxa: u32) -> Self {
        Self {
            nucleo: Nucleo::novo(taxa, 1, None),
            tx: broadcast::channel(64).0,
            base: Instant::now(),
            frames: Mutex::new(0),
            gravado: Mutex::new(Vec::new()),
            latencia: Duration::from_millis(10),
            taxa: AtomicU32::new(taxa),
        }
    }

    pub fn hora_base(&self) -> Instant {
        self.base
    }

    pub fn latencia(&self) -> Duration {
        self.latencia
    }

    /// Simula o dispositivo consumindo `frames`; devolve o que "tocou".
    pub fn avancar(&self, frames: usize) -> Vec<f32> {
        let taxa = f64::from(self.taxa.load(Ordering::Relaxed));
        let mut out = vec![0.0; frames];
        let mut n = self.frames.lock().unwrap();
        let hora = self.base + Duration::from_secs_f64(*n as f64 / taxa);
        self.nucleo.callback(&mut out, hora, self.latencia);
        *n += frames as u64;
        drop(n);
        self.gravado.lock().unwrap().extend_from_slice(&out);
        self.bombear();
        out
    }

    /// Tudo que o dispositivo ja "tocou".
    pub fn gravado(&self) -> Vec<f32> {
        self.gravado.lock().unwrap().clone()
    }

    /// Encaminha os sinais do callback para o canal de eventos.
    pub fn bombear(&self) {
        let mut rx = self.nucleo.sinais.lock().unwrap();
        while let Ok(s) = rx.pop() {
            let _ = self.tx.send(match s {
                Sinal::PrimeiroSom { fala, hora } => EventoSaida::PrimeiroSom {
                    fala,
                    hora_saida: hora,
                },
                Sinal::Esvaziou { fala } => EventoSaida::Esvaziou { fala },
            });
        }
    }

    /// Simula troca de dispositivo/taxa: o buffer e preservado.
    pub fn reconstruir(&self, nova_taxa: u32) -> usize {
        let n = self.nucleo.reconstruir(nova_taxa, 1);
        self.taxa.store(nova_taxa, Ordering::Relaxed);
        let _ = self.tx.send(EventoSaida::DispositivoMudou {
            nome: "fake".into(),
            taxa: nova_taxa,
        });
        n
    }

    pub fn pendentes(&self) -> u64 {
        self.nucleo.ctl.pendentes()
    }
}

impl Saida for FakeSaida {
    fn tocar(&self, fala: FalaId, pcm: Pcm24k, ultimo: bool) {
        self.nucleo.enfileirar(fala, &pcm, ultimo);
    }

    /// No fake o fade roda aqui mesmo, avancando o relogio o suficiente.
    fn calar(&self, fade_ms: u32) {
        self.nucleo
            .ctl
            .fade_ms
            .store(fade_ms.clamp(1, 200), std::sync::atomic::Ordering::Relaxed);
        self.nucleo
            .ctl
            .pedidos
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        let frames =
            (u64::from(fade_ms) * u64::from(self.taxa.load(Ordering::Relaxed)) / 1000) as usize + 2;
        self.avancar(frames);
    }

    fn eventos(&self) -> broadcast::Receiver<EventoSaida> {
        self.tx.subscribe()
    }
}
