//! Lock compativel com o `lockf` do jarvis-fala shell (F3). Esqueleto da F0.

use std::future::Future;
use std::time::Duration;

use thiserror::Error;
use tokio::sync::watch;

/// O prazo acabou sem conseguir o lock.
#[derive(Debug, Error)]
#[error("prazo esgotado esperando o lock")]
pub struct Esgotou;

/// Lock seguro. Ao soltar (drop), libera. Se o guardiao (processo que segura
/// o lock de verdade) morrer, [`GuardaLock::morreu`] completa.
#[derive(Debug)]
pub struct GuardaLock {
    morte: watch::Receiver<bool>,
}

impl GuardaLock {
    pub fn new(morte: watch::Receiver<bool>) -> Self {
        Self { morte }
    }

    /// Completa quando o guardiao morre (ou o lado emissor some).
    pub fn morreu(&self) -> impl Future<Output = ()> + Send + 'static {
        let mut rx = self.morte.clone();
        async move {
            let _ = rx.wait_for(|m| *m).await;
        }
    }
}

pub trait Guardiao: Send + Sync {
    fn segurar(&self, prazo: Duration) -> impl Future<Output = Result<GuardaLock, Esgotou>> + Send;
}
