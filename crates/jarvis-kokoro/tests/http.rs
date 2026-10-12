//! Servidor HTTP/1.0 falso (como o do server.py) e teste real `#[ignore]`.

use std::time::{Duration, Instant};

use jarvis_kokoro::{ErroKokoro, KokoroHttp, Sintetizador, URL_PADRAO, wav};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Sobe um servidor que responde `resposta` (bytes crus) a cada conexao.
/// `atraso` espera antes de responder.
async fn falso(resposta: Vec<u8>, atraso: Duration) -> KokoroHttp {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                return;
            };
            let r = resposta.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let mut lido = Vec::new();
                // Le ate o fim dos cabecalhos e do corpo declarado.
                loop {
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    lido.extend_from_slice(&buf[..n]);
                    if let Some(i) = lido.windows(4).position(|w| w == b"\r\n\r\n") {
                        let cab = String::from_utf8_lossy(&lido[..i]).to_lowercase();
                        let cl = cab
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if lido.len() >= i + 4 + cl {
                            break;
                        }
                    }
                }
                tokio::time::sleep(atraso).await;
                let _ = s.write_all(&r).await;
                let _ = s.shutdown().await;
            });
        }
    });
    KokoroHttp::new(format!("http://127.0.0.1:{porta}")).unwrap()
}

fn http10(status: &str, tipo: &str, corpo: &[u8]) -> Vec<u8> {
    let mut v = format!(
        "HTTP/1.0 {status}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\n\r\n",
        corpo.len()
    )
    .into_bytes();
    v.extend_from_slice(corpo);
    v
}

fn erro_kokoro(e: &anyhow::Error) -> &ErroKokoro {
    e.downcast_ref::<ErroKokoro>().expect("ErroKokoro")
}

#[tokio::test]
async fn http_200_devolve_pcm() {
    let amostras: Vec<i16> = (0..2400).map(|i| (i % 100) as i16).collect();
    let k = falso(
        http10("200 OK", "audio/wav", &wav::codificar(&amostras, 24_000)),
        Duration::ZERO,
    )
    .await;
    let p = k.sintetizar("Olá.", "pf_dora", 1.15).await.unwrap();
    assert_eq!(p.amostras, amostras);
    assert_eq!(p.taxa, 24_000);
}

#[tokio::test]
async fn http_503_vira_erro_http() {
    let k = falso(
        http10(
            "503 Service Unavailable",
            "application/json",
            br#"{"error":"modelo carregando"}"#,
        ),
        Duration::ZERO,
    )
    .await;
    let e = k.sintetizar("Olá.", "pf_dora", 1.0).await.unwrap_err();
    assert!(
        matches!(erro_kokoro(&e), ErroKokoro::Http { status: 503, corpo } if corpo.contains("carregando"))
    );
}

#[tokio::test]
async fn http_400_vira_erro_http() {
    let k = falso(
        http10(
            "400 Bad Request",
            "application/json",
            br#"{"error":"voz desconhecida"}"#,
        ),
        Duration::ZERO,
    )
    .await;
    let e = k.sintetizar("Olá.", "xx", 1.0).await.unwrap_err();
    assert!(matches!(
        erro_kokoro(&e),
        ErroKokoro::Http { status: 400, .. }
    ));
}

#[tokio::test]
async fn timeout_de_10s() {
    let k = falso(
        http10("200 OK", "audio/wav", &wav::codificar(&[1], 24_000)),
        Duration::from_secs(30),
    )
    .await;
    let t = Instant::now();
    let e = k.sintetizar("Olá.", "pf_dora", 1.0).await.unwrap_err();
    let dt = t.elapsed();
    assert!(
        matches!(erro_kokoro(&e), ErroKokoro::Transporte(r) if r.is_timeout()),
        "{e:#}"
    );
    assert!(
        dt >= Duration::from_secs(9) && dt < Duration::from_secs(12),
        "{dt:?}"
    );
}

#[tokio::test]
async fn wav_truncado_no_corpo_e_erro() {
    // Content-Length coerente, mas o WAV declara mais dados do que ha.
    let w = wav::codificar(&[1, 2, 3, 4, 5, 6], 24_000);
    let k = falso(
        http10("200 OK", "audio/wav", &w[..w.len() - 5]),
        Duration::ZERO,
    )
    .await;
    let e = k.sintetizar("Olá.", "pf_dora", 1.0).await.unwrap_err();
    assert!(matches!(erro_kokoro(&e), ErroKokoro::Wav(_)), "{e:#}");
}

#[tokio::test]
async fn conexao_cortada_no_meio_e_erro() {
    // Content-Length maior que o corpo enviado.
    let w = wav::codificar(&[1, 2, 3, 4], 24_000);
    let mut r = format!(
        "HTTP/1.0 200 OK\r\nContent-Length: {}\r\n\r\n",
        w.len() + 100
    )
    .into_bytes();
    r.extend_from_slice(&w);
    let k = falso(r, Duration::ZERO).await;
    assert!(k.sintetizar("Olá.", "pf_dora", 1.0).await.is_err());
}

#[tokio::test]
async fn wav_com_taxa_errada_e_erro() {
    let k = falso(
        http10("200 OK", "audio/wav", &wav::codificar(&[1, 2], 16_000)),
        Duration::ZERO,
    )
    .await;
    let e = k.sintetizar("Olá.", "pf_dora", 1.0).await.unwrap_err();
    assert!(matches!(erro_kokoro(&e), ErroKokoro::Wav(_)));
}

#[tokio::test]
async fn saudavel_200_503_e_sem_servidor() {
    let ok = falso(
        http10("200 OK", "application/json", br#"{"status":"ok"}"#),
        Duration::ZERO,
    )
    .await;
    assert!(ok.saudavel().await);
    let carregando = falso(
        http10(
            "503 Service Unavailable",
            "application/json",
            br#"{"status":"loading"}"#,
        ),
        Duration::ZERO,
    )
    .await;
    assert!(!carregando.saudavel().await);
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = l.local_addr().unwrap().port();
    drop(l);
    assert!(
        !KokoroHttp::new(format!("http://127.0.0.1:{porta}"))
            .unwrap()
            .saudavel()
            .await
    );
}

#[tokio::test]
async fn vozes_le_a_lista() {
    let k = falso(
        http10(
            "200 OK",
            "application/json",
            br#"{"voices":["pf_dora","pm_alex"],"default":"pf_dora"}"#,
        ),
        Duration::ZERO,
    )
    .await;
    assert_eq!(k.vozes().await.unwrap(), vec!["pf_dora", "pm_alex"]);
}

/// Contra o servidor de verdade em 127.0.0.1:7349. Regrava `fala.txt` (inofensivo).
#[tokio::test]
#[ignore = "precisa do Kokoro real em 127.0.0.1:7349"]
async fn real_health_voices_e_frase_curta() {
    let k = KokoroHttp::new(URL_PADRAO).unwrap();
    assert!(k.saudavel().await, "/health");
    let vozes = k.vozes().await.unwrap();
    assert!(!vozes.is_empty(), "/voices");
    let voz = if vozes.iter().any(|v| v == "pf_dora") {
        "pf_dora"
    } else {
        &vozes[0]
    };
    let t = Instant::now();
    let p = k.sintetizar("Teste rápido.", voz, 1.15).await.unwrap();
    let dt = t.elapsed();
    eprintln!(
        "frase curta: {dt:?}, {:.2} s de audio, voz {voz}",
        p.duracao_s()
    );
    assert!(p.duracao_s() > 0.3);
}
