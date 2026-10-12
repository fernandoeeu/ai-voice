//! Execucao de uma fala: frases com lookahead de 1, fallback `say`.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use jarvis_audio::{EventoSaida, Saida};
use jarvis_kokoro::{Sintetizador, dividir_frases};
use jarvis_protocolo::{Evento, FalaId, Pcm24k};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc, watch};

use crate::config::Config;

/// O que ja aconteceu na fala (lido para o `fim` e para o log, mesmo se a
/// execucao for cancelada no meio).
#[derive(Debug, Default)]
pub(crate) struct Progresso {
    pub kokoro: bool,
    pub say: bool,
    pub audio_s: f64,
    pub ttfa_ms: Option<u64>,
}

impl Progresso {
    pub fn motor(&self) -> &'static str {
        match (self.kokoro, self.say) {
            (true, false) => "kokoro",
            (false, true) => "say",
            (true, true) => "kokoro+say",
            _ => "?",
        }
    }
}

pub(crate) struct Ctx<S, A> {
    pub synth: Arc<S>,
    pub saida: Arc<A>,
    pub cfg: Arc<Config>,
}

impl<S, A> Clone for Ctx<S, A> {
    fn clone(&self) -> Self {
        Self {
            synth: self.synth.clone(),
            saida: self.saida.clone(),
            cfg: self.cfg.clone(),
        }
    }
}

pub(crate) struct Entrada {
    pub id: FalaId,
    pub texto: String,
    pub voz: String,
    pub vel: f32,
    pub lock: watch::Receiver<bool>,
    pub prog: Arc<Mutex<Progresso>>,
    pub ev: mpsc::UnboundedSender<Evento>,
}

fn marca_primeiro_som(e: &Entrada, ms: u64) {
    let mut p = e.prog.lock().unwrap_or_else(|x| x.into_inner());
    if p.ttfa_ms.is_none() {
        p.ttfa_ms = Some(ms);
        drop(p);
        let _ = e.ev.send(Evento::PrimeiroSom { fala: e.id, ms });
    }
}

async fn tocar<A: Saida + 'static>(saida: &Arc<A>, id: FalaId, pcm: Pcm24k, ultimo: bool) {
    let saida = saida.clone();
    // `tocar` pode bloquear por backpressure do ring buffer.
    let _ = tokio::task::spawn_blocking(move || saida.tocar(id, pcm, ultimo)).await;
}

/// Fala `texto` com o `say`. Devolve o rc.
async fn say<S, A>(ctx: &Ctx<S, A>, e: &Entrada, t0: Instant, texto: &str) -> i32 {
    e.prog.lock().unwrap_or_else(|x| x.into_inner()).say = true;
    let filho = Command::new(&ctx.cfg.say_exe)
        .arg("-v")
        .arg(&ctx.cfg.voz_mac)
        .arg("--")
        .arg(texto)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut filho = match filho {
        Ok(f) => f,
        Err(err) => {
            tracing::warn!("say: {err}");
            return crate::ator::RC_ERRO;
        }
    };
    marca_primeiro_som(e, t0.elapsed().as_millis() as u64);
    match filho.wait().await {
        Ok(st) => st.code().unwrap_or(crate::ator::RC_INTERROMPIDA),
        Err(_) => crate::ator::RC_ERRO,
    }
}

struct Produzido {
    /// Texto que sobrou sem sintetizar (Kokoro falhou no meio).
    resto: Option<String>,
}

/// Roda a fala inteira e devolve o rc. Cancelavel por drop.
pub(crate) async fn executar<S, A>(ctx: Ctx<S, A>, e: Entrada) -> i32
where
    S: Sintetizador + 'static,
    A: Saida + 'static,
{
    let t0 = Instant::now();
    let frases = dividir_frases(&e.texto, ctx.cfg.primeira_curta);
    if frases.is_empty() {
        return crate::ator::RC_OK;
    }
    // Assina antes de tocar para nao perder `PrimeiroSom`/`Esvaziou`.
    let mut eventos = ctx.saida.eventos();

    // Guardiao e primeira sintese em paralelo.
    let mut lock = e.lock.clone();
    let esperar_lock = async move { lock.wait_for(|v| *v).await.is_ok() };
    let (p0, lock_ok) = tokio::join!(
        ctx.synth.sintetizar(&frases[0], &e.voz, e.vel),
        esperar_lock
    );
    if !lock_ok {
        return crate::ator::RC_ERRO;
    }
    let p0 = match p0 {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!("kokoro falhou na primeira frase, usando say: {err:#}");
            let texto = e.texto.clone();
            return say(&ctx, &e, t0, &texto).await;
        }
    };

    let id = e.id;
    let produtor = async {
        let mut atual = p0;
        let mut i = 0;
        let n = frases.len();
        loop {
            let ultimo = i + 1 == n;
            let dur = atual.duracao_s();
            {
                let mut p = e.prog.lock().unwrap_or_else(|x| x.into_inner());
                p.kokoro = true;
                p.audio_s += dur;
            }
            if ultimo {
                tocar(&ctx.saida, id, atual, true).await;
                return Produzido { resto: None };
            }
            let (_, proxima) = tokio::join!(
                tocar(&ctx.saida, id, atual, false),
                ctx.synth.sintetizar(&frases[i + 1], &e.voz, e.vel)
            );
            match proxima {
                Ok(pcm) => {
                    atual = pcm;
                    i += 1;
                }
                Err(err) => {
                    tracing::warn!("kokoro falhou na frase {}, say no resto: {err:#}", i + 2);
                    // Fecha o bloco com um pedaco vazio para a saida avisar `Esvaziou`.
                    tocar(&ctx.saida, id, Pcm24k::new(Vec::new()), true).await;
                    return Produzido {
                        resto: Some(frases[i + 1..].join(" ")),
                    };
                }
            }
        }
    };
    tokio::pin!(produtor);

    let mut produzido: Option<Produzido> = None;
    let mut esvaziou = false;
    while produzido.is_none() || !esvaziou {
        tokio::select! {
            p = &mut produtor, if produzido.is_none() => produzido = Some(p),
            ev = eventos.recv() => match ev {
                Ok(EventoSaida::PrimeiroSom { fala, hora_saida }) if fala == id => {
                    let ms = hora_saida.saturating_duration_since(t0).as_millis() as u64;
                    marca_primeiro_som(&e, ms);
                }
                Ok(EventoSaida::Esvaziou { fala }) if fala == id => esvaziou = true,
                Ok(EventoSaida::Erro { mensagem }) => tracing::warn!("saida: {mensagem}"),
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return crate::ator::RC_ERRO,
            },
        }
    }
    match produzido.and_then(|p| p.resto) {
        Some(resto) => say(&ctx, &e, t0, &resto).await,
        None => crate::ator::RC_OK,
    }
}
