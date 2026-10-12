//! Testes da fila com fakes proprios de `Sintetizador`, `Saida` e `Guardiao`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jarvis_audio::{EventoSaida, Saida};
use jarvis_fala::{Config, Escopo, Evento, FalaId, Fila, Pedido};
use jarvis_kokoro::{Result as KResult, Sintetizador};
use jarvis_lock::{Esgotou, GuardaLock, Guardiao};
use jarvis_protocolo::Pcm24k;
use tokio::sync::{broadcast, mpsc, watch};

const TEXTO3: &str = "Primeira frase de teste aqui. Segunda frase de teste aqui. Terceira frase de teste aqui.";

// ---------- fakes ----------

#[derive(Default)]
struct SintInner {
    atrasos: Mutex<HashMap<String, Duration>>,
    falhas: Mutex<Vec<String>>,
    log: Mutex<Vec<(String, Instant)>>,
}
#[derive(Clone, Default)]
struct FakeSint(Arc<SintInner>);

impl FakeSint {
    fn atrasar(&self, trecho: &str, d: Duration) {
        self.0.atrasos.lock().unwrap().insert(trecho.into(), d);
    }
    fn falhar(&self, trecho: &str) {
        self.0.falhas.lock().unwrap().push(trecho.into());
    }
    fn fim_de(&self, trecho: &str) -> Instant {
        self.0.log.lock().unwrap().iter().find(|(f, _)| f.contains(trecho)).expect("sintese nao ocorreu").1
    }
}
impl Sintetizador for FakeSint {
    async fn sintetizar(&self, frase: &str, _voz: &str, _vel: f32) -> KResult<Pcm24k> {
        let atraso = self.0.atrasos.lock().unwrap().iter().find(|(k, _)| frase.contains(k.as_str())).map(|(_, v)| *v);
        if let Some(d) = atraso {
            tokio::time::sleep(d).await;
        }
        if self.0.falhas.lock().unwrap().iter().any(|k| frase.contains(k.as_str())) {
            anyhow::bail!("kokoro falso falhou");
        }
        self.0.log.lock().unwrap().push((frase.to_owned(), Instant::now()));
        Ok(Pcm24k::new(vec![100; 2400])) // 0,1 s
    }
    async fn saudavel(&self) -> bool {
        true
    }
}

struct SaidaInner {
    tx: broadcast::Sender<EventoSaida>,
    tocadas: Mutex<Vec<(FalaId, usize, bool, Instant)>>,
    auto_esvaziar: Mutex<Option<Duration>>,
    bloquear: AtomicBool,
    calares: AtomicU32,
    epoca: AtomicU64,
}
#[derive(Clone)]
struct FakeSaida(Arc<SaidaInner>);

impl FakeSaida {
    fn novo(auto: Option<Duration>) -> Self {
        Self(Arc::new(SaidaInner {
            tx: broadcast::channel(64).0,
            tocadas: Mutex::default(),
            auto_esvaziar: Mutex::new(auto),
            bloquear: AtomicBool::new(false),
            calares: AtomicU32::new(0),
            epoca: AtomicU64::new(0),
        }))
    }
    fn esvaziar(&self, fala: FalaId) {
        let _ = self.0.tx.send(EventoSaida::Esvaziou { fala });
    }
    fn n_tocadas(&self) -> usize {
        self.0.tocadas.lock().unwrap().len()
    }
}
impl Saida for FakeSaida {
    fn tocar(&self, fala: FalaId, pcm: Pcm24k, ultimo: bool) {
        // Simula a backpressure do ring: bloqueia ate o `calar`.
        let epoca = self.0.epoca.load(SeqCst);
        while self.0.bloquear.load(SeqCst) && self.0.epoca.load(SeqCst) == epoca {
            std::thread::sleep(Duration::from_millis(1));
        }
        if self.0.epoca.load(SeqCst) != epoca {
            return;
        }
        let primeiro = {
            let mut t = self.0.tocadas.lock().unwrap();
            let primeiro = !t.iter().any(|(f, n, _, _)| *f == fala && *n > 0) && !pcm.amostras.is_empty();
            t.push((fala, pcm.amostras.len(), ultimo, Instant::now()));
            primeiro
        };
        if primeiro {
            let _ = self.0.tx.send(EventoSaida::PrimeiroSom { fala, hora_saida: Instant::now() });
        }
        if ultimo && let Some(d) = *self.0.auto_esvaziar.lock().unwrap() {
            let (tx, ep, base) = (self.0.tx.clone(), self.0.epoca.load(SeqCst), self.0.clone());
            std::thread::spawn(move || {
                std::thread::sleep(d);
                if base.epoca.load(SeqCst) == ep {
                    let _ = tx.send(EventoSaida::Esvaziou { fala });
                }
            });
        }
    }
    fn calar(&self, _fade_ms: u32) {
        self.0.calares.fetch_add(1, SeqCst);
        self.0.epoca.fetch_add(1, SeqCst);
    }
    fn eventos(&self) -> broadcast::Receiver<EventoSaida> {
        self.0.tx.subscribe()
    }
}

#[derive(Default)]
struct GuardInner {
    nunca: AtomicBool,
    chamadas: AtomicU32,
    guardas: Mutex<Vec<watch::Sender<bool>>>,
}
#[derive(Clone, Default)]
struct FakeGuardiao(Arc<GuardInner>);

impl FakeGuardiao {
    fn solto(&self, n: usize) -> bool {
        self.0.guardas.lock().unwrap()[n].is_closed()
    }
    fn matar(&self, n: usize) {
        let _ = self.0.guardas.lock().unwrap()[n].send(true);
    }
}
impl Guardiao for FakeGuardiao {
    async fn segurar(&self, prazo: Duration) -> Result<GuardaLock, Esgotou> {
        self.0.chamadas.fetch_add(1, SeqCst);
        if self.0.nunca.load(SeqCst) {
            tokio::time::sleep(prazo).await;
            return Err(Esgotou);
        }
        let (tx, rx) = watch::channel(false);
        self.0.guardas.lock().unwrap().push(tx);
        Ok(GuardaLock::new(rx))
    }
}

// ---------- apoio ----------

struct Cena {
    fila: Fila,
    sint: FakeSint,
    saida: FakeSaida,
    guard: FakeGuardiao,
    dir: PathBuf,
}

fn dir_temp() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!("jfala-{}-{}", std::process::id(), N.fetch_add(1, SeqCst)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn cena(auto: Option<Duration>) -> Cena {
    let dir = dir_temp();
    let say = dir.join("say.sh");
    std::fs::write(&say, format!("#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > {}/say.args\n", dir.display())).unwrap();
    std::process::Command::new("chmod").arg("755").arg(&say).status().unwrap();
    let cfg = Config { say_exe: say, fala_log: dir.join("fala.log"), voz_mac: "Luciana".into(), ..Config::default() };
    let (sint, saida, guard) = (FakeSint::default(), FakeSaida::novo(auto), FakeGuardiao::default());
    let fila = Fila::iniciar(sint.clone(), saida.clone(), guard.clone(), cfg);
    Cena { fila, sint, saida, guard, dir }
}

async fn ate_fim(rx: &mut mpsc::UnboundedReceiver<Evento>) -> Vec<Evento> {
    let mut v = Vec::new();
    loop {
        let e = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.expect("timeout esperando fim").expect("canal fechou");
        let fim = matches!(e, Evento::Fim { .. });
        v.push(e);
        if fim {
            return v;
        }
    }
}

fn rc_de(v: &[Evento]) -> i32 {
    match v.last() {
        Some(Evento::Fim { rc, .. }) => *rc,
        o => panic!("sem fim: {o:?}"),
    }
}

async fn espera(mut cond: impl FnMut() -> bool) {
    for _ in 0..500 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condicao nao ocorreu");
}

// ---------- testes ----------

#[tokio::test]
async fn fifo_e_eventos() {
    let c = cena(Some(Duration::from_millis(20)));
    let (a, mut ra) = c.fila.falar(Pedido::novo(TEXTO3)).await.unwrap();
    let (b, mut rb) = c.fila.falar(Pedido::novo("Outra fala curta.")).await.unwrap();
    assert!(matches!(ra.recv().await, Some(Evento::Aceito { posicao: 0, .. })));
    let va = ate_fim(&mut ra).await;
    let vb = ate_fim(&mut rb).await;
    assert!(va.iter().any(|e| matches!(e, Evento::PrimeiroSom { .. })));
    assert_eq!(rc_de(&va), 0);
    assert_eq!(rc_de(&vb), 0);
    let ordem: Vec<FalaId> = c.saida.0.tocadas.lock().unwrap().iter().map(|t| t.0).collect();
    let ultima_a = ordem.iter().rposition(|f| *f == a).unwrap();
    let primeira_b = ordem.iter().position(|f| *f == b).unwrap();
    assert!(ultima_a < primeira_b, "FIFO violada: {ordem:?}");
    match va.last() {
        Some(Evento::Fim { motor, audio_s, chars, .. }) => {
            assert_eq!(motor, "kokoro");
            assert!(*audio_s > 0.25);
            assert_eq!(*chars as usize, TEXTO3.chars().count());
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn primeiro_som_antes_da_sintese_da_segunda_frase() {
    let c = cena(Some(Duration::from_millis(10)));
    c.sint.atrasar("Segunda", Duration::from_millis(300));
    let (_, mut rx) = c.fila.falar(Pedido::novo(TEXTO3)).await.unwrap();
    assert_eq!(rc_de(&ate_fim(&mut rx).await), 0);
    let primeiro_tocar = c.saida.0.tocadas.lock().unwrap()[0].3;
    assert!(primeiro_tocar < c.sint.fim_de("Segunda"), "tocou so depois de sintetizar a 2a frase");
}

#[tokio::test]
async fn lock_preso_entre_frases_e_entre_falas_e_solto_so_apos_esvaziou() {
    let c = cena(None); // Esvaziou manual
    let (a, mut ra) = c.fila.falar(Pedido::novo(TEXTO3)).await.unwrap();
    let (b, mut rb) = c.fila.falar(Pedido::novo("Segunda fala.")).await.unwrap();
    espera(|| c.saida.n_tocadas() >= 3).await; // todas as frases de A foram entregues
    assert_eq!(c.guard.0.chamadas.load(SeqCst), 1);
    assert!(!c.guard.solto(0), "lock solto entre frases");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(rb.try_recv().is_ok(), "B deveria ter so o Aceito");
    // Ultimo bloco entregue mas sem Esvaziou: nada de fim, lock preso.
    assert!(ra.try_recv().is_ok_and(|e| matches!(e, Evento::Aceito { .. })));
    while let Ok(e) = ra.try_recv() {
        assert!(!matches!(e, Evento::Fim { .. }), "fim antes do Esvaziou");
    }
    assert!(!c.guard.solto(0));
    c.saida.esvaziar(a);
    assert_eq!(rc_de(&ate_fim(&mut ra).await), 0);
    // Entre falas: B esta na fila, o lock continua o mesmo e preso.
    espera(|| c.saida.0.tocadas.lock().unwrap().iter().any(|t| t.0 == b)).await;
    assert!(!c.guard.solto(0), "lock solto entre falas");
    assert_eq!(c.guard.0.chamadas.load(SeqCst), 1);
    c.saida.esvaziar(b);
    assert_eq!(rc_de(&ate_fim(&mut rb).await), 0);
    espera(|| c.guard.solto(0)).await;
}

#[tokio::test]
async fn barge_in_cala_e_limpa_a_fila() {
    let c = cena(None);
    let (_, mut ra) = c.fila.falar(Pedido::novo(TEXTO3)).await.unwrap();
    let (_, mut rb) = c.fila.falar(Pedido::novo("Pendente um.")).await.unwrap();
    let (_, mut rc) = c.fila.falar(Pedido::novo("Pendente dois.")).await.unwrap();
    espera(|| c.saida.n_tocadas() >= 1).await;
    c.guard.matar(0);
    for r in [&mut ra, &mut rb, &mut rc] {
        assert_eq!(rc_de(&ate_fim(r).await), 143);
    }
    assert!(c.saida.0.calares.load(SeqCst) >= 1);
    let est = c.fila.estado().await.unwrap();
    assert_eq!((est.falando, est.fila, est.lock_preso), (None, 0, false));
}

#[tokio::test]
async fn calar_atual_e_tudo() {
    let c = cena(None);
    c.saida.0.bloquear.store(true, SeqCst); // tocar bloqueia como o ring cheio
    let (_, mut ra) = c.fila.falar(Pedido::novo(TEXTO3)).await.unwrap();
    let (_, mut rb) = c.fila.falar(Pedido::novo("Pendente.")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    c.fila.calar(Escopo::Atual).await.unwrap();
    assert_eq!(rc_de(&ate_fim(&mut ra).await), 143);
    // B vira a atual e tambem bloqueia; calar tudo derruba.
    tokio::time::sleep(Duration::from_millis(100)).await;
    c.fila.calar(Escopo::Tudo).await.unwrap();
    assert_eq!(rc_de(&ate_fim(&mut rb).await), 143);
    espera(|| c.guard.solto(0)).await;
}

#[tokio::test]
async fn prazo_da_fila_esgotado_da_75() {
    let c = cena(None);
    let (_, mut ra) = c.fila.falar(Pedido::novo("Fala que segura a fila.")).await.unwrap();
    let pedido = Pedido { texto: "Vai vencer.".into(), prazo_fila: Some(Duration::from_millis(100)), ..Pedido::default() };
    let (_, mut rb) = c.fila.falar(pedido).await.unwrap();
    assert_eq!(rc_de(&ate_fim(&mut rb).await), 75);
    assert!(ra.try_recv().is_ok_and(|e| matches!(e, Evento::Aceito { .. })));
}

#[tokio::test]
async fn prazo_do_lock_esgotado_da_75() {
    let c = cena(None);
    c.guard.0.nunca.store(true, SeqCst);
    let pedido = Pedido { texto: "Sem lock.".into(), prazo_fila: Some(Duration::from_millis(100)), ..Pedido::default() };
    let (_, mut rx) = c.fila.falar(pedido).await.unwrap();
    assert_eq!(rc_de(&ate_fim(&mut rx).await), 75);
}

#[tokio::test]
async fn kokoro_falha_na_primeira_frase_usa_say_no_texto_todo() {
    let c = cena(Some(Duration::from_millis(10)));
    c.sint.falhar("Primeira");
    let (_, mut rx) = c.fila.falar(Pedido::novo(TEXTO3)).await.unwrap();
    let v = ate_fim(&mut rx).await;
    assert_eq!(rc_de(&v), 0);
    assert!(matches!(v.last(), Some(Evento::Fim { motor, .. }) if motor == "say"));
    let args = std::fs::read_to_string(c.dir.join("say.args")).unwrap();
    assert_eq!(args.lines().collect::<Vec<_>>(), ["-v", "Luciana", "--", TEXTO3]);
    assert_eq!(c.saida.n_tocadas(), 0);
}

#[tokio::test]
async fn kokoro_falha_no_meio_usa_say_so_no_resto() {
    let c = cena(Some(Duration::from_millis(10)));
    c.sint.falhar("Segunda");
    let (_, mut rx) = c.fila.falar(Pedido::novo(TEXTO3)).await.unwrap();
    let v = ate_fim(&mut rx).await;
    assert_eq!(rc_de(&v), 0);
    assert!(matches!(v.last(), Some(Evento::Fim { motor, .. }) if motor == "kokoro+say"));
    let args = std::fs::read_to_string(c.dir.join("say.args")).unwrap();
    let texto = args.lines().last().unwrap().to_owned();
    assert!(texto.starts_with("Segunda") && texto.contains("Terceira"), "resto errado: {texto}");
    assert!(c.saida.0.tocadas.lock().unwrap().iter().any(|t| t.2 && t.1 == 0), "faltou o fechamento vazio");
}

#[tokio::test]
async fn linha_do_fala_log() {
    let c = cena(Some(Duration::from_millis(10)));
    let (_, mut rx) = c.fila.falar(Pedido::novo("Oi, tudo bem.")).await.unwrap();
    assert_eq!(rc_de(&ate_fim(&mut rx).await), 0);
    let log = std::fs::read_to_string(c.dir.join("fala.log")).unwrap();
    let linha = log.lines().next().unwrap();
    let campos: Vec<&str> = linha.split(' ').collect();
    assert_eq!(campos.len(), 8, "{linha}");
    let ts = campos[0];
    assert!(ts.len() == 20 && ts.ends_with('Z') && ts.as_bytes()[10] == b'T', "{ts}");
    assert!(campos[1].strip_prefix("pid=").is_some_and(|p| p.parse::<u32>().is_ok()));
    assert_eq!(campos[2], "motor=kokoro");
    assert_eq!(campos[3], "chars=13");
    assert!(campos[4].strip_prefix("audio_s=").is_some_and(|p| p.parse::<f64>().is_ok()), "{linha}");
    assert_eq!(campos[5], "rc=0");
    assert!(campos[6].strip_prefix("dur_s=").is_some_and(|p| p.parse::<u64>().is_ok()));
    assert!(campos[7].strip_prefix("ttfa_ms=").is_some_and(|p| p.parse::<u64>().is_ok()), "{linha}");
}
