#!/usr/bin/env bun
/**
 * Hook SessionStart (startup|resume|compact|clear) do maestro.
 *
 * Só age quando a sessão roda na casa do maestro (`<JARVIS_MAESTRO_DIR>/casa`):
 * grava o id da sessão em `sessao.json` (o vigia retoma com `--resume <id>`)
 * e devolve o `estado.md` como additionalContext. Fora disso, sai calado.
 */
import { paths } from "../config.ts"
import { ehMaestro, garantirEstado, gravar, maestroDir } from "../memoria.ts"
import { readFileSync } from "node:fs"

interface Entrada {
  readonly session_id?: string
  readonly transcript_path?: string
  readonly cwd?: string
  readonly source?: string
}

const entrada: Entrada = JSON.parse((await Bun.stdin.text()) || "{}")
const dir = maestroDir()
if (!ehMaestro(entrada.cwd, dir) || !entrada.session_id) process.exit(0)

const nome = process.env.JARVIS_MAESTRO_NOME || "maestro"
const estado = garantirEstado(dir, nome)
gravar(
  paths(dir).sessao,
  JSON.stringify(
    {
      session_id: entrada.session_id,
      source: entrada.source ?? "startup",
      transcript_path: entrada.transcript_path,
      cwd: entrada.cwd,
      em: new Date().toISOString()
    },
    null,
    2
  ) + "\n"
)

const conteudo = readFileSync(estado, "utf8")
const additionalContext = [
  `Memória do ${nome} (${estado}), reinjetada pelo hook SessionStart (${entrada.source ?? "startup"}).`,
  `Retome as pendências e confira os subagentes vivos antes de qualquer coisa; mantenha o arquivo atualizado.`,
  ``,
  conteudo
].join("\n")

process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: "SessionStart", additionalContext } }))
