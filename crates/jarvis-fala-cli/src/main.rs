//! `jarvis-fala-rs`: cliente fino do jarvisd (NDJSON v1 sobre socket Unix).
//!
//! Sem tokio e sem clap, para o start ser o menor possivel. Se o socket nao
//! existir, nao aceitar ou nao responder `aceito` em 50 ms, faz `exec` do
//! script legado (`fala-kokoro.sh`) com o mesmo texto e voz.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, exit};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use jarvis_protocolo::{CodigoErro, Esperar, Evento, Mensagem, Op, Requisicao, codificar, decodificar, rc};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

/// Marcado antes do shutdown do socket, para a thread principal (que ve o EOF
/// primeiro) sair com 143 em vez de tratar como erro/fallback.
static INTERROMPIDO: AtomicBool = AtomicBool::new(false);

const PRAZO_ACEITO: Duration = Duration::from_millis(50);

struct Args {
    texto: Option<String>,
    voz: Option<String>,
    modo: Esperar,
}

fn ler_args() -> Args {
    let mut pos: Vec<String> = Vec::new();
    let mut modo = Esperar::Fim;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--sem-esperar" => modo = Esperar::Aceito,
            "--primeiro-som" => modo = Esperar::PrimeiroSom,
            _ => pos.push(a),
        }
    }
    let mut it = pos.into_iter();
    Args { texto: it.next(), voz: it.next(), modo }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn env_nao_vazia(nome: &str) -> Option<String> {
    std::env::var(nome).ok().filter(|v| !v.is_empty())
}

fn fallback(texto: &str, voz: Option<&str>) -> ! {
    let legado = env_nao_vazia("JARVIS_FALA_LEGADO")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share/jarvis/tts/fala-kokoro.sh"));
    let mut cmd = Command::new(&legado);
    cmd.arg(texto);
    if let Some(v) = voz {
        cmd.arg(v);
    }
    let erro = cmd.exec();
    eprintln!("jarvis-fala-rs: exec de {} falhou: {erro}", legado.display());
    exit(rc::ERRO);
}

fn main() {
    let args = ler_args();
    let texto = match args.texto {
        Some(t) => t,
        None => {
            let mut s = String::new();
            let _ = std::io::stdin().read_to_string(&mut s);
            s.trim_end_matches(['\n', '\r']).to_string()
        }
    };
    if texto.trim().is_empty() {
        exit(0);
    }
    let voz = args.voz.or_else(|| env_nao_vazia("JARVIS_TTS_VOICE"));
    let verboso = std::env::var("JARVIS_FALA_VERBOSO").is_ok_and(|v| v == "1");

    let sock = env_nao_vazia("JARVIS_FALA_SOCK")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share/jarvis/poc/jarvisd.sock"));
    let Ok(mut stream) = UnixStream::connect(&sock) else {
        fallback(&texto, voz.as_deref());
    };

    let id = format!("cli-{}", std::process::id());
    let req = Requisicao::nova(
        id,
        Op::Falar {
            texto: texto.clone(),
            voz: voz.clone(),
            velocidade: None,
            origem: Some("jarvis-fala-rs".into()),
            esperar: args.modo,
            prazo_fila_s: None,
            cancelar_ao_desconectar: Some(args.modo != Esperar::Aceito),
        },
    );
    let linha = codificar(&req).expect("requisicao serializavel");
    let _ = stream.set_write_timeout(Some(PRAZO_ACEITO));
    if stream.write_all(linha.as_bytes()).is_err() {
        fallback(&texto, voz.as_deref());
    }
    let _ = stream.set_write_timeout(None);
    let _ = stream.set_read_timeout(Some(PRAZO_ACEITO));

    // SIGTERM/SIGINT: so fecha a conexao e sai com 143; o daemon cancela a
    // fala deste cliente (`cancelar_ao_desconectar`), sem tocar nas dos outros.
    if let (Ok(mut sinais), Ok(escrita)) = (Signals::new([SIGTERM, SIGINT]), stream.try_clone()) {
        std::thread::spawn(move || {
            if sinais.forever().next().is_some() {
                INTERROMPIDO.store(true, Ordering::SeqCst);
                let _ = escrita.shutdown(Shutdown::Both);
                exit(rc::INTERROMPIDA);
            }
        });
    }

    let mut leitor = BufReader::new(stream);
    let mut aceito = false;
    let mut buf = String::new();
    loop {
        buf.clear();
        let lido = leitor.read_line(&mut buf);
        if INTERROMPIDO.load(Ordering::SeqCst) && !matches!(lido, Ok(n) if n > 0) {
            exit(rc::INTERROMPIDA);
        }
        match lido {
            Ok(0) => {
                if !aceito {
                    fallback(&texto, voz.as_deref());
                }
                eprintln!("jarvis-fala-rs: conexao encerrada antes do fim");
                exit(rc::ERRO);
            }
            Ok(_) => {}
            Err(_) if !aceito => fallback(&texto, voz.as_deref()),
            Err(e) => {
                eprintln!("jarvis-fala-rs: erro de leitura: {e}");
                exit(rc::ERRO);
            }
        }
        let Ok(msg) = decodificar::<Mensagem>(&buf) else { continue };
        match msg.ev {
            Evento::Aceito { .. } => {
                aceito = true;
                let _ = leitor.get_ref().set_read_timeout(None);
                if args.modo == Esperar::Aceito {
                    exit(0);
                }
            }
            Evento::PrimeiroSom { ms, .. } => {
                if verboso {
                    eprintln!("primeiro_som ms={ms}");
                }
                if args.modo == Esperar::PrimeiroSom {
                    exit(0);
                }
            }
            Evento::Fim { rc, .. } => exit(rc),
            Evento::Erro { codigo, mensagem } => {
                eprintln!("jarvis-fala-rs: {mensagem}");
                exit(if codigo == CodigoErro::FilaEsgotou { rc::PRAZO_FILA } else { rc::ERRO });
            }
            _ => {}
        }
    }
}
