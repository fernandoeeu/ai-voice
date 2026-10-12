#!/usr/bin/env bash
# Roteiro da POC Rust do Jarvis, lado a lado com a producao.
# Uso: scripts/poc-rust.sh build|up [--integrado]|down|status|logs
# Nunca usa launchctl nem toca em ~/Library/LaunchAgents.
set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
POC="$HOME/.local/share/jarvis/poc"
PID_FILE="$POC/jarvisd.pid"
LOG_DIR_PADRAO="$POC/logs"
SAIDA_LOG="$LOG_DIR_PADRAO/jarvisd.out"
SOCK="${JARVISD_SOCK:-$POC/jarvisd.sock}"
BIN="$RAIZ/target/release/jarvisd"
CLI="$RAIZ/target/release/jarvis-fala-rs"
PING='{"v":1,"id":"p","op":"ping"}'

uso() {
  echo "uso: $0 build | up [--integrado] | down | status | logs" >&2
  exit 2
}

vivo() {
  [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null
}

# Envia uma linha NDJSON ao socket e imprime a resposta (timeout curto).
pergunta() {
  local linha="$1"
  [[ -S "$SOCK" ]] || return 1
  printf '%s\n' "$linha" | nc -U -w 1 "$SOCK" 2>/dev/null | head -n 1
}

responde_ping() {
  local r
  if [[ -x "$CLI" ]]; then
    JARVIS_FALA_SOCK="$SOCK" "$CLI" --ping >/dev/null 2>&1
    return
  fi
  r="$(pergunta "$PING" || true)"
  [[ -n "$r" ]]
}

cmd_build() {
  mkdir -p "$HOME/.local/share/jarvis"
  cd "$RAIZ"
  lockf -t 1800 "$HOME/.local/share/jarvis/bench.lock" \
    nice -n 10 cargo build --release -p jarvisd -p jarvis-fala-cli
}

cmd_up() {
  local integrado=0
  for a in "$@"; do
    case "$a" in
      --integrado) integrado=1 ;;
      *) uso ;;
    esac
  done
  if vivo; then
    echo "jarvisd ja esta rodando (pid $(cat "$PID_FILE"))" >&2
    exit 1
  fi
  [[ -x "$BIN" ]] || { echo "binario ausente: $BIN (rode: $0 build)" >&2; exit 1; }

  local lock
  if (( integrado )); then
    lock="$HOME/.local/share/jarvis/fala.lock"
    echo "AVISO: modo integrado, lock compartilhado com a producao e a Escuta ($lock)." >&2
  else
    lock="${JARVISD_LOCK:-$POC/fala.lock}"
  fi

  mkdir -p "$POC" "${JARVISD_LOG_DIR:-$LOG_DIR_PADRAO}" "$LOG_DIR_PADRAO"
  rm -f "$SOCK"

  # Repassa so o que estiver definido; o resto fica no padrao do binario.
  local -a amb=("JARVISD_SOCK=$SOCK" "JARVISD_LOCK=$lock")
  local v
  for v in JARVISD_MANTER JARVISD_DITHER_DB JARVISD_SAIDA JARVIS_TTS_URL JARVISD_LOG_DIR; do
    if [[ -n "${!v:-}" ]]; then amb+=("$v=${!v}"); fi
  done

  env "${amb[@]}" nohup "$BIN" >>"$SAIDA_LOG" 2>&1 </dev/null &
  echo $! >"$PID_FILE"
  disown 2>/dev/null || true


  for _ in $(seq 1 30); do
    if ! vivo; then
      echo "jarvisd morreu ao subir; veja $SAIDA_LOG" >&2
      rm -f "$PID_FILE"
      exit 1
    fi
    if responde_ping; then
      echo "jarvisd no ar (pid $(cat "$PID_FILE"), socket $SOCK, lock $lock)"
      return 0
    fi
    sleep 0.1
  done
  echo "jarvisd nao respondeu ao ping em 3 s; veja $SAIDA_LOG" >&2
  exit 1
}

cmd_down() {
  if ! vivo; then
    echo "jarvisd nao esta rodando"
    rm -f "$PID_FILE"
    return 0
  fi
  local pid
  pid="$(cat "$PID_FILE")"
  kill -TERM "$pid"
  for _ in $(seq 1 100); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "$pid" 2>/dev/null; then
    echo "pid $pid nao saiu em 10 s apos SIGTERM" >&2
    exit 1
  fi
  rm -f "$PID_FILE"
  echo "jarvisd parado"
}

cmd_status() {
  if vivo; then
    echo "vivo (pid $(cat "$PID_FILE")), socket $SOCK"
    pergunta '{"v":1,"id":"s","op":"estado"}' || echo "(socket nao respondeu ao estado)"
  else
    echo "parado"
    return 1
  fi
}

cmd_logs() {
  local d="${JARVISD_LOG_DIR:-$LOG_DIR_PADRAO}"
  mkdir -p "$d"
  local -a arqs=()
  local f
  for f in "$SAIDA_LOG" "$d"/*.log; do
    [[ -f "$f" ]] && arqs+=("$f")
  done
  (( ${#arqs[@]} )) || { echo "sem logs em $d" >&2; exit 1; }
  tail -n 50 -F "${arqs[@]}"
}

[[ $# -ge 1 ]] || uso
acao="$1"; shift
case "$acao" in
  build) cmd_build ;;
  up) cmd_up "$@" ;;
  down) cmd_down ;;
  status) cmd_status ;;
  logs) cmd_logs ;;
  *) uso ;;
esac
