use std::ffi::OsString;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use jarvis_lock::{Guardiao, GuardiaoProcesso, esta_preso};

const BIN: &str = env!("CARGO_BIN_EXE_jarvis-lock-guardiao");

fn guardiao(caminho: &std::path::Path) -> GuardiaoProcesso {
    GuardiaoProcesso::new(BIN, Vec::<OsString>::new(), caminho)
}

/// Caminho em diretorio temporario, sem fd aberto no processo de teste.
struct Tmp(#[allow(dead_code)] tempfile::TempDir, std::path::PathBuf);
impl Tmp {
    fn path(&self) -> &std::path::Path {
        &self.1
    }
}
fn tmp() -> Tmp {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("fala.lock");
    std::fs::write(&p, b"").unwrap();
    Tmp(d, p)
}

#[tokio::test]
async fn lockf_sai_75_com_guardiao_vivo() {
    let f = tmp();
    let g = guardiao(f.path()).segurar(Duration::from_secs(5)).await.unwrap();
    let st = Command::new("/usr/bin/lockf")
        .args(["-t", "0"])
        .arg(f.path())
        .arg("true")
        .status()
        .unwrap();
    assert_eq!(st.code(), Some(75));
    drop(g);
}

#[tokio::test]
async fn sigterm_no_guardiao_avisa_rapido() {
    let f = tmp();
    let g = guardiao(f.path()).segurar(Duration::from_secs(5)).await.unwrap();
    let pid = g.pid().unwrap();
    let morreu = g.morreu();
    let t = Instant::now();
    Command::new("kill").args(["-TERM", &pid.to_string()]).status().unwrap();
    tokio::time::timeout(Duration::from_millis(50), morreu)
        .await
        .expect("morreu() demorou mais de 50 ms");
    eprintln!("latencia_morte_ms={:.2}", t.elapsed().as_secs_f64() * 1000.0);
    assert!(!esta_preso(f.path()));
}

#[tokio::test]
async fn processo_de_teste_nao_aparece_no_lsof() {
    let f = tmp();
    let g = guardiao(f.path()).segurar(Duration::from_secs(5)).await.unwrap();
    let out = Command::new("lsof").arg("-t").arg(f.path()).output().unwrap();
    let pids: Vec<u32> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();
    assert!(pids.contains(&g.pid().unwrap()), "{pids:?}");
    assert!(!pids.contains(&std::process::id()), "{pids:?}");
}

#[tokio::test]
async fn prazo_esgota_com_outro_segurando() {
    let f = tmp();
    let mut outro = Command::new("/usr/bin/lockf")
        .args(["-k"])
        .arg(f.path())
        .args(["sleep", "30"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let t = Instant::now();
    while !esta_preso(f.path()) {
        assert!(t.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let r = guardiao(f.path()).segurar(Duration::from_millis(300)).await;
    assert!(r.is_err());
    let _ = outro.kill();
    let _ = outro.wait();
}

#[tokio::test]
async fn esta_preso_nos_momentos_certos() {
    let f = tmp();
    assert!(!esta_preso(f.path()));
    assert!(!esta_preso(&f.path().with_extension("naoexiste")));
    let g = guardiao(f.path()).segurar(Duration::from_secs(5)).await.unwrap();
    assert!(esta_preso(f.path()));
    let morreu = g.morreu();
    drop(g);
    tokio::time::timeout(Duration::from_secs(1), morreu).await.unwrap_err_or_ok();
    let t = Instant::now();
    while esta_preso(f.path()) {
        assert!(t.elapsed() < Duration::from_secs(2));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

trait Ignora {
    fn unwrap_err_or_ok(self);
}
impl<T, E> Ignora for Result<T, E> {
    fn unwrap_err_or_ok(self) {}
}
