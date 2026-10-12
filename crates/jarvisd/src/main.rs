//! `jarvisd`: daemon residente do Jarvis (POC em Rust).
//!
//! `jarvisd --segurar-lock <caminho>` e o modo guardiao do lock (o proprio
//! executavel e o guardiao). Sem argumentos, sobe o servidor.

mod saida;
mod servidor;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use jarvis_fala::{Config, Fila};
use jarvis_kokoro::{KokoroHttp, URL_PADRAO};
use jarvis_lock::{GuardiaoProcesso, rodar_guardiao};
use jarvis_protocolo::{Escopo, Manter};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::broadcast;

use crate::saida::SaidaDaemon;

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty())
}

fn poc_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share/jarvis/poc")
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "--segurar-lock") {
        let Some(caminho) = args.next() else {
            eprintln!("uso: jarvisd --segurar-lock <caminho>");
            std::process::exit(2);
        };
        std::process::exit(rodar_guardiao(std::path::Path::new(&caminho)));
    }
    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(servir())
}

async fn servir() -> anyhow::Result<()> {
    let sock = env("JARVISD_SOCK").map_or_else(|| poc_dir().join("jarvisd.sock"), PathBuf::from);
    let lock = env("JARVISD_LOCK").map_or_else(|| poc_dir().join("fala.lock"), PathBuf::from);
    let log_dir = env("JARVISD_LOG_DIR").map_or_else(|| poc_dir().join("logs"), PathBuf::from);
    let tts = env("JARVIS_TTS_URL").unwrap_or_else(|| URL_PADRAO.to_owned());
    let manter = match env("JARVISD_MANTER") {
        Some(v) => v.parse::<Manter>().map_err(anyhow::Error::msg)?,
        None => Manter::Sempre,
    };

    std::fs::create_dir_all(&log_dir)?;
    if let Some(pai) = lock.parent() {
        std::fs::create_dir_all(pai)?;
    }
    let arq = std::fs::OpenOptions::new().create(true).append(true).open(log_dir.join("jarvisd.log"))?;
    tracing_subscriber::fmt().with_writer(std::sync::Mutex::new(arq)).with_ansi(false).init();

    let saida = match env("JARVISD_SAIDA").as_deref() {
        Some("fake") => SaidaDaemon::fake(),
        None | Some("cpal") => SaidaDaemon::cpal().map_err(anyhow::Error::msg)?,
        Some(o) => anyhow::bail!("JARVISD_SAIDA invalido: {o:?} (cpal|fake)"),
    };

    let mut cfg = Config::do_ambiente();
    // O fala.log da POC nunca e o de producao.
    cfg.fala_log = log_dir.join("fala.log");
    let kokoro = KokoroHttp::new(tts)?;
    let guardiao = GuardiaoProcesso::new(std::env::current_exe()?, vec!["--segurar-lock".into()], &lock);
    let fila = Fila::iniciar(kokoro.clone(), saida.clone(), guardiao, cfg);

    let ctx = Arc::new(servidor::Ctx { fila, kokoro, saida, manter, eventos: broadcast::channel(256).0 });
    let listener = servidor::ligar(&sock)?;
    tracing::info!(sock = %sock.display(), lock = %lock.display(), "jarvisd no ar");

    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    tokio::select! {
        () = servidor::aceitar(&listener, &ctx) => {}
        _ = term.recv() => tracing::info!("SIGTERM"),
        _ = int.recv() => tracing::info!("SIGINT"),
    }

    // Encerramento: cala tudo, espera o guardiao soltar, limpa o socket.
    let _ = ctx.fila.calar(Escopo::Tudo).await;
    for _ in 0..40 {
        match ctx.fila.estado().await {
            Ok(e) if e.lock_preso => tokio::time::sleep(Duration::from_millis(25)).await,
            _ => break,
        }
    }
    tokio::time::sleep(Duration::from_millis(50)).await; // `fim` 143 chegarem aos clientes
    drop(listener);
    let _ = std::fs::remove_file(&sock);
    tracing::info!("jarvisd encerrado");
    Ok(())
}
