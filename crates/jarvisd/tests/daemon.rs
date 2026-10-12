//! Integracao: sobe o binario `jarvisd` com saida fake e um Kokoro falso.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

// ---------- Kokoro falso ----------

fn wav(segundos: f64) -> Vec<u8> {
    let n = (24_000.0 * segundos) as usize;
    let mut b = Vec::with_capacity(44 + n * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n as u32 * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24_000u32.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n as u32 * 2).to_le_bytes());
    for i in 0..n {
        let v = (f64::sin(i as f64 * 440.0 * std::f64::consts::TAU / 24_000.0) * 8000.0) as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

fn kokoro_falso(segundos: f64) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let corpo = wav(segundos);
    thread::spawn(move || {
        for c in l.incoming().flatten() {
            let corpo = corpo.clone();
            thread::spawn(move || atender(c, &corpo));
        }
    });
    url
}

fn atender(c: TcpStream, wav: &[u8]) {
    let mut r = BufReader::new(c.try_clone().unwrap());
    let mut w = c;
    loop {
        let mut linha = String::new();
        if r.read_line(&mut linha).unwrap_or(0) == 0 {
            return;
        }
        let health = linha.starts_with("GET /health");
        let mut len = 0usize;
        loop {
            let mut h = String::new();
            if r.read_line(&mut h).unwrap_or(0) == 0 {
                return;
            }
            if h.trim().is_empty() {
                break;
            }
            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
        let mut corpo = vec![0u8; len];
        if r.read_exact(&mut corpo).is_err() {
            return;
        }
        let (tipo, dados): (&str, Vec<u8>) = if health {
            ("application/json", br#"{"ok":true}"#.to_vec())
        } else {
            ("audio/wav", wav.to_vec())
        };
        let cab = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\n\r\n",
            dados.len()
        );
        if w.write_all(cab.as_bytes())
            .and_then(|()| w.write_all(&dados))
            .is_err()
        {
            return;
        }
    }
}

// ---------- Daemon ----------

struct Daemon {
    filho: Child,
    dir: TempDir,
}

impl Daemon {
    fn subir(segundos: f64) -> Self {
        let dir = TempDir::new().unwrap();
        let filho = Command::new(env!("CARGO_BIN_EXE_jarvisd"))
            .env("JARVISD_SAIDA", "fake")
            .env("JARVISD_SOCK", dir.path().join("run/d.sock"))
            .env("JARVISD_LOCK", dir.path().join("fala.lock"))
            .env("JARVISD_LOG_DIR", dir.path().join("logs"))
            .env("JARVIS_TTS_URL", kokoro_falso(segundos))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let d = Self { filho, dir };
        let t = Instant::now();
        while UnixStream::connect(d.sock()).is_err() {
            assert!(t.elapsed() < Duration::from_secs(10), "daemon nao subiu");
            thread::sleep(Duration::from_millis(20));
        }
        d
    }

    fn sock(&self) -> PathBuf {
        self.dir.path().join("run/d.sock")
    }

    fn lock(&self) -> PathBuf {
        self.dir.path().join("fala.lock")
    }

    fn cliente(&self) -> Cliente {
        Cliente::conectar(&self.sock())
    }

    fn term(&self) {
        let st = Command::new("kill")
            .args(["-TERM", &self.filho.id().to_string()])
            .status()
            .unwrap();
        assert!(st.success());
    }

    fn esperar_saida(&mut self) -> std::process::ExitStatus {
        let t = Instant::now();
        loop {
            if let Some(s) = self.filho.try_wait().unwrap() {
                return s;
            }
            assert!(t.elapsed() < Duration::from_secs(10), "daemon nao saiu");
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

struct Cliente {
    w: UnixStream,
    r: BufReader<UnixStream>,
}

impl Cliente {
    fn conectar(sock: &Path) -> Self {
        let w = UnixStream::connect(sock).unwrap();
        w.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        Self {
            r: BufReader::new(w.try_clone().unwrap()),
            w,
        }
    }

    fn enviar(&mut self, v: &Value) {
        writeln!(self.w, "{v}").unwrap();
    }

    fn falar(&mut self, id: &str, texto: &str, extra: Value) {
        let mut v = json!({"v":1,"id":id,"op":"falar","texto":texto,"esperar":"fim"});
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        self.enviar(&v);
    }

    fn ler(&mut self) -> Value {
        let mut l = String::new();
        let n = self.r.read_line(&mut l).expect("leitura");
        assert!(n > 0, "conexao fechada");
        serde_json::from_str(&l).unwrap()
    }

    /// Le ate achar `ev` (devolve o que passou, incluindo o achado).
    fn ate(&mut self, ev: &str) -> Vec<Value> {
        let mut v = Vec::new();
        loop {
            let m = self.ler();
            let achou = m["ev"] == ev;
            v.push(m);
            if achou {
                return v;
            }
        }
    }
}

fn evs(v: &[Value]) -> Vec<&str> {
    v.iter().map(|m| m["ev"].as_str().unwrap()).collect()
}

// ---------- Testes ----------

#[test]
fn ping_pong() {
    let d = Daemon::subir(0.2);
    let mut c = d.cliente();
    c.enviar(&json!({"v":1,"id":"p1","op":"ping"}));
    let m = c.ler();
    assert_eq!(
        (m["ev"].as_str(), m["id"].as_str(), m["v"].as_u64()),
        (Some("pong"), Some("p1"), Some(1))
    );
    // varias mensagens na mesma conexao
    c.enviar(&json!({"v":1,"id":"p2","op":"ping"}));
    assert_eq!(c.ler()["id"], "p2");
}

#[test]
fn erro_de_protocolo_vira_ev_erro() {
    let d = Daemon::subir(0.2);
    let mut c = d.cliente();
    writeln!(c.w, "isto nao e json").unwrap();
    assert_eq!(c.ler()["ev"], "erro");
    c.enviar(&json!({"v":2,"id":"x","op":"ping"}));
    let m = c.ler();
    assert_eq!(
        (m["ev"].as_str(), m["id"].as_str(), m["codigo"].as_str()),
        (Some("erro"), Some("x"), Some("protocolo"))
    );
    // a conexao continua util
    c.enviar(&json!({"v":1,"id":"ok","op":"ping"}));
    assert_eq!(c.ler()["ev"], "pong");
}

#[test]
fn falar_emite_aceito_primeiro_som_e_fim() {
    let d = Daemon::subir(0.3);
    let mut c = d.cliente();
    c.falar("f1", "ola mundo", json!({}));
    let v = c.ate("fim");
    assert_eq!(evs(&v), ["aceito", "primeiro_som", "fim"]);
    assert!(v.iter().all(|m| m["id"] == "f1"));
    assert_eq!(v[2]["rc"], 0);
    assert_eq!(v[2]["chars"], 9);
    assert_eq!(v[0]["fala"], v[2]["fala"]);
    // o log da POC fica no diretorio configurado
    let log = std::fs::read_to_string(d.dir.path().join("logs/fala.log")).unwrap();
    assert!(log.contains("rc=0"), "{log}");
}

#[test]
fn esperar_aceito_e_primeiro_som() {
    let d = Daemon::subir(0.3);
    let mut c = d.cliente();
    c.enviar(&json!({"v":1,"id":"a","op":"falar","texto":"oi","esperar":"aceito"}));
    assert_eq!(c.ler()["ev"], "aceito");
    c.enviar(&json!({"v":1,"id":"b","op":"falar","texto":"oi","esperar":"primeiro_som"}));
    let v = c.ate("primeiro_som");
    assert_eq!(evs(&v), ["aceito", "primeiro_som"]);
}

#[test]
fn dois_clientes_em_fifo() {
    let d = Daemon::subir(0.6);
    let mut a = d.cliente();
    let mut b = d.cliente();
    a.falar("a", "primeira", json!({}));
    assert_eq!(a.ler()["posicao"], 0);
    b.falar("b", "segunda", json!({}));
    let aceito_b = b.ler();
    assert_eq!(aceito_b["ev"], "aceito");
    assert_eq!(aceito_b["posicao"], 1);
    let ta = thread::spawn(move || {
        let v = a.ate("fim");
        (Instant::now(), v)
    });
    let tb = thread::spawn(move || {
        let v = b.ate("fim");
        (Instant::now(), v)
    });
    let (hora_a, va) = ta.join().unwrap();
    let (hora_b, vb) = tb.join().unwrap();
    assert_eq!(va.last().unwrap()["rc"], 0);
    assert_eq!(vb.last().unwrap()["rc"], 0);
    assert!(hora_a < hora_b, "a segunda fala terminou antes da primeira");
    assert!(vb.iter().any(|m| m["ev"] == "primeiro_som"));
}

#[test]
fn desconectar_cancela_fala_tocando() {
    let d = Daemon::subir(4.0);
    let mut obs = d.cliente();
    obs.enviar(&json!({"v":1,"id":"s","op":"assinar"}));
    assert_eq!(obs.ler()["ev"], "estado");

    let mut c = d.cliente();
    c.falar(
        "f",
        "vai ser cortada",
        json!({"cancelar_ao_desconectar": true}),
    );
    c.ate("primeiro_som");
    let t = Instant::now();
    drop(c);

    let v = obs.ate("fim");
    let fim = v.last().unwrap();
    assert_eq!(fim["rc"], 143, "{v:?}");
    assert!(t.elapsed() < Duration::from_secs(2), "cancelamento demorou");
    assert!(
        v.iter().all(|m| m["id"].is_null()),
        "eventos assinados nao levam id"
    );
}

#[test]
fn desconectar_cancela_fala_na_fila() {
    let d = Daemon::subir(1.5);
    let mut a = d.cliente();
    a.falar("a", "tocando", json!({}));
    a.ate("primeiro_som");
    let mut b = d.cliente();
    b.falar("b", "na fila", json!({"cancelar_ao_desconectar": true}));
    assert_eq!(b.ler()["posicao"], 1);
    drop(b);
    // A segue ate o fim, a fila esvazia.
    assert_eq!(a.ate("fim").last().unwrap()["rc"], 0);
    let mut c = d.cliente();
    c.enviar(&json!({"v":1,"id":"e","op":"estado"}));
    let e = c.ler();
    assert_eq!(e["fila"], 0);
}

#[test]
fn sem_a_flag_a_fala_sobrevive_a_desconexao() {
    let d = Daemon::subir(1.0);
    let mut obs = d.cliente();
    obs.enviar(&json!({"v":1,"id":"s","op":"assinar"}));
    obs.ler();
    let mut c = d.cliente();
    c.falar("f", "segue falando", json!({}));
    c.ate("primeiro_som");
    drop(c);
    assert_eq!(obs.ate("fim").last().unwrap()["rc"], 0);
}

#[test]
fn estado_reflete_a_fila() {
    let d = Daemon::subir(1.0);
    let mut c = d.cliente();
    c.enviar(&json!({"v":1,"id":"e0","op":"estado"}));
    let e = c.ler();
    assert_eq!(e["ev"], "estado");
    assert_eq!(e["id"], "e0");
    assert!(e["falando"].is_null());
    assert_eq!(e["fila"], 0);
    assert_eq!(e["kokoro_saudavel"], true);
    assert_eq!(e["dispositivo"], "fake");
    assert_eq!(e["manter"], "sempre");

    c.falar("f", "ocupado", json!({}));
    let fala = c.ate("primeiro_som")[0]["fala"].clone();
    let mut o = d.cliente();
    o.enviar(&json!({"v":1,"id":"e1","op":"estado"}));
    assert_eq!(o.ler()["falando"], fala);
    c.ate("fim");
    o.enviar(&json!({"v":1,"id":"e2","op":"estado"}));
    assert!(o.ler()["falando"].is_null());
}

#[test]
fn calar_da_143() {
    let d = Daemon::subir(4.0);
    let mut c = d.cliente();
    c.falar("f", "vai calar", json!({}));
    c.ate("primeiro_som");
    let mut o = d.cliente();
    o.enviar(&json!({"v":1,"id":"c","op":"calar","escopo":"tudo"}));
    assert_eq!(o.ler()["ev"], "estado");
    let fim = c.ate("fim");
    assert_eq!(fim.last().unwrap()["rc"], 143);
}

#[test]
fn permissoes_do_socket_e_da_pasta() {
    let d = Daemon::subir(0.2);
    let modo = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(modo(d.sock()), 0o600);
    assert_eq!(modo(d.sock().parent().unwrap().to_path_buf()), 0o700);
}

#[test]
fn socket_velho_e_removido_e_vivo_e_respeitado() {
    let d = Daemon::subir(0.2);
    // segundo daemon no mesmo socket recusa
    let st = Command::new(env!("CARGO_BIN_EXE_jarvisd"))
        .env("JARVISD_SAIDA", "fake")
        .env("JARVISD_SOCK", d.sock())
        .env("JARVISD_LOCK", d.dir.path().join("outro.lock"))
        .env("JARVISD_LOG_DIR", d.dir.path().join("logs2"))
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!st.success());
    // o primeiro segue atendendo
    let mut c = d.cliente();
    c.enviar(&json!({"v":1,"id":"p","op":"ping"}));
    assert_eq!(c.ler()["ev"], "pong");
    // socket velho (ninguem atende): um novo daemon assume
    let dir = TempDir::new().unwrap();
    let velho = dir.path().join("d.sock");
    drop(std::os::unix::net::UnixListener::bind(&velho).unwrap());
    assert!(velho.exists());
    let mut filho = Command::new(env!("CARGO_BIN_EXE_jarvisd"))
        .env("JARVISD_SAIDA", "fake")
        .env("JARVISD_SOCK", &velho)
        .env("JARVISD_LOCK", dir.path().join("l"))
        .env("JARVISD_LOG_DIR", dir.path().join("logs"))
        .spawn()
        .unwrap();
    let t = Instant::now();
    while UnixStream::connect(&velho).is_err() {
        assert!(t.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(20));
    }
    let _ = filho.kill();
    let _ = filho.wait();
}

fn jarvisd_em(dir: &Path, sock: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_jarvisd"));
    c.env("JARVISD_SAIDA", "fake")
        .env("JARVISD_SOCK", sock)
        .env("JARVISD_LOCK", dir.join("fala.lock"))
        .env("JARVISD_LOG_DIR", dir.join("logs"))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c
}

#[test]
fn arquivo_comum_no_caminho_do_socket_nao_e_apagado() {
    let dir = TempDir::new().unwrap();
    let alvo = dir.path().join("d.sock");
    std::fs::write(&alvo, "importante").unwrap();
    let st = jarvisd_em(dir.path(), &alvo).status().unwrap();
    assert!(!st.success());
    assert_eq!(std::fs::read_to_string(&alvo).unwrap(), "importante");
}

#[test]
fn pasta_preexistente_mantem_o_modo() {
    let dir = TempDir::new().unwrap();
    let pasta = dir.path().join("aberta");
    std::fs::create_dir(&pasta).unwrap();
    std::fs::set_permissions(&pasta, std::fs::Permissions::from_mode(0o755)).unwrap();
    let sock = pasta.join("d.sock");
    let mut filho = jarvisd_em(dir.path(), &sock).spawn().unwrap();
    let t = Instant::now();
    while UnixStream::connect(&sock).is_err() {
        assert!(t.elapsed() < Duration::from_secs(10), "daemon nao subiu");
        thread::sleep(Duration::from_millis(20));
    }
    let modo = std::fs::metadata(&pasta).unwrap().permissions().mode() & 0o777;
    let _ = filho.kill();
    let _ = filho.wait();
    assert_eq!(modo, 0o755);
}

#[test]
fn segundo_daemon_simultaneo_sai_com_erro_e_nao_mexe_no_socket() {
    let d = Daemon::subir(0.2);
    let dir2 = TempDir::new().unwrap();
    let st = jarvisd_em(dir2.path(), &d.sock()).status().unwrap();
    assert!(!st.success());
    let mut c = d.cliente();
    c.enviar(&json!({"v":1,"id":"p","op":"ping"}));
    assert_eq!(c.ler()["ev"], "pong");
}

#[test]
fn sigterm_limpa_o_socket() {
    let mut d = Daemon::subir(0.2);
    d.term();
    assert_eq!(d.esperar_saida().code(), Some(0));
    assert!(!d.sock().exists());
}

#[test]
fn sigterm_com_fala_cala_solta_o_lock_e_sai_com_zero() {
    let mut d = Daemon::subir(4.0);
    let mut c = d.cliente();
    c.falar("f", "em curso", json!({}));
    c.ate("primeiro_som");
    d.term();
    assert_eq!(c.ate("fim").last().unwrap()["rc"], 143);
    assert_eq!(d.esperar_saida().code(), Some(0));
    assert!(!d.sock().exists());
    assert_eq!(lockf(&d.lock()), 0, "lock deveria estar livre");
}

fn lockf(lock: &Path) -> i32 {
    Command::new("lockf")
        .args(["-t", "0"])
        .arg(lock)
        .arg("true")
        .status()
        .unwrap()
        .code()
        .unwrap()
}

#[test]
fn guardiao_segura_o_lock_durante_a_fala() {
    let d = Daemon::subir(1.5);
    assert_eq!(lockf(&d.lock()), 0, "ocioso: lock livre");
    let mut c = d.cliente();
    c.falar("f", "segurando", json!({}));
    c.ate("primeiro_som");
    assert_eq!(lockf(&d.lock()), 75, "falando: lock preso");
    c.ate("fim");
    let t = Instant::now();
    while lockf(&d.lock()) != 0 {
        assert!(t.elapsed() < Duration::from_secs(5), "lock nao foi solto");
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn modo_segurar_lock_sai_com_75_para_o_lockf() {
    let dir = TempDir::new().unwrap();
    let lock = dir.path().join("g.lock");
    let mut g = Command::new(env!("CARGO_BIN_EXE_jarvisd"))
        .arg("--segurar-lock")
        .arg(&lock)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut ok = String::new();
    BufReader::new(g.stdout.take().unwrap())
        .read_line(&mut ok)
        .unwrap();
    assert_eq!(ok.trim(), "ok");
    assert_eq!(lockf(&lock), 75);
    drop(g.stdin.take()); // dono some: o guardiao sai com 0
    assert_eq!(g.wait().unwrap().code(), Some(0));
    assert_eq!(lockf(&lock), 0);
}
