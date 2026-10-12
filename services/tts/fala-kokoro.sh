#!/bin/bash
# Cliente do servidor local de TTS (Kokoro). Uso: fala-kokoro.sh "texto" [voz]
# Sintetiza no servidor local (AI_VOICE_TTS_URL, padrão http://127.0.0.1:7349) e toca com afplay;
# se o servidor falhar ou passar de 10 s, cai para `say -v $AI_VOICE_MAC_VOICE` (padrão Luciana).
# Serializa a fala com lockf no mesmo arquivo que o servidor MCP usa (AI_VOICE_MAC_LOCK).
set -u

TEXT="${1:-}"
VOICE="${2:-${JARVIS_TTS_VOICE:-pf_dora}}"
[ -n "$TEXT" ] || { echo "uso: $0 \"texto\" [voz]" >&2; exit 2; }

TTS_URL="${AI_VOICE_TTS_URL:-http://127.0.0.1:7349}"
MAC_VOICE="${AI_VOICE_MAC_VOICE:-Luciana}"
LOCK="${AI_VOICE_MAC_LOCK:-$HOME/.local/share/jarvis/fala.lock}"
mkdir -p "$(dirname "$LOCK")"

# Reentra sob o lock (mesmo lock do jarvis-fala). O -t limita só a espera na fila por quem
# ainda não tem o lock (sai com 75, EX_TEMPFAIL); quem já o segura toca até o fim, sem prazo.
if [ -z "${JARVIS_FALA_LOCKED:-}" ]; then
  JARVIS_FALA_LOCKED=1 exec /usr/bin/lockf -t 300 "$LOCK" "$0" "$@"
fi

TMP="$(mktemp -t jarvis-tts).wav" || exit 1
trap 'rm -f "$TMP" "${TMP%.wav}"' EXIT

json_str() {
  local s="$1"
  s="${s//\\/\\\\}"; s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"; s="${s//$'\r'/\\r}"; s="${s//$'\t'/\\t}"
  s="$(printf '%s' "$s" | LC_ALL=C tr -d '\000-\010\013\014\016-\037')"
  printf '"%s"' "$s"
}
BODY="{\"text\":$(json_str "$TEXT"),\"voice\":$(json_str "$VOICE")}"

# Uma linha por fala em FALA_LOG: quanto áudio havia e como o tocador terminou. Código 143 é
# SIGTERM (o barge-in da escuta, ou outro processo que segura o mesmo lock); 0 é fala inteira.
FALA_LOG="${JARVIS_FALA_LOG:-$HOME/.local/share/jarvis/tts/logs/fala.log}"
T0=$(date +%s)
registra() {
  mkdir -p "$(dirname "$FALA_LOG")" 2>/dev/null
  printf '%s pid=%s motor=%s chars=%s audio_s=%s rc=%s dur_s=%s\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$$" "$1" "${#TEXT}" "$2" "$3" "$(( $(date +%s) - T0 ))" \
    >>"$FALA_LOG" 2>/dev/null
}
trap 'registra "${MOTOR:-?}" "${AUDIO_S:-?}" 143; exit 143' TERM

if /usr/bin/curl -sf --max-time 10 \
     -H 'Content-Type: application/json' \
     --data-binary "$BODY" \
     -o "$TMP" "${TTS_URL%/}/synthesize" \
   && [ -s "$TMP" ]; then
  MOTOR=kokoro
  # WAV mono 16 bits a 24 kHz com cabeçalho de 44 bytes.
  AUDIO_S=$(( ($(stat -f %z "$TMP") - 44) / 48000 ))
  /usr/bin/afplay "$TMP"
else
  MOTOR=say AUDIO_S=?
  /usr/bin/say -v "$MAC_VOICE" -- "$TEXT"
fi
RC=$?
registra "$MOTOR" "$AUDIO_S" "$RC"
exit "$RC"
