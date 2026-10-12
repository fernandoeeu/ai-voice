//! Saida de audio do daemon: CoreAudio (cpal) ou o fake deterministico dos testes.

use std::sync::Arc;
use std::time::Duration;

use jarvis_audio::{CpalSaida, EventoSaida, FakeSaida, Saida};
use jarvis_protocolo::{FalaId, Manter, Pcm24k};
use tokio::sync::broadcast;

#[derive(Clone)]
pub enum SaidaDaemon {
    Cpal(Arc<CpalSaida>),
    Fake(Arc<FakeSaida>),
}

impl SaidaDaemon {
    pub fn cpal() -> Result<Self, String> {
        Ok(Self::Cpal(Arc::new(CpalSaida::do_ambiente()?)))
    }

    /// Fake em tempo real: uma tarefa "consome" o buffer a cada 10 ms.
    pub fn fake() -> Self {
        const TAXA: u32 = 24_000;
        let f = Arc::new(FakeSaida::novo(TAXA));
        let bomba = f.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_millis(10));
            t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                t.tick().await;
                bomba.avancar((TAXA / 100) as usize);
            }
        });
        Self::Fake(f)
    }

    pub fn dispositivo(&self) -> (Option<String>, Option<u32>) {
        match self {
            Self::Cpal(c) => c.dispositivo().map_or((None, None), |(n, t)| (Some(n), Some(t))),
            Self::Fake(_) => (Some("fake".into()), Some(24_000)),
        }
    }

    pub fn manter(&self, padrao: Manter) -> Manter {
        match self {
            Self::Cpal(c) => c.manter(),
            Self::Fake(_) => padrao,
        }
    }
}

impl Saida for SaidaDaemon {
    fn tocar(&self, fala: FalaId, pcm: Pcm24k, ultimo: bool) {
        match self {
            Self::Cpal(c) => c.tocar(fala, pcm, ultimo),
            Self::Fake(f) => f.tocar(fala, pcm, ultimo),
        }
    }

    fn calar(&self, fade_ms: u32) {
        match self {
            Self::Cpal(c) => c.calar(fade_ms),
            Self::Fake(f) => f.calar(fade_ms),
        }
    }

    fn eventos(&self) -> broadcast::Receiver<EventoSaida> {
        match self {
            Self::Cpal(c) => c.eventos(),
            Self::Fake(f) => f.eventos(),
        }
    }
}
