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

# Reentra sob o lock (mesmo lock do jarvis-fala).
if [ -z "${JARVIS_FALA_LOCKED:-}" ]; then
  JARVIS_FALA_LOCKED=1 exec /usr/bin/lockf -t 90 "$LOCK" "$0" "$@"
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

if /usr/bin/curl -sf --max-time 10 \
     -H 'Content-Type: application/json' \
     --data-binary "$BODY" \
     -o "$TMP" "${TTS_URL%/}/synthesize" \
   && [ -s "$TMP" ]; then
  /usr/bin/afplay "$TMP"
else
  /usr/bin/say -v "$MAC_VOICE" -- "$TEXT"
fi
