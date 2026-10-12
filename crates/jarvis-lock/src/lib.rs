//! Lock compativel com o `lockf` do jarvis-fala shell (F3).
//!
//! O processo chamador nunca segura o fd do lock: quem segura e um processo
//! guardiao (`rodar_guardiao`), criado por `posix_spawn` (via `std::process`,
//! que usa `posix_spawn` no macOS quando nao ha `pre_exec`). Assim o barge-in
//! da Escuta (`lsof -t` + SIGTERM) mata so o guardiao, nunca o daemon.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::future::Future;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use rustix::fs::{FlockOperation, flock};
use rustix::io::Errno;
use rustix::process::{Pid, Signal, kill_process};
use thiserror::Error;
use tokio::sync::{oneshot, watch};

/// O prazo acabou sem conseguir o lock (equivale ao rc 75 do `lockf -t`).
#[derive(Debug, Error)]
#[error("prazo esgotado esperando o lock")]
pub struct Esgotou;

/// Codigo de saida equivalente a `Esgotou`.
pub const RC_ESGOTOU: i32 = 75;

/// Lock seguro. Ao soltar (drop), libera matando o guardiao. Se o guardiao
/// morrer, [`GuardaLock::morreu`] completa.
#[derive(Debug)]
pub struct GuardaLock {
    morte: watch::Receiver<bool>,
    pid: Option<u32>,
}

impl GuardaLock {
    /// Sem processo real (fakes).
    pub fn new(morte: watch::Receiver<bool>) -> Self {
        Self { morte, pid: None }
    }

    /// Completa quando o guardiao morre (ou o lado emissor some).
    pub fn morreu(&self) -> impl Future<Output = ()> + Send + 'static {
        let mut rx = self.morte.clone();
        async move {
            let _ = rx.wait_for(|m| *m).await;
        }
    }

    /// Pid do guardiao, se houver.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
}

impl Drop for GuardaLock {
    fn drop(&mut self) {
        if let Some(pid) = self.pid
            && !*self.morte.borrow()
            && let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw)
        {
            let _ = kill_process(pid, Signal::KILL);
        }
    }
}

pub trait Guardiao: Send + Sync {
    fn segurar(&self, prazo: Duration) -> impl Future<Output = Result<GuardaLock, Esgotou>> + Send;
}

/// Guardiao real: cria `binario args... <caminho>` e espera o "ok".
#[derive(Debug, Clone)]
pub struct GuardiaoProcesso {
    binario: PathBuf,
    args: Vec<OsString>,
    caminho: PathBuf,
}

impl GuardiaoProcesso {
    /// `args` vem antes do caminho (ex.: `["--segurar-lock"]` para o jarvisd).
    pub fn new(
        binario: impl Into<PathBuf>,
        args: Vec<OsString>,
        caminho: impl Into<PathBuf>,
    ) -> Self {
        Self {
            binario: binario.into(),
            args,
            caminho: caminho.into(),
        }
    }
}

impl Guardiao for GuardiaoProcesso {
    async fn segurar(&self, prazo: Duration) -> Result<GuardaLock, Esgotou> {
        let mut filho = Command::new(&self.binario)
            .args(&self.args)
            .arg(&self.caminho)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Esgotou)?;
        let pid = filho.id();
        let stdout = filho.stdout.take().ok_or(Esgotou)?;
        let stdin = filho.stdin.take();

        let (ok_tx, ok_rx) = oneshot::channel::<bool>();
        std::thread::spawn(move || {
            let mut linha = String::new();
            let ok = BufReader::new(stdout).read_line(&mut linha).is_ok() && linha.trim() == "ok";
            let _ = ok_tx.send(ok);
        });

        let (morte_tx, morte_rx) = watch::channel(false);
        std::thread::spawn(move || {
            let _stdin = stdin; // fechado ao morrer: o guardiao sai se o dono sumir
            let _ = filho.wait();
            let _ = morte_tx.send(true);
        });

        // Se o futuro for abandonado ou o prazo vencer, o guarda mata o filho.
        let guarda = GuardaLock {
            morte: morte_rx,
            pid: Some(pid),
        };
        match tokio::time::timeout(prazo, ok_rx).await {
            Ok(Ok(true)) => Ok(guarda),
            _ => Err(Esgotou),
        }
    }
}

/// Modo guardiao (`jarvisd --segurar-lock <caminho>`): abre o arquivo, toma
/// `flock(LOCK_EX)` (bloqueante; o prazo e do chamador), escreve "ok" no
/// stdout e fica parado ate o stdin fechar ou ate ser morto. Devolve o codigo
/// de saida.
pub fn rodar_guardiao(caminho: &Path) -> i32 {
    let arquivo = match OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o644)
        .custom_flags(rustix::fs::OFlags::CLOEXEC.bits() as i32)
        .open(caminho)
    {
        Ok(a) => a,
        Err(_) => return 1,
    };
    loop {
        match flock(&arquivo, FlockOperation::LockExclusive) {
            Ok(()) => break,
            Err(Errno::INTR) => continue,
            Err(_) => return 1,
        }
    }
    {
        use std::io::Write;
        let mut out = std::io::stdout();
        if out.write_all(b"ok\n").and_then(|_| out.flush()).is_err() {
            return 1;
        }
    }
    // Parado: so sai quando o dono fechar o stdin (ou por sinal).
    let mut buf = [0u8; 64];
    let mut stdin = std::io::stdin();
    while matches!(stdin.read(&mut buf), Ok(n) if n > 0) {}
    0
}

/// Mesma semantica de `SpeechLock.isHeld` da Escuta: true se alguem segura o
/// lock (flock compartilhado nao bloqueante falha com EWOULDBLOCK).
pub fn esta_preso(caminho: &Path) -> bool {
    let Ok(arquivo) = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(caminho)
    else {
        return false;
    };
    match flock(&arquivo, FlockOperation::NonBlockingLockShared) {
        Ok(()) => {
            let _ = flock(&arquivo, FlockOperation::Unlock);
            false
        }
        Err(e) => e == Errno::WOULDBLOCK,
    }
}
