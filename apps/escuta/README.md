# Escuta do Jarvis

App de barra de menus (sem Dock) que ouve sempre, transcreve em pt-BR no próprio Mac
(SpeechAnalyzer/SpeechTranscriber, on-device) e, quando a frase começa com "Jarvis" ou "camarada",
manda o pedido inteiro, numa mensagem só, como `[voz do Fernando] ...` para o painel do orquestrador
no herdr (`w1:p1`; se o id mudar, o agente chamado `maestro`). Toca um Tink ao enviar.

## Como decide que a fala acabou

- Silêncio medido no próprio áudio do microfone, em tempo real.
- Com 1,3 s de silêncio, pede ao reconhecedor os resultados finais (sem esperar ele decidir sozinho).
- Frase com cara de completa: envia. Frase terminada em conectivo ("e", "que", "pra", "mas",
  "porque"...) ou vírgula: espera até 2,5 s de silêncio. Texto ainda mudando: espera.
- Só envia texto final; nunca manda fragmento nem "(continuação)".
- Depois de entregar um pedido, a fala que começar nos 8 s seguintes também é entregue, sem precisar de
  "Jarvis", como mensagem nova e inteira (uma pausa longa no meio do pedido não perde palavras).
- Enquanto `~/.local/share/jarvis/fala.lock` está ocupado (o Jarvis falando), o reconhecedor recebe silêncio.

## Instalar e operar

```sh
scripts/instalar.sh --carregar     # compila, monta ~/.local/share/jarvis/bin/Escuta.app e carrega o LaunchAgent
launchctl kickstart -k gui/$(id -u)/dev.jarvis.escuta   # reiniciar
launchctl bootout gui/$(id -u)/dev.jarvis.escuta        # desligar até o próximo login
```

O LaunchAgent `dev.jarvis.escuta` abre no login e reabre depois de falha (saída 75 = reinício pedido
pela própria escuta, por exemplo pelo botão Reiniciar). "Sair" no painel encerra de verdade (saída 0).
Clicar no ícone da barra de menus abre o painel com a onda do microfone, estado, pedidos e controles.
Se o coreaudiod reiniciar, o microfone sumir ou a captura der erro, o processo sai e o launchd abre de novo.
A captura é só de entrada (AVCaptureSession): não mexe na saída de áudio nem no dispositivo padrão.

Logs: `~/.local/share/jarvis/escuta/logs/escuta.log` (diagnóstico, com a latência de cada pedido)
e `escuta.events.jsonl` (eventos).

Ajustes no `EnvironmentVariables` do plist: `ESCUTA_SILENCIO` (1.3), `ESCUTA_SILENCIO_ABERTA` (2.5), `ESCUTA_CONVERSA` (8),
`ESCUTA_MIC` (parte do nome do microfone, padrão `iPhone`, cai no embutido), `ESCUTA_PANE`, `ESCUTA_ORQUESTRADOR`.

Cada binário novo tem assinatura ad hoc diferente: o macOS pode pedir de novo a permissão do microfone.

## Testar sem microfone

```sh
say -v Luciana -o /tmp/f.aiff "Jarvis, teste da escuta."
.build/release/Escuta --audio /tmp/f.aiff --saida /tmp/pedidos.jsonl
.build/release/Escuta --saida /tmp/pedidos.jsonl --sem-som --abrir-painel   # instância de teste com o painel aberto
swift test
```
