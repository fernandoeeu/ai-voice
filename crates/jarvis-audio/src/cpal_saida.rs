//! `CpalSaida`: stream CoreAudio residente via cpal. Uma thread gerente e dona
//! do `Stream` (que nao e `Send`), encaminha os sinais do callback como
//! `EventoSaida`, aplica a politica `Manter` e reconstroi o stream quando o
//! dispositivo padrao ou a taxa mudam. O ring buffer vive no `Nucleo` e
//! sobrevive as reconstrucoes.

use std::collections::VecDeque;
use std::sync::atomic::{
    AtomicBool,
    Ordering::{AcqRel, Relaxed},
};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use jarvis_protocolo::Manter;
use tokio::sync::broadcast;

use crate::nucleo::{Nucleo, Sinal};
use crate::{EventoSaida, FalaId, Pcm24k, Saida};

/// Folga antes de fechar o stream no modo `nunca`/`janela`, depois do
/// "esvaziou" (que ja sai na hora prevista do som real).
const FOLGA_FECHAR: Duration = Duration::from_millis(400);

enum Cmd {
    Abrir,
    Parar,
}

pub struct CpalSaida {
    nucleo: Arc<Nucleo>,
    tx: broadcast::Sender<EventoSaida>,
    cmd: mpsc::Sender<Cmd>,
    manter: Manter,
    dispositivo: Arc<Mutex<Option<(String, u32)>>>,
    gerente: Option<JoinHandle<()>>,
}

impl CpalSaida {
    /// Le `JARVISD_MANTER` (sempre|janela:<s>|nunca) e `JARVISD_DITHER_DB`.
    pub fn do_ambiente() -> Result<Self, String> {
        let manter = match std::env::var("JARVISD_MANTER") {
            Ok(v) if !v.trim().is_empty() => v.parse::<Manter>()?,
            _ => Manter::Sempre,
        };
        let dither = std::env::var("JARVISD_DITHER_DB")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok());
        Self::novo(manter, dither)
    }

    /// Em `Sempre` abre o stream ja e falha se nao houver dispositivo.
    pub fn novo(manter: Manter, dither_db: Option<f32>) -> Result<Self, String> {
        let nucleo = Nucleo::novo(48_000, 2, dither_db);
        let (tx, _) = broadcast::channel(128);
        let (cmd, rx) = mpsc::channel();
        let (pronto_tx, pronto_rx) = mpsc::channel();
        let dispositivo = Arc::new(Mutex::new(None));
        let mudou = Arc::new(AtomicBool::new(false));
        let mut g = Gerente {
            nucleo: nucleo.clone(),
            tx: tx.clone(),
            rx,
            manter,
            mudou: mudou.clone(),
            ouvintes: crate::ouvinte::Ouvintes::novo(mudou),
            dispositivo: dispositivo.clone(),
            stream: None,
            fechar_em: None,
            ultimo_erro: None,
            fila: VecDeque::new(),
        };
        let gerente = std::thread::Builder::new()
            .name("jarvis-audio".into())
            .spawn(move || g.rodar(pronto_tx))
            .map_err(|e| e.to_string())?;
        pronto_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())??;
        Ok(Self {
            nucleo,
            tx,
            cmd,
            manter,
            dispositivo,
            gerente: Some(gerente),
        })
    }

    pub fn manter(&self) -> Manter {
        self.manter
    }

    /// Nome e taxa do dispositivo atual, se houver stream aberto ou ja aberto antes.
    pub fn dispositivo(&self) -> Option<(String, u32)> {
        self.dispositivo.lock().ok().and_then(|d| d.clone())
    }
}

impl Saida for CpalSaida {
    fn tocar(&self, fala: FalaId, pcm: Pcm24k, ultimo: bool) {
        if self.manter != Manter::Sempre {
            let _ = self.cmd.send(Cmd::Abrir);
        }
        self.nucleo.enfileirar(fala, &pcm, ultimo);
    }

    fn calar(&self, fade_ms: u32) {
        self.nucleo.calar(fade_ms);
    }

    fn eventos(&self) -> broadcast::Receiver<EventoSaida> {
        self.tx.subscribe()
    }
}

impl Drop for CpalSaida {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Parar);
        if let Some(h) = self.gerente.take() {
            let _ = h.join();
        }
    }
}

struct Gerente {
    nucleo: Arc<Nucleo>,
    tx: broadcast::Sender<EventoSaida>,
    rx: mpsc::Receiver<Cmd>,
    manter: Manter,
    mudou: Arc<AtomicBool>,
    ouvintes: crate::ouvinte::Ouvintes,
    dispositivo: Arc<Mutex<Option<(String, u32)>>>,
    stream: Option<cpal::Stream>,
    fechar_em: Option<Instant>,
    ultimo_erro: Option<Instant>,
    /// Sinais do callback na ordem; o `Esvaziou` so sai na hora do som real e
    /// segura os seguintes atras dele.
    fila: VecDeque<Sinal>,
}

impl Gerente {
    fn rodar(&mut self, pronto: mpsc::Sender<Result<(), String>>) {
        let inicial = if self.manter == Manter::Sempre {
            self.abrir()
        } else {
            Ok(())
        };
        let falhou = inicial.is_err();
        let _ = pronto.send(inicial);
        if falhou {
            return;
        }
        loop {
            match self.rx.recv_timeout(Duration::from_millis(2)) {
                Ok(Cmd::Abrir) => {
                    self.fechar_em = None;
                    if self.stream.is_none() {
                        self.abrir_ou_avisar();
                    }
                }
                Ok(Cmd::Parar) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.encaminhar_sinais();
            if self.mudou.swap(false, AcqRel)
                && (self.stream.is_some() || self.manter == Manter::Sempre)
            {
                self.stream = None;
                self.abrir_ou_avisar();
            }
            if self.stream.is_none() && self.manter == Manter::Sempre {
                self.reabrir_com_pausa();
            }
            if self.fechar_em.is_some_and(|t| Instant::now() >= t) {
                self.fechar_em = None;
                if self.nucleo.ctl.pendentes() == 0 {
                    self.stream = None;
                }
            }
        }
    }

    fn reabrir_com_pausa(&mut self) {
        if self
            .ultimo_erro
            .is_none_or(|t| t.elapsed() > Duration::from_millis(500))
        {
            self.abrir_ou_avisar();
        }
    }

    fn abrir_ou_avisar(&mut self) {
        if let Err(mensagem) = self.abrir() {
            self.ultimo_erro = Some(Instant::now());
            let _ = self.tx.send(EventoSaida::Erro { mensagem });
        }
    }

    fn encaminhar_sinais(&mut self) {
        if let Ok(mut rx) = self.nucleo.sinais.lock() {
            while let Ok(s) = rx.pop() {
                self.fila.push_back(s);
            }
        }
        let agora = Instant::now();
        while let Some(&s) = self.fila.front() {
            if matches!(s, Sinal::Esvaziou { hora, .. } if agora < hora) {
                break;
            }
            self.fila.pop_front();
            match s {
                Sinal::PrimeiroSom { fala, hora } => {
                    self.fechar_em = None;
                    let _ = self.tx.send(EventoSaida::PrimeiroSom {
                        fala,
                        hora_saida: hora,
                    });
                }
                Sinal::Esvaziou { fala, .. } => {
                    let _ = self.tx.send(EventoSaida::Esvaziou { fala });
                    self.fechar_em = match self.manter {
                        Manter::Sempre => None,
                        Manter::Nunca => Some(Instant::now() + FOLGA_FECHAR),
                        Manter::Janela(s) => {
                            Some(Instant::now() + FOLGA_FECHAR + Duration::from_secs(u64::from(s)))
                        }
                    };
                }
            }
        }
    }

    fn abrir(&mut self) -> Result<(), String> {
        self.stream = None;
        let host = cpal::default_host();
        let dev = host
            .default_output_device()
            .ok_or("sem dispositivo de saida padrao")?;
        let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
        if cfg.sample_format() != cpal::SampleFormat::F32 {
            return Err(format!(
                "formato {:?} nao suportado (so f32)",
                cfg.sample_format()
            ));
        }
        let (taxa, canais) = (cfg.sample_rate(), usize::from(cfg.channels()));
        let nome = dev
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "?".into());
        // Stream ainda nao existe: sem corrida com o callback.
        self.nucleo.reconstruir(taxa, canais);
        let n = self.nucleo.clone();
        let flag = self.mudou.clone();
        let stream = dev
            .build_output_stream::<f32, _, _>(
                cfg.config(),
                move |dados, info| {
                    let ts = info.timestamp();
                    let lat = ts.playback.saturating_duration_since(ts.callback);
                    n.callback(dados, Instant::now(), lat);
                },
                move |_erro| flag.store(true, Relaxed),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        self.ouvintes.rearmar();
        let novo = (nome.clone(), taxa);
        let mudou = self
            .dispositivo
            .lock()
            .map(|mut d| d.replace(novo.clone()) != Some(novo))
            .unwrap_or(true);
        if mudou {
            let _ = self.tx.send(EventoSaida::DispositivoMudou { nome, taxa });
        }
        Ok(())
    }
}
