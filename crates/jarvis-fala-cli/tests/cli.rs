use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

/// Servidor falso: aceita uma conexao, repassa as linhas recebidas por canal
/// e responde `aceito` + os eventos pedidos (`None` = segura a conexao).
fn servidor(dir: &Path, eventos: Vec<Value>) -> (mpsc::Receiver<Value>, std::path::PathBuf) {
    let sock = dir.join("d.sock");
    let l = UnixListener::bind(&sock).unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut linha = String::new();
        r.read_line(&mut linha).unwrap();
        tx.send(serde_json::from_str(&linha).unwrap()).unwrap();
        for e in eventos {
            writeln!(s, "{e}").unwrap();
        }
        loop {
            linha.clear();
            match r.read_line(&mut linha) {
                Ok(0) | Err(_) => break,
                Ok(_) => tx.send(serde_json::from_str(&linha).unwrap()).unwrap(),
            }
        }
    });
    (rx, sock)
}

fn aceito() -> Value {
    json!({"v":1,"ev":"aceito","fala":1,"posicao":0})
}
fn fim(rc: i32) -> Value {
    json!({"v":1,"ev":"fim","fala":1,"rc":rc,"motor":"fake","audio_s":1.0,"chars":3})
}

fn cli(sock: &Path) -> Command {
    let mut c = Command::cargo_bin("jarvis-fala-rs").unwrap();
    c.env("JARVIS_FALA_SOCK", sock)
        .env_remove("JARVIS_TTS_VOICE")
        .env_remove("JARVIS_FALA_VERBOSO")
        .timeout(Duration::from_secs(10));
    c
}

#[test]
fn argumento_e_voz() {
    let d = TempDir::new().unwrap();
    let (rx, sock) = servidor(d.path(), vec![aceito(), fim(0)]);
    cli(&sock).args(["ola mundo", "pf_dora"]).assert().code(0);
    let req = rx.recv().unwrap();
    assert_eq!(req["op"], "falar");
    assert_eq!(req["texto"], "ola mundo");
    assert_eq!(req["voz"], "pf_dora");
    assert_eq!(req["esperar"], "fim");
}

#[test]
fn stdin_e_voz_do_ambiente() {
    let d = TempDir::new().unwrap();
    let (rx, sock) = servidor(d.path(), vec![aceito(), fim(0)]);
    cli(&sock)
        .env("JARVIS_TTS_VOICE", "pm_alex")
        .write_stdin("do stdin\n")
        .assert()
        .code(0);
    let req = rx.recv().unwrap();
    assert_eq!(req["texto"], "do stdin");
    assert_eq!(req["voz"], "pm_alex");
}

#[test]
fn texto_vazio_sai_com_zero() {
    let d = TempDir::new().unwrap();
    let sock = d.path().join("nao-existe.sock");
    cli(&sock).write_stdin("  \n").assert().code(0);
    cli(&sock).arg("").assert().code(0);
}

#[test]
fn repassa_rc() {
    for codigo in [0, 143, 75] {
        let d = TempDir::new().unwrap();
        let (_rx, sock) = servidor(d.path(), vec![aceito(), fim(codigo)]);
        cli(&sock).arg("oi").assert().code(codigo);
    }
}

#[test]
fn sem_esperar_sai_no_aceito() {
    let d = TempDir::new().unwrap();
    let (rx, sock) = servidor(d.path(), vec![aceito()]);
    cli(&sock).args(["--sem-esperar", "oi"]).assert().code(0);
    assert_eq!(rx.recv().unwrap()["esperar"], "aceito");
}

#[test]
fn primeiro_som_e_verboso() {
    let d = TempDir::new().unwrap();
    let ps = json!({"v":1,"ev":"primeiro_som","fala":1,"ms":42});
    let (_rx, sock) = servidor(d.path(), vec![aceito(), ps]);
    let out = cli(&sock)
        .env("JARVIS_FALA_VERBOSO", "1")
        .args(["--primeiro-som", "oi"])
        .assert()
        .code(0);
    assert!(String::from_utf8_lossy(&out.get_output().stderr).contains("ms=42"));
}

#[test]
fn sigterm_so_fecha_a_conexao() {
    let d = TempDir::new().unwrap();
    let (rx, sock) = servidor(d.path(), vec![aceito()]);
    let mut filho = std::process::Command::new(assert_cmd::cargo::cargo_bin("jarvis-fala-rs"))
        .env("JARVIS_FALA_SOCK", &sock)
        .arg("fala longa")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap()["op"],
        "falar"
    );
    thread::sleep(Duration::from_millis(200));
    enviar_sigterm(filho.id());
    let status = filho.wait().unwrap();
    assert_eq!(status.code(), Some(143));
    // O servidor falso ve EOF (o canal fecha) e nunca recebe `calar`.
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
}

fn enviar_sigterm(pid: u32) {
    // `kill` do sistema: o workspace proibe unsafe.
    let ok = std::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .unwrap();
    assert!(ok.success());
}

#[test]
fn sem_socket_faz_exec_do_legado() {
    let d = TempDir::new().unwrap();
    let script = d.path().join("legado.sh");
    std::fs::write(&script, "#!/bin/sh\necho \"legado:$1:$2\"\nexit 7\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = cli(&d.path().join("nada.sock"))
        .env("JARVIS_FALA_LEGADO", &script)
        .args(["oi", "pf_dora"])
        .assert()
        .code(7);
    assert_eq!(
        String::from_utf8_lossy(&out.get_output().stdout).trim(),
        "legado:oi:pf_dora"
    );
}

#[test]
fn servidor_mudo_cai_no_legado() {
    let d = TempDir::new().unwrap();
    let script = d.path().join("legado.sh");
    std::fs::write(&script, "#!/bin/sh\necho \"legado:$1\"\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_rx, sock) = servidor(d.path(), vec![]);
    let out = cli(&sock)
        .env("JARVIS_FALA_LEGADO", &script)
        .write_stdin("via stdin")
        .assert()
        .code(0);
    assert_eq!(
        String::from_utf8_lossy(&out.get_output().stdout).trim(),
        "legado:via stdin"
    );
}
