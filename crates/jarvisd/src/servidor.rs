//! Servidor do socket Unix: NDJSON v1, varias conexoes, varias mensagens por conexao.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jarvis_fala::{Fila, Pedido};
use jarvis_kokoro::{KokoroHttp, Sintetizador};
use jarvis_protocolo::{
    CodigoErro, Esperar, EstadoDaemon, Evento, MAX_LINHA, Manter, Mensagem, Op, Requisicao,
    codificar, decodificar,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, watch};

use crate::saida::SaidaDaemon;

pub struct Ctx {
    pub fila: Fila,
    pub kokoro: KokoroHttp,
    pub saida: SaidaDaemon,
    pub manter: Manter,
    pub eventos: broadcast::Sender<Evento>,
}

type Tx = mpsc::UnboundedSender<String>;

/// Lock de instancia unica; fica aberto (O_CLOEXEC) enquanto o processo vive.
/// Nunca e o fala.lock.
static LOCK_INSTANCIA: std::sync::OnceLock<std::fs::File> = std::sync::OnceLock::new();

fn travar_instancia(caminho: &Path) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut nome = caminho.as_os_str().to_owned();
    nome.push(".pid");
    let pid = std::path::PathBuf::from(nome);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::CLOEXEC.bits() as i32)
        .open(&pid)?;
    if rustix::fs::flock(&f, rustix::fs::FlockOperation::NonBlockingLockExclusive).is_err() {
        anyhow::bail!(
            "ja ha um jarvisd rodando (lock de instancia ocupado em {})",
            pid.display()
        );
    }
    f.set_len(0)?;
    writeln!(f, "{}", std::process::id())?;
    let _ = LOCK_INSTANCIA.set(f);
    Ok(())
}

/// Cria a pasta (0700, so se for nova), trava a instancia, remove socket velho
/// (so se for socket e ninguem atender) e faz o bind (0600).
pub fn ligar(caminho: &Path) -> anyhow::Result<UnixListener> {
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
    if let Some(pai) = caminho.parent().filter(|p| !p.as_os_str().is_empty()) {
        if pai.exists() {
            let m = std::fs::metadata(pai)?;
            if m.uid() == rustix::process::getuid().as_raw() && m.mode() & 0o077 != 0 {
                tracing::warn!(
                    pasta = %pai.display(),
                    modo = format!("{:o}", m.mode() & 0o777),
                    "pasta do socket pre-existente com permissao mais aberta que 0700; mantida"
                );
            }
        } else {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(pai)?;
        }
    }
    travar_instancia(caminho)?;
    if let Ok(m) = std::fs::symlink_metadata(caminho) {
        if !m.file_type().is_socket() {
            anyhow::bail!(
                "{} existe e nao e um socket; recusando apagar",
                caminho.display()
            );
        }
        if std::os::unix::net::UnixStream::connect(caminho).is_ok() {
            anyhow::bail!("ja ha um jarvisd atendendo em {}", caminho.display());
        }
        std::fs::remove_file(caminho)?;
    }
    let l = UnixListener::bind(caminho)?;
    std::fs::set_permissions(caminho, std::fs::Permissions::from_mode(0o600))?;
    Ok(l)
}

pub async fn aceitar(l: &UnixListener, ctx: &Arc<Ctx>) {
    let meu = rustix::process::getuid().as_raw();
    loop {
        let Ok((s, _)) = l.accept().await else {
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        };
        match s.peer_cred() {
            Ok(c) if c.uid() == meu => {
                tokio::spawn(conexao(s, ctx.clone()));
            }
            outro => tracing::warn!("conexao recusada (uid do par: {outro:?})"),
        }
    }
}

fn enviar(tx: &Tx, id: Option<&str>, ev: Evento) {
    if let Ok(l) = codificar(&Mensagem::nova(id.map(str::to_owned), ev)) {
        let _ = tx.send(l);
    }
}

fn erro(tx: &Tx, id: Option<&str>, msg: impl Into<String>) {
    enviar(
        tx,
        id,
        Evento::Erro {
            codigo: CodigoErro::Protocolo,
            mensagem: msg.into(),
        },
    );
}

async fn conexao(s: UnixStream, ctx: Arc<Ctx>) {
    let (rd, mut wr) = s.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let escritor = tokio::spawn(async move {
        while let Some(l) = rx.recv().await {
            if wr.write_all(l.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let (desc_tx, desc_rx) = watch::channel(false);
    let mut leitor = BufReader::new(rd);
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = match (&mut leitor)
            .take(MAX_LINHA as u64 + 2)
            .read_line(&mut buf)
            .await
        {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if !buf.ends_with('\n') {
            if n > MAX_LINHA {
                erro(&tx, None, "linha grande demais");
            }
            break; // EOF no meio de uma linha ou linha gigante
        }
        if buf.trim().is_empty() {
            continue;
        }
        match decodificar::<Requisicao>(&buf) {
            Ok(req) => tratar(&ctx, &tx, &desc_rx, req).await,
            Err(e) => {
                let id = id_da_linha(&buf);
                erro(&tx, id.as_deref(), e.to_string());
            }
        }
    }
    let _ = desc_tx.send(true);
    // Deixa as respostas pendentes sairem por um instante antes de fechar.
    drop(tx);
    let _ = tokio::time::timeout(Duration::from_millis(200), escritor).await;
}

/// Tenta recuperar o `id` de uma linha que nao decodificou.
fn id_da_linha(linha: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(linha).ok()?;
    v.get("id")?.as_str().map(str::to_owned)
}

async fn estado(ctx: &Ctx) -> Option<EstadoDaemon> {
    let e = ctx.fila.estado().await.ok()?;
    let saudavel = tokio::time::timeout(Duration::from_secs(1), ctx.kokoro.saudavel())
        .await
        .unwrap_or(false);
    let (dispositivo, taxa) = ctx.saida.dispositivo();
    Some(EstadoDaemon {
        falando: e.falando,
        fila: e.fila,
        kokoro_saudavel: saudavel,
        dispositivo,
        taxa,
        manter: ctx.saida.manter(ctx.manter),
    })
}

async fn tratar(ctx: &Arc<Ctx>, tx: &Tx, desc: &watch::Receiver<bool>, req: Requisicao) {
    let id = Some(req.id.as_str());
    match req.op {
        Op::Ping => enviar(tx, id, Evento::Pong),
        Op::Estado => match estado(ctx).await {
            Some(e) => enviar(tx, id, Evento::Estado(e)),
            None => erro(tx, id, "fila encerrada"),
        },
        Op::Calar { escopo } => {
            let _ = ctx.fila.calar(escopo).await;
            match estado(ctx).await {
                Some(e) => enviar(tx, id, Evento::Estado(e)),
                None => erro(tx, id, "fila encerrada"),
            }
        }
        Op::Assinar => {
            let mut sub = ctx.eventos.subscribe();
            if let Some(e) = estado(ctx).await {
                enviar(tx, id, Evento::Estado(e));
            }
            let tx = tx.clone();
            tokio::spawn(async move {
                loop {
                    match sub.recv().await {
                        Ok(ev) => enviar(&tx, None, ev),
                        Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                    if tx.is_closed() {
                        break;
                    }
                }
            });
        }
        Op::Falar {
            texto,
            voz,
            velocidade,
            origem,
            esperar,
            prazo_fila_s,
            cancelar_ao_desconectar,
        } => {
            let pedido = Pedido {
                texto,
                voz,
                vel: velocidade,
                origem,
                prazo_fila: prazo_fila_s.map(|s| Duration::from_secs(u64::from(s))),
            };
            tokio::spawn(falar(
                ctx.clone(),
                tx.clone(),
                req.id,
                pedido,
                esperar,
                cancelar_ao_desconectar.unwrap_or(false),
                desc.clone(),
            ));
        }
    }
}

async fn falar(
    ctx: Arc<Ctx>,
    tx: Tx,
    req_id: String,
    pedido: Pedido,
    esperar: Esperar,
    cancelar: bool,
    mut desc: watch::Receiver<bool>,
) {
    let id = Some(req_id.as_str());
    let (fala, mut rx) = match ctx.fila.falar(pedido).await {
        Ok(x) => x,
        Err(e) => return erro(&tx, id, e.to_string()),
    };
    let mut cancelado = false;
    // Com `esperar == aceito`, o cancelamento por desconexao vale so ate o `aceito`.
    let mut desacoplada = false;
    loop {
        tokio::select! {
            // Desconexao antes do `aceito` vence um evento pronto: nada foi aceito ainda.
            biased;
            _ = desc.changed(), if cancelar && !cancelado && !desacoplada => {
                cancelado = true;
                tracing::info!(fala = fala.0, "cliente desconectou: cancelando");
                let _ = ctx.fila.cancelar(fala).await;
            }
            ev = rx.recv() => {
                let Some(ev) = ev else { break };
                if let Evento::PrimeiroSom { fala, ms } = &ev {
                    tracing::info!(fala = fala.0, ms, "primeiro_som");
                }
                let fim = matches!(ev, Evento::Fim { .. });
                let _ = ctx.eventos.send(ev.clone());
                let encaminha = match &ev {
                    Evento::Aceito { .. } => true,
                    Evento::PrimeiroSom { .. } => matches!(esperar, Esperar::Fim | Esperar::PrimeiroSom),
                    Evento::Fim { .. } => esperar == Esperar::Fim,
                    _ => false,
                };
                let aceito = matches!(ev, Evento::Aceito { .. });
                if encaminha {
                    enviar(&tx, id, ev);
                }
                if aceito && esperar == Esperar::Aceito {
                    desacoplada = true;
                }
                if fim {
                    break;
                }
            }
        }
    }
}
