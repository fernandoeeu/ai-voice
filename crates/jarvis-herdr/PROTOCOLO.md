# Protocolo do herdr.sock (confirmado em 2026-10-12, herdr 0.9.1, protocolo 22)

Socket Unix: `~/.config/herdr/herdr.sock`. Spike so de leitura (ping, agent.list,
agent.get, events.subscribe); nenhum input foi enviado a pane.

## Enquadramento

NDJSON: uma mensagem JSON por linha terminada em `\n`, nos dois sentidos.

- Requisicao: `{"id":"<str>","method":"<metodo>","params":{...}}` (sem campo de versao).
- Sucesso: `{"id":"<mesmo id>","result":{"type":"<tipo>", ...}}`.
- Erro: `{"id":"","error":{"code":"invalid_request","message":"..."}}`. Para metodo
  desconhecido o `id` volta vazio e a mensagem lista as variantes validas.
- A conexao continua aberta depois da resposta (varias requisicoes na mesma conexao);
  o `nc -w 2` do spike fechou por timeout.

## Respostas confirmadas

| Metodo | `params` | `result` |
|---|---|---|
| `ping` | `{}` | `{"type":"pong","version":"0.9.1","protocol":22,"capabilities":{...}}` |
| `agent.list` | `{}` | `{"type":"agent_list","agents":[AgentInfo...]}` |
| `agent.get` | `{"target":"<pane_id ou nome>"}` | `{"type":"agent_info","agent":AgentInfo}` |
| `events.subscribe` | `{"subscriptions":[{"type":"pane.agent_status_changed","pane_id":"..","agent_status":null\|"idle"..}]}` | `{"type":"subscription_started"}`, depois a conexao vira fluxo de eventos |

`AgentInfo` traz: `terminal_id`, `name`, `agent`, `terminal_title(_stripped)`,
`agent_status` (`idle|working|blocked|done|unknown`), `workspace_id`, `tab_id`,
`pane_id` (ex.: `wT2:pN`), `focused`, `interactive_ready` (opcional),
`state_change_seq`, `cwd`, `foreground_cwd`, `revision`.

## Nao exercitados (so no schema)

- `pane.send_input`: `{"pane_id","text"?,"keys"?[]}` (text e keys numa chamada: steer).
- `agent.send_keys`: `{"target","keys":[..]}`.
- `agent.prompt`: `{"target","text","wait"?:{"until":[status..],"timeout_ms"}}`.
- Eventos empurrados depois do subscribe: `{"event":"pane.agent_status_changed"|"pane.output_matched"|"pane.scroll_changed","data":{...}}`
  (ver `schema.subscription_event.json`). A forma exata da linha (se leva `id`) ainda
  precisa ser vista num evento real; em 2 s sem mudanca nao chegou nada.

## Fixtures (`tests/fixtures/`)

Respostas reais gravadas (`*.resposta.json`; `agent.list` cortado em 2 agentes) e recortes
do `herdr api schema --json` (`schema.requisicoes.json` com os 7 metodos e seus tipos,
`schema.resposta.json`, `schema.subscription_event.json`).
