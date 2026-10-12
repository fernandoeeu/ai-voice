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
use thiserror::Error;
use tokio::sync::{oneshot, watch};

/// Por que `segurar` nao devolveu o lock.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ErroLock {
    /// O guardiao morreu antes do "ok" (ex.: barge-in da Escuta, que ja ve o
    /// arquivo aberto pelo `lsof` mesmo antes do flock).
    #[error("o guardiao morreu antes de segurar o lock")]
    Morreu,
    /// O prazo acabou sem conseguir o lock (equivale ao rc 75 do `lockf -t`).
    #[error("prazo esgotado esperando o lock")]
    Prazo,
    /// Nao foi possivel criar o processo guardiao.
    #[error("falha ao criar o guardiao: {0}")]
    Spawn(String),
}

/// Codigo de saida de `ErroLock::Prazo` (`lockf -t` esgotado).
pub const RC_ESGOTOU: i32 = 75;

/// Lock seguro. Ao soltar (drop), libera pedindo a thread dona do processo
/// que mate o guardiao. Se o guardiao morrer, [`GuardaLock::morreu`] completa.
#[derive(Debug)]
pub struct GuardaLock {
    morte: watch::Receiver<bool>,
    pid: Option<u32>,
    /// Pedido de kill para a thread dona do `Child` (nunca matamos por pid:
    /// ele pode ter sido reaproveitado depois do `wait`).
    kill: Option<std::sync::mpsc::Sender<Msg>>,
}

#[derive(Debug)]
enum Msg {
    Matar,
    /// O stdout do guardiao fechou: o processo acabou (ou vai acabar).
    Eof,
}

impl GuardaLock {
    /// Sem processo real (fakes).
    pub fn new(morte: watch::Receiver<bool>) -> Self {
        Self {
            morte,
            pid: None,
            kill: None,
        }
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
        if let Some(tx) = &self.kill {
            // Se a thread ja saiu, o processo ja foi colhido: nada a fazer.
            let _ = tx.send(Msg::Matar);
        }
    }
}

pub trait Guardiao: Send + Sync {
    fn segurar(&self, prazo: Duration)
    -> impl Future<Output = Result<GuardaLock, ErroLock>> + Send;
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
    async fn segurar(&self, prazo: Duration) -> Result<GuardaLock, ErroLock> {
        let mut filho = Command::new(&self.binario)
            .args(&self.args)
            .arg(&self.caminho)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ErroLock::Spawn(format!("{}: {e}", self.binario.display())))?;
        let pid = filho.id();
        let (Some(stdout), stdin) = (filho.stdout.take(), filho.stdin.take()) else {
            let _ = filho.kill();
            let _ = filho.wait();
            return Err(ErroLock::Spawn("sem stdout".into()));
        };

        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<Msg>();
        let (ok_tx, ok_rx) = oneshot::channel::<bool>();
        let eof_tx = msg_tx.clone();
        std::thread::spawn(move || {
            let mut leitor = BufReader::new(stdout);
            let mut linha = String::new();
            let ok = leitor.read_line(&mut linha).is_ok() && linha.trim() == "ok";
            let _ = ok_tx.send(ok);
            // Segue lendo ate o EOF: so acontece quando o guardiao morre.
            let mut lixo = Vec::new();
            while matches!(leitor.read_until(b'\n', &mut lixo), Ok(n) if n > 0) {
                lixo.clear();
            }
            let _ = eof_tx.send(Msg::Eof);
        });

        // Esta thread e a unica dona do `Child`: mata e colhe. O Drop do
        // `GuardaLock` so manda o pedido.
        let (morte_tx, morte_rx) = watch::channel(false);
        std::thread::spawn(move || {
            let _stdin = stdin; // fechado ao morrer: o guardiao sai se o dono sumir
            let _ = msg_rx.recv(); // Matar, Eof ou todos os emissores sumiram
            let _ = filho.kill(); // no-op se ja saiu (e o pid so e liberado no wait)
            let _ = filho.wait();
            let _ = morte_tx.send(true);
        });

        // Se o futuro for abandonado ou o prazo vencer, o guarda mata o filho.
        let guarda = GuardaLock {
            morte: morte_rx,
            pid: Some(pid),
            kill: Some(msg_tx),
        };
        match tokio::time::timeout(prazo, ok_rx).await {
            Ok(Ok(true)) => Ok(guarda),
            Ok(_) => Err(ErroLock::Morreu),
            Err(_) => Err(ErroLock::Prazo),
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
