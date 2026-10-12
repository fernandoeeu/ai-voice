//! Nucleo independente de dispositivo: ring buffer, marcas de primeiro som e
//! fim, fade do `calar`, dither e reconstrucao (troca de taxa) preservando o
//! buffer. O callback do CoreAudio e o `FakeSaida` chamam o mesmo
//! `Consumidor::preencher`, entao os testes cobrem o caminho real.
//!
//! Regras do callback: sem alocar, sem esperar lock (so `try_lock`), sem log.

use std::sync::atomic::{
    AtomicU32, AtomicU64,
    Ordering::{Acquire, Relaxed, Release},
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jarvis_protocolo::{FalaId, Pcm24k};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::resample::Reamostrador;

/// Capacidade fisica do ring (amostras mono): 2 s a 96 kHz.
pub const CAPACIDADE: usize = 192_000;
/// Limite logico de ocupacao: ~2 s na taxa do dispositivo.
pub const SEGUNDOS_BUFFER: u32 = 2;
const CAP_MARCAS: usize = 256;
const CAP_SINAIS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoMarca {
    Inicio,
    Fim,
}

/// Posicao absoluta (em amostras escritas) de um evento da fala.
#[derive(Debug, Clone, Copy)]
pub struct Marca {
    pub pos: u64,
    pub fala: FalaId,
    pub tipo: TipoMarca,
}

/// Sinal do callback para a thread gerente (sem alocacao).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sinal {
    PrimeiroSom { fala: FalaId, hora: Instant },
    Esvaziou { fala: FalaId },
}

/// Estado atomico compartilhado entre produtor, callback e quem chama `calar`.
#[derive(Default)]
pub struct Controle {
    pub pedidos: AtomicU64,
    pub feitos: AtomicU64,
    pub fade_ms: AtomicU32,
    pub escritas: AtomicU64,
    pub lidas: AtomicU64,
}

impl Controle {
    /// Amostras escritas e ainda nao tocadas.
    pub fn pendentes(&self) -> u64 {
        self.escritas
            .load(Acquire)
            .saturating_sub(self.lidas.load(Acquire))
    }
}

pub struct Produtor {
    amostras: Producer<f32>,
    marcas: Producer<Marca>,
    escritas: u64,
    taxa: u32,
    fala_vista: Option<FalaId>,
    reamostrador: Option<Reamostrador>,
    ctl: Arc<Controle>,
}

pub struct Consumidor {
    amostras: Consumer<f32>,
    marcas: Consumer<Marca>,
    sinais: Producer<Sinal>,
    lidas: u64,
    taxa: u32,
    canais: usize,
    fadando: bool,
    fade_restante: u32,
    fade_total: u32,
    alvo_calar: u64,
    dither: Option<(f32, u32)>,
    ctl: Arc<Controle>,
}

pub struct Nucleo {
    pub produtor: Mutex<Produtor>,
    pub consumidor: Mutex<Consumidor>,
    pub sinais: Mutex<Consumer<Sinal>>,
    pub ctl: Arc<Controle>,
}

impl Nucleo {
    /// `dither_db`: nivel do ruido em dBFS no silencio (`None` = silencio digital).
    pub fn novo(taxa: u32, canais: usize, dither_db: Option<f32>) -> Arc<Self> {
        let ctl = Arc::new(Controle::default());
        let (pa, ca) = RingBuffer::<f32>::new(CAPACIDADE);
        let (pm, cm) = RingBuffer::<Marca>::new(CAP_MARCAS);
        let (ps, cs) = RingBuffer::<Sinal>::new(CAP_SINAIS);
        Arc::new(Self {
            produtor: Mutex::new(Produtor {
                amostras: pa,
                marcas: pm,
                escritas: 0,
                taxa,
                fala_vista: None,
                reamostrador: None,
                ctl: ctl.clone(),
            }),
            consumidor: Mutex::new(Consumidor {
                amostras: ca,
                marcas: cm,
                sinais: ps,
                lidas: 0,
                taxa,
                canais: canais.max(1),
                fadando: false,
                fade_restante: 0,
                fade_total: 1,
                alvo_calar: 0,
                dither: dither_db.map(|db| (10f32.powf(db / 20.0), 0x9E37_79B9)),
                ctl: ctl.clone(),
            }),
            sinais: Mutex::new(cs),
            ctl,
        })
    }

    pub fn taxa(&self) -> u32 {
        self.produtor.lock().map(|p| p.taxa).unwrap_or(0)
    }

    /// Enfileira um bloco. Bloqueia (backpressure) se o buffer estiver cheio;
    /// desiste se um `calar` chegar no meio. Retorna `false` se desistiu.
    ///
    /// O lock do produtor e tomado por passo (um pedaco por vez) e solto a cada
    /// espera: `reconstruir` nunca fica preso atras de um produtor em
    /// backpressure. Se a taxa mudar entre passos, o resto e re-amostrado.
    pub fn enfileirar(&self, fala: FalaId, pcm: &Pcm24k, ultimo: bool) -> bool {
        let pedidos_ini = self.ctl.pedidos.load(Acquire);
        let (mut saida, mut taxa) = {
            let Ok(mut p) = self.produtor.lock() else {
                return false;
            };
            (p.converter(pcm), p.taxa)
        };
        let mut i = 0;
        loop {
            {
                let Ok(mut p) = self.produtor.lock() else {
                    return false;
                };
                if p.taxa != taxa {
                    saida = Reamostrador::novo(taxa, p.taxa).processar(&saida[i..]);
                    i = 0;
                    taxa = p.taxa;
                }
                if let Some(ok) = p.passo(fala, &saida, &mut i, ultimo, pedidos_ini) {
                    return ok;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Pede `calar` e espera o callback concluir o fade e limpar o buffer.
    /// Sem stream ativo (nada consome), limpa direto, sem fade.
    pub fn calar(&self, fade_ms: u32) {
        self.ctl.fade_ms.store(fade_ms.clamp(1, 200), Relaxed);
        let alvo = self.ctl.pedidos.fetch_add(1, Release) + 1;
        let limite = Instant::now() + Duration::from_millis(u64::from(fade_ms) + 60);
        while self.ctl.feitos.load(Acquire) < alvo && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(1));
        }
        if self.ctl.feitos.load(Acquire) < alvo
            && let Ok(mut c) = self.consumidor.try_lock()
        {
            c.descartar();
            self.ctl.feitos.fetch_max(alvo, Release);
        }
    }

    /// Troca de taxa/canais (dispositivo novo): re-amostra o que estava no
    /// buffer para a taxa nova e reajusta as marcas. Com o stream parado.
    /// Nao depende do produtor: `enfileirar` so segura o lock por um passo.
    /// Retorna quantas amostras foram preservadas.
    pub fn reconstruir(&self, nova_taxa: u32, canais: usize) -> usize {
        let (Ok(mut p), Ok(mut c)) = (self.produtor.lock(), self.consumidor.lock()) else {
            return 0;
        };
        c.canais = canais.max(1);
        let antiga = c.taxa;
        if antiga == nova_taxa {
            return c.amostras.slots();
        }
        let mut pend = Vec::with_capacity(c.amostras.slots());
        while let Ok(v) = c.amostras.pop() {
            pend.push(v);
        }
        let mut marcas = Vec::new();
        while let Ok(m) = c.marcas.pop() {
            marcas.push(m);
        }
        let lidas_antigas = c.lidas;
        let razao = f64::from(nova_taxa) / f64::from(antiga.max(1));
        let novas = Reamostrador::novo(antiga, nova_taxa).processar(&pend);
        let n = novas.len().min(CAPACIDADE);
        let lidas_novas = (lidas_antigas as f64 * razao).round() as u64;
        let escritas_novas = lidas_novas + n as u64;
        for &v in &novas[..n] {
            let _ = p.amostras.push(v);
        }
        for m in marcas {
            let rel = m.pos.saturating_sub(lidas_antigas) as f64 * razao;
            let pos = (lidas_novas + rel.round() as u64).min(escritas_novas);
            let _ = p.marcas.push(Marca { pos, ..m });
        }
        c.lidas = lidas_novas;
        c.taxa = nova_taxa;
        p.escritas = escritas_novas;
        p.taxa = nova_taxa;
        p.reamostrador = None;
        self.ctl.lidas.store(lidas_novas, Release);
        self.ctl.escritas.store(escritas_novas, Release);
        n
    }

    /// Chamado pelo callback de audio (ou pelo fake). Nunca bloqueia.
    pub fn callback(&self, saida: &mut [f32], hora_cb: Instant, latencia: Duration) {
        match self.consumidor.try_lock() {
            Ok(mut c) => c.preencher(saida, hora_cb, latencia),
            Err(_) => saida.fill(0.0),
        }
    }
}

impl Produtor {
    fn limite(&self) -> usize {
        ((self.taxa * SEGUNDOS_BUFFER) as usize).min(CAPACIDADE)
    }

    /// Converte o bloco para mono f32 na taxa do dispositivo.
    fn converter(&mut self, pcm: &Pcm24k) -> Vec<f32> {
        let mono: Vec<f32> = pcm
            .amostras
            .iter()
            .map(|&s| f32::from(s) / 32768.0)
            .collect();
        if pcm.taxa == self.taxa {
            return mono;
        }
        if self.reamostrador.as_ref().map(Reamostrador::taxas) != Some((pcm.taxa, self.taxa)) {
            self.reamostrador = Some(Reamostrador::novo(pcm.taxa, self.taxa));
        }
        self.reamostrador
            .as_mut()
            .map(|r| r.processar(&mono))
            .unwrap_or_default()
    }

    /// Um passo do enfileiramento, com o lock tomado: marca de inicio, o
    /// quanto couber de `saida[*i..]` e, no fim, a marca de fim. `None` = falta
    /// espaco (quem chama solta o lock e tenta de novo); `Some(ok)` = acabou.
    fn passo(
        &mut self,
        fala: FalaId,
        saida: &[f32],
        i: &mut usize,
        ultimo: bool,
        pedidos_ini: u64,
    ) -> Option<bool> {
        if self.ctl.pedidos.load(Acquire) != pedidos_ini {
            return Some(false);
        }
        if !saida.is_empty() && self.fala_vista != Some(fala) {
            let m = Marca {
                pos: self.escritas,
                fala,
                tipo: TipoMarca::Inicio,
            };
            self.marcas.push(m).ok()?;
            self.fala_vista = Some(fala);
        }
        let ocupado = CAPACIDADE - self.amostras.slots();
        let livre = self.limite().saturating_sub(ocupado);
        let n = livre.min(saida.len() - *i);
        if n > 0 {
            for &v in &saida[*i..*i + n] {
                let _ = self.amostras.push(v);
            }
            *i += n;
            self.escritas += n as u64;
            self.ctl.escritas.store(self.escritas, Release);
        }
        if *i < saida.len() {
            return None;
        }
        if ultimo {
            let m = Marca {
                pos: self.escritas,
                fala,
                tipo: TipoMarca::Fim,
            };
            self.marcas.push(m).ok()?;
            self.fala_vista = None;
        }
        Some(true)
    }
}

impl Consumidor {
    /// Descarta tudo (amostras e marcas) mantendo os contadores coerentes.
    pub fn descartar(&mut self) {
        while self.amostras.pop().is_ok() {
            self.lidas += 1;
        }
        while self.marcas.pop().is_ok() {}
        self.ctl.lidas.store(self.lidas, Release);
    }

    fn emitir_marcas(&mut self, hora: Instant) {
        while let Ok(m) = self.marcas.peek().copied() {
            let pronta = match m.tipo {
                TipoMarca::Inicio => m.pos < self.lidas,
                TipoMarca::Fim => m.pos <= self.lidas,
            };
            if !pronta {
                break;
            }
            let _ = self.marcas.pop();
            let _ = self.sinais.push(match m.tipo {
                TipoMarca::Inicio => Sinal::PrimeiroSom { fala: m.fala, hora },
                TipoMarca::Fim => Sinal::Esvaziou { fala: m.fala },
            });
        }
    }

    fn ruido(&mut self) -> f32 {
        match self.dither.as_mut() {
            Some((amp, estado)) => {
                let mut x = *estado;
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                *estado = x;
                (x as f32 / u32::MAX as f32 * 2.0 - 1.0) * *amp
            }
            None => 0.0,
        }
    }

    /// Preenche um buffer intercalado do dispositivo. `hora_cb + latencia` e o
    /// instante previsto em que o primeiro frame do buffer sai no DAC.
    pub fn preencher(&mut self, saida: &mut [f32], hora_cb: Instant, latencia: Duration) {
        let canais = self.canais;
        let taxa = f64::from(self.taxa.max(1));
        let pedidos = self.ctl.pedidos.load(Acquire);
        if !self.fadando && pedidos != self.ctl.feitos.load(Acquire) {
            if self.amostras.slots() == 0 {
                self.descartar();
                self.ctl.feitos.store(pedidos, Release);
            } else {
                self.fadando = true;
                self.alvo_calar = pedidos;
                self.fade_total = (self.ctl.fade_ms.load(Relaxed) * self.taxa / 1000).max(1);
                self.fade_restante = self.fade_total;
            }
        }
        self.emitir_marcas(hora_cb + latencia);
        for (f, frame) in saida.chunks_mut(canais).enumerate() {
            let hora = hora_cb + latencia + Duration::from_secs_f64(f as f64 / taxa);
            let mut v = if self.fadando && self.fade_restante == 0 {
                None
            } else {
                self.amostras.pop().ok()
            };
            if let Some(s) = v.as_mut() {
                self.lidas += 1;
                if self.fadando {
                    *s *= self.fade_restante as f32 / self.fade_total as f32;
                    self.fade_restante -= 1;
                }
            }
            let amostra = match v {
                Some(s) => {
                    self.emitir_marcas(hora);
                    s
                }
                None => self.ruido(),
            };
            frame.fill(amostra);
            if self.fadando && self.fade_restante == 0 {
                self.fadando = false;
                self.descartar();
                self.ctl.feitos.store(self.alvo_calar, Release);
            }
        }
        self.ctl.lidas.store(self.lidas, Release);
    }
}
