#!/bin/bash
# Compila a Escuta, monta ~/.local/share/jarvis/bin/Escuta.app e grava o LaunchAgent dev.jarvis.escuta.
# Uso: scripts/instalar.sh [--carregar]
#   --carregar  também (re)carrega o LaunchAgent no launchd.
# O .app só é trocado se o binário mudou: cada binário novo (assinatura ad hoc) pede de novo a
# permissão do microfone no macOS.
set -euo pipefail

cd "$(dirname "$0")/.."
PKG="$PWD"
JARVIS="$HOME/.local/share/jarvis"
APP="$JARVIS/bin/Escuta.app"
LABEL="dev.jarvis.escuta"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
STATE="$JARVIS/escuta"

swift build -c release --package-path "$PKG"
BIN="$(swift build -c release --package-path "$PKG" --show-bin-path)/Escuta"

mkdir -p "$APP/Contents/MacOS" "$STATE/logs"
if cmp -s "$BIN" "$APP/Contents/MacOS/Escuta" && cmp -s "$PKG/Resources/Info.plist" "$APP/Contents/Info.plist"; then
  echo "Escuta.app já está atualizado"
else
  cp "$PKG/Resources/Info.plist" "$APP/Contents/Info.plist"
  cp "$BIN" "$APP/Contents/MacOS/Escuta"
  codesign --force --sign - --identifier "$LABEL" "$APP"
  echo "Escuta.app instalado em $APP"
fi

cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>$APP/Contents/MacOS/Escuta</string>
  </array>
  <key>WorkingDirectory</key><string>$STATE</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>ESCUTA_SILENCIO</key><string>1.3</string>
    <key>ESCUTA_CONVERSA</key><string>8</string>
    <key>ESCUTA_SILENCIO_ABERTA</key><string>2.5</string>
    <key>ESCUTA_PANE</key><string>w1:p1</string>
    <key>ESCUTA_ORQUESTRADOR</key><string>maestro</string>
    <key>PATH</key><string>$HOME/.local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key><false/>
  </dict>
  <key>ThrottleInterval</key><integer>5</integer>
  <key>ProcessType</key><string>Interactive</string>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
  <key>StandardOutPath</key><string>$STATE/logs/escuta.events.jsonl</string>
  <key>StandardErrorPath</key><string>$STATE/logs/escuta.log</string>
</dict>
</plist>
EOF
plutil -lint "$PLIST" >/dev/null
echo "LaunchAgent gravado em $PLIST"

if [ "${1:-}" = "--carregar" ]; then
  launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
  launchctl bootstrap "gui/$(id -u)" "$PLIST"
  echo "LaunchAgent carregado"
fi
