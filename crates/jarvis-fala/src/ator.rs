//! O ator da fila: FIFO, guardiao do lock, barge-in, prazo e `fim`.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use jarvis_audio::Saida;
use jarvis_kokoro::Sintetizador;
use jarvis_lock::{ErroLock, GuardaLock, Guardiao};
use jarvis_protocolo::{Escopo, Evento, FalaId, rc};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::execucao::{Ctx, Entrada, Progresso, executar};
use crate::log;

pub(crate) const RC_OK: i32 = rc::OK;
pub(crate) const RC_ERRO: i32 = rc::ERRO;
pub(crate) const RC_INTERROMPIDA: i32 = rc::INTERROMPIDA;

/// Pedido de fala.
#[derive(Debug, Clone, Default)]
pub struct Pedido {
    pub texto: String,
    pub voz: Option<String>,
    pub vel: Option<f32>,
    pub origem: Option<String>,
    /// Quanto o pedido pode esperar na fila antes de virar rc 75.
    pub prazo_fila: Option<Duration>,
}

impl Pedido {
    pub fn novo(texto: impl Into<String>) -> Self {
        Self {
            texto: texto.into(),
            ..Self::default()
        }
    }
}

/// Instantaneo da fila.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Estado {
    pub falando: Option<FalaId>,
    /// Falas esperando atras da atual.
    pub fila: u32,
    /// O guardiao do lock esta preso.
    pub lock_preso: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("a fila de fala foi encerrada")]
pub struct FilaEncerrada;

enum Comando {
    Falar(
        Pedido,
        mpsc::UnboundedSender<Evento>,
        oneshot::Sender<FalaId>,
    ),
    Calar(Escopo),
    Cancelar(FalaId),
    Estado(oneshot::Sender<Estado>),
}

/// Alca (barata de clonar) para o ator. O ator termina quando todas somem.
#[derive(Debug, Clone)]
pub struct Fila {
    tx: mpsc::Sender<Comando>,
}

impl Fila {
    /// Sobe o ator na runtime tokio atual.
    pub fn iniciar<S, A, G>(synth: S, saida: A, guardiao: G, cfg: Config) -> Self
    where
        S: Sintetizador + 'static,
        A: Saida + 'static,
        G: Guardiao + 'static,
    {
        let (tx, rx) = mpsc::channel(64);
        let (lock_tx, _) = watch::channel(false);
        let ator = Ator {
            ctx: Ctx {
                synth: Arc::new(synth),
                saida: Arc::new(saida),
                cfg: Arc::new(cfg),
            },
            guardiao: Arc::new(guardiao),
            rx,
            proximo_id: 1,
            fila: VecDeque::new(),
            atual: None,
            adquirindo: None,
            guarda: None,
            lock_tx,
        };
        tokio::spawn(ator.rodar());
        Self { tx }
    }

    /// Enfileira. Devolve o id e o canal de eventos (`Aceito`, `PrimeiroSom`, `Fim`).
    pub async fn falar(
        &self,
        pedido: Pedido,
    ) -> Result<(FalaId, mpsc::UnboundedReceiver<Evento>), FilaEncerrada> {
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let (id_tx, id_rx) = oneshot::channel();
        self.tx
            .send(Comando::Falar(pedido, ev_tx, id_tx))
            .await
            .map_err(|_| FilaEncerrada)?;
        Ok((id_rx.await.map_err(|_| FilaEncerrada)?, ev_rx))
    }

    /// `calar(atual)` ou `calar(tudo)`: as falas afetadas terminam com rc 143.
    pub async fn calar(&self, escopo: Escopo) -> Result<(), FilaEncerrada> {
        self.tx
            .send(Comando::Calar(escopo))
            .await
            .map_err(|_| FilaEncerrada)
    }

    /// Cancela uma fala especifica (atual ou na fila) com rc 143.
    pub async fn cancelar(&self, id: FalaId) -> Result<(), FilaEncerrada> {
        self.tx
            .send(Comando::Cancelar(id))
            .await
            .map_err(|_| FilaEncerrada)
    }

    pub async fn estado(&self) -> Result<Estado, FilaEncerrada> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Comando::Estado(tx))
            .await
            .map_err(|_| FilaEncerrada)?;
        rx.await.map_err(|_| FilaEncerrada)
    }
}

struct Item {
    id: FalaId,
    pedido: Pedido,
    ev: mpsc::UnboundedSender<Evento>,
    enfileirada: Instant,
    prazo: Duration,
    chars: u32,
}

impl Item {
    fn limite(&self) -> Instant {
        self.enfileirada + self.prazo
    }
}

struct Atual {
    item: Item,
    prog: Arc<Mutex<Progresso>>,
    inicio: Instant,
    fut: Pin<Box<dyn Future<Output = i32> + Send>>,
}

struct Ator<S, A, G> {
    ctx: Ctx<S, A>,
    guardiao: Arc<G>,
    rx: mpsc::Receiver<Comando>,
    proximo_id: u64,
    fila: VecDeque<Item>,
    atual: Option<Atual>,
    adquirindo: Option<JoinHandle<Result<GuardaLock, ErroLock>>>,
    guarda: Option<GuardaLock>,
    lock_tx: watch::Sender<bool>,
}

impl<S, A, G> Ator<S, A, G>
where
    S: Sintetizador + 'static,
    A: Saida + 'static,
    G: Guardiao + 'static,
{
    async fn rodar(mut self) {
        loop {
            self.avancar();
            let morte = self.guarda.as_ref().map(GuardaLock::morreu);
            let limite = self.fila.iter().map(Item::limite).min();
            tokio::select! {
                cmd = self.rx.recv() => match cmd {
                    Some(c) => self.comando(c).await,
                    None => break,
                },
                rc = async {
                    match self.atual.as_mut() {
                        Some(a) => a.fut.as_mut().await,
                        None => std::future::pending().await,
                    }
                }, if self.atual.is_some() => self.terminou(rc),
                lock = async {
                    match self.adquirindo.as_mut() {
                        Some(h) => h.await,
                        None => std::future::pending().await,
                    }
                }, if self.adquirindo.is_some() => {
                    self.adquirindo = None;
                    self.lock_adquirido(lock).await;
                },
                () = async {
                    match morte {
                        Some(m) => m.await,
                        None => std::future::pending().await,
                    }
                }, if self.guarda.is_some() => self.barge_in().await,
                () = async {
                    match limite {
                        Some(l) => tokio::time::sleep_until(l.into()).await,
                        None => std::future::pending().await,
                    }
                }, if limite.is_some() => self.expirar(),
            }
        }
        // Encerrando: ninguem fica sem `fim`.
        self.atual = None;
        self.encerrar_todas(RC_INTERROMPIDA);
    }

    /// Inicia a proxima fala (e a aquisicao do lock) se estiver ocioso.
    fn avancar(&mut self) {
        if self.atual.is_some() {
            return;
        }
        while let Some(item) = self.fila.pop_front() {
            if item.enfileirada.elapsed() >= item.prazo {
                self.fim(item, rc::PRAZO_FILA, None, Instant::now());
                continue;
            }
            if self.guarda.is_none() {
                // Uma aquisicao antiga (de fala cancelada) carrega o prazo da
                // outra: refaz com o prazo desta fala.
                if let Some(h) = self.adquirindo.take() {
                    h.abort();
                }
                let restante = item.prazo.saturating_sub(item.enfileirada.elapsed());
                let g = self.guardiao.clone();
                self.adquirindo = Some(tokio::spawn(async move { g.segurar(restante).await }));
            }
            let prog = Arc::new(Mutex::new(Progresso::default()));
            let cfg = &self.ctx.cfg;
            let entrada = Entrada {
                id: item.id,
                texto: item.pedido.texto.clone(),
                voz: item
                    .pedido
                    .voz
                    .clone()
                    .unwrap_or_else(|| cfg.voz_padrao.clone()),
                vel: item.pedido.vel.unwrap_or(cfg.vel_padrao),
                lock: self.lock_tx.subscribe(),
                prog: prog.clone(),
                ev: item.ev.clone(),
            };
            let fut = Box::pin(executar(self.ctx.clone(), entrada));
            self.atual = Some(Atual {
                item,
                prog,
                inicio: Instant::now(),
                fut,
            });
            return;
        }
    }

    async fn comando(&mut self, c: Comando) {
        match c {
            Comando::Falar(pedido, ev, id_tx) => {
                let id = FalaId(self.proximo_id);
                self.proximo_id += 1;
                let posicao = self.fila.len() as u32 + u32::from(self.atual.is_some());
                let prazo = pedido.prazo_fila.unwrap_or(self.ctx.cfg.prazo_fila_padrao);
                let chars = pedido.texto.chars().count() as u32;
                let _ = ev.send(Evento::Aceito { fala: id, posicao });
                let _ = id_tx.send(id);
                self.fila.push_back(Item {
                    id,
                    pedido,
                    ev,
                    enfileirada: Instant::now(),
                    prazo,
                    chars,
                });
            }
            Comando::Calar(Escopo::Atual) => self.calar_atual().await,
            Comando::Calar(Escopo::Tudo) => {
                self.calar_atual().await;
                self.encerrar_todas(RC_INTERROMPIDA);
                self.soltar_se_vazio();
            }
            Comando::Cancelar(id) => {
                if self.atual.as_ref().is_some_and(|a| a.item.id == id) {
                    self.calar_atual().await;
                } else if let Some(pos) = self.fila.iter().position(|i| i.id == id)
                    && let Some(item) = self.fila.remove(pos)
                {
                    self.fim(item, RC_INTERROMPIDA, None, Instant::now());
                }
            }
            Comando::Estado(tx) => {
                let _ = tx.send(Estado {
                    falando: self.atual.as_ref().map(|a| a.item.id),
                    fila: self.fila.len() as u32,
                    lock_preso: self.guarda.is_some(),
                });
            }
        }
    }

    /// Para a fala atual (se houver) e responde rc 143.
    async fn calar_atual(&mut self) {
        if let Some(a) = self.atual.take() {
            drop(a.fut); // cancela sintese, tocar e `say` (kill_on_drop)
            calar_saida(self.ctx.saida.clone(), self.ctx.cfg.fade_ms).await;
            self.fim(a.item, RC_INTERROMPIDA, Some(&a.prog), a.inicio);
            self.soltar_se_vazio();
        }
    }

    /// O guardiao morreu: a Escuta fez barge-in. Cala e limpa a fila toda.
    async fn barge_in(&mut self) {
        tracing::info!("guardiao do lock morreu: barge-in, limpando a fila");
        self.guarda = None;
        let _ = self.lock_tx.send(false);
        self.calar_atual().await;
        self.encerrar_todas(RC_INTERROMPIDA);
    }

    fn terminou(&mut self, rc: i32) {
        if let Some(a) = self.atual.take() {
            self.fim(a.item, rc, Some(&a.prog), a.inicio);
        }
        self.soltar_se_vazio();
    }

    async fn lock_adquirido(
        &mut self,
        r: Result<Result<GuardaLock, ErroLock>, tokio::task::JoinError>,
    ) {
        match r {
            Ok(Ok(g)) => {
                self.guarda = Some(g);
                let _ = self.lock_tx.send(true);
            }
            // O guardiao morreu antes do "ok": a Escuta ja viu o arquivo e
            // fez barge-in. Mesmo caminho do barge-in: 143 e fila limpa.
            Ok(Err(ErroLock::Morreu)) => self.barge_in().await,
            Ok(Err(ErroLock::Prazo)) => {
                // Prazo do lock esgotado: a fala da vez responde 75.
                if let Some(a) = self.atual.take() {
                    self.fim(a.item, rc::PRAZO_FILA, Some(&a.prog), a.inicio);
                }
            }
            // Nao deu para criar o guardiao (ou a tarefa caiu): erro, rc 1.
            Ok(Err(ErroLock::Spawn(m))) => {
                tracing::warn!("guardiao do lock: {m}");
                self.falha_lock();
            }
            Err(e) => {
                tracing::warn!("tarefa do guardiao caiu: {e}");
                self.falha_lock();
            }
        }
    }

    /// Sem guardiao nao ha fala: a da vez responde rc 1.
    fn falha_lock(&mut self) {
        if let Some(a) = self.atual.take() {
            self.fim(a.item, RC_ERRO, Some(&a.prog), a.inicio);
        }
    }

    /// Falas na fila cujo prazo venceu.
    fn expirar(&mut self) {
        let agora = Instant::now();
        let (vencidas, resto): (Vec<_>, Vec<_>) = std::mem::take(&mut self.fila)
            .into_iter()
            .partition(|i| i.limite() <= agora);
        self.fila = resto.into();
        for item in vencidas {
            self.fim(item, rc::PRAZO_FILA, None, agora);
        }
    }

    fn encerrar_todas(&mut self, rc: i32) {
        let agora = Instant::now();
        for item in std::mem::take(&mut self.fila) {
            self.fim(item, rc, None, agora);
        }
    }

    /// Fila vazia e ninguem falando: solta o guardiao (o som ja esvaziou).
    fn soltar_se_vazio(&mut self) {
        if self.atual.is_none() && self.fila.is_empty() {
            if let Some(h) = self.adquirindo.take() {
                h.abort();
            }
            if self.guarda.take().is_some() {
                let _ = self.lock_tx.send(false);
            }
        }
    }

    /// Manda o `fim` e escreve a linha do log.
    fn fim(&self, item: Item, rc: i32, prog: Option<&Arc<Mutex<Progresso>>>, inicio: Instant) {
        let (motor, audio_s, kokoro, ttfa_ms) = match prog {
            Some(p) => {
                let p = p.lock().unwrap_or_else(|x| x.into_inner());
                (p.motor(), p.audio_s, p.kokoro, p.ttfa_ms)
            }
            None => ("?", 0.0, false, None),
        };
        let _ = item.ev.send(Evento::Fim {
            fala: item.id,
            rc,
            motor: motor.to_owned(),
            audio_s,
            chars: item.chars,
        });
        let linha = log::formatar(
            SystemTime::now(),
            std::process::id(),
            &log::Registro {
                motor,
                chars: item.chars,
                audio_s: kokoro.then_some(audio_s),
                rc,
                dur_s: inicio.elapsed().as_secs(),
                ttfa_ms,
            },
        );
        log::anexar(&self.ctx.cfg.fala_log, &linha);
    }
}

async fn calar_saida<A: Saida + 'static>(saida: Arc<A>, fade_ms: u32) {
    let _ = tokio::task::spawn_blocking(move || saida.calar(fade_ms)).await;
}
