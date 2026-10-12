# Crates da POC Rust do Jarvis

Workspace Cargo da reescrita em Rust do nucleo de fala. Branch `rust-core-poc`. Plano completo em `~/.local/share/jarvis/maestro/agentes/rust-core/arquitetura.md`.

## Crates

| Crate | O que e |
|---|---|
| `jarvis-protocolo` | NDJSON v1 entre clientes e o `jarvisd`, mais tipos comuns (`FalaId`, `Pcm24k`) |
| `jarvis-audio` | Saida de audio residente (cpal, ring buffer, resample, saida falsa para testes) |
| `jarvis-kokoro` | Cliente do Kokoro (porta 7349), sintese por frase |
| `jarvis-lock` | Guardiao do lock compativel com o `lockf` do `jarvis-fala` e com a Escuta |
| `jarvis-fala` | Ator da fila de fala |
| `jarvisd` | Daemon residente: socket, fila, audio e lock |
| `jarvis-fala-cli` | Cliente fino `jarvis-fala-rs` |
| `jarvis-herdr` | Cliente NDJSON do `herdr.sock` (F9, opcional) |

## Build e testes

O `cargo` nao esta no PATH padrao, e builds pesados usam lock e `nice`, um por vez:

```sh
export PATH="$HOME/.cargo/bin:$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
scripts/poc-rust.sh build   # lockf -t 1800 ~/.local/share/jarvis/bench.lock nice -n 10 cargo build --release -p jarvisd -p jarvis-fala-cli
lockf -t 1800 ~/.local/share/jarvis/bench.lock nice -n 10 cargo test --workspace
lockf -t 1800 ~/.local/share/jarvis/bench.lock nice -n 10 cargo clippy --workspace --all-targets
```

## Subir a POC

```sh
scripts/poc-rust.sh up               # isolado: lock proprio em poc/fala.lock
scripts/poc-rust.sh up --integrado   # lock compartilhado com a producao e a Escuta
scripts/poc-rust.sh status
scripts/poc-rust.sh logs
scripts/poc-rust.sh down
```

O `up` repassa do ambiente: `JARVISD_SOCK`, `JARVISD_LOCK`, `JARVISD_MANTER`, `JARVISD_DITHER_DB`, `JARVISD_SAIDA`, `JARVIS_TTS_URL`, `JARVISD_LOG_DIR`. Espera o socket responder `ping` por ate 3 s.

- Isolado: a POC e a producao podem falar por cima uma da outra, e a Escuta nao ve a POC (sem barge-in, o eco pode virar pedido).
- Integrado (`JARVISD_LOCK=~/.local/share/jarvis/fala.lock`): serializa com a producao e a Escuta enxerga a fala. Use este para testar no fone.

O script nunca usa `launchctl` nem mexe em `~/Library/LaunchAgents`.

## A/B manual

Com a POC no ar (preferir `--integrado` no fone):

```sh
jarvis-fala "teste de latencia"                     # producao (curl + afplay)
target/release/jarvis-fala-rs "teste de latencia"   # POC (daemon residente)
```

Para numeros, use a bancada da F7 (mesmo metodo do `bench.py`).

## Producao contra POC

| Recurso | Producao | POC |
|---|---|---|
| Socket | nao existe | `~/.local/share/jarvis/poc/jarvisd.sock` |
| Lock da fala | `~/.local/share/jarvis/fala.lock` | `poc/fala.lock` (isolado) ou o lock da producao (`--integrado`) |
| Kokoro | 7349 | 7349, compartilhado, so como cliente |
| Saida de audio | `afplay` por fala | stream residente do `jarvisd` |
| Logs | `tts/logs/fala.log` | `poc/logs/fala.log`, `poc/logs/jarvisd.log`, `poc/logs/jarvisd.out` |
| PID | LaunchAgent | `poc/jarvisd.pid` |

## Reverter

```sh
scripts/poc-rust.sh down
rm -rf ~/.local/share/jarvis/poc
```

Nada mais foi alterado na producao.
