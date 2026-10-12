import { Effect, FileSystem, Option, Schema } from "effect"
import { mkdirSync } from "node:fs"
import { homedir } from "node:os"
import { join } from "node:path"
import { type MaestroConfig, paths } from "./config.ts"
import {
  createTab,
  createWorkspace,
  findAgent,
  getPane,
  type HerdrError,
  type HerdrRejected,
  listWorkspaces,
  startClaude
} from "./herdr.ts"
import { garantirEstado, gravar, lerModelo, renderizar } from "./memoria.ts"

const Sessao = Schema.fromJsonString(Schema.Struct({
  session_id: Schema.String,
  transcript_path: Schema.optionalKey(Schema.String)
}))

const PaneSalvo = Schema.fromJsonString(Schema.Struct({ pane_id: Schema.String }))

export type Resultado =
  | { readonly _tag: "Vivo"; readonly pane: string }
  | { readonly _tag: "Recriado"; readonly pane: string; readonly retomou: Option.Option<string> }

/** Argumentos do claude: a regra de delegação, a fala liberada e, se houver, a sessão a retomar. */
export const argsClaude = (options: {
  readonly sistema: string
  readonly fala: string
  readonly settings: string
  readonly resume: Option.Option<string>
}): ReadonlyArray<string> => [
  ...(Option.isSome(options.resume) ? ["--resume", options.resume.value] : []),
  "--append-system-prompt-file",
  options.sistema,
  "--allowedTools",
  `Bash(${options.fala}:*)`,
  ...(options.settings !== "" ? ["--settings", options.settings] : [])
]

/** Escreve CLAUDE.md da casa, o prompt de sistema e o estado.md inicial a partir dos modelos. */
export const prepararCasa = (config: MaestroConfig) =>
  Effect.sync(() => {
    const p = paths(config.dir)
    const estado = garantirEstado(config.dir, config.name)
    mkdirSync(p.casa, { recursive: true, mode: 0o700 })
    const valores = { NOME: config.name, FALA: config.fala, ESTADO: estado }
    gravar(join(p.casa, "CLAUDE.md"), renderizar(lerModelo("CLAUDE.md.in"), valores))
    gravar(p.sistema, renderizar(lerModelo("sistema.md.in"), valores))
  })

/** O id da última sessão, se o transcript ainda existir (sem ele o --resume falharia). */
export const sessaoSalva = (dir: string) =>
  Effect.gen(function*() {
    const fs = yield* FileSystem.FileSystem
    const sessao = yield* fs.readFileString(paths(dir).sessao).pipe(
      Effect.flatMap(Schema.decodeUnknownEffect(Sessao)),
      Effect.option
    )
    if (Option.isNone(sessao)) return Option.none<string>()
    const { session_id, transcript_path } = sessao.value
    const transcript = transcript_path ?? ""
    const existe = transcript !== "" && (yield* fs.exists(transcript).pipe(Effect.orElseSucceed(() => false)))
    if (existe) return Option.some(session_id)
    // Sem caminho salvo, procura o transcript em qualquer projeto do Claude Code.
    const glob = new Bun.Glob(`*/${session_id}.jsonl`)
    const achou = yield* Effect.promise(async () => {
      for await (const _ of glob.scan({ cwd: join(homedir(), ".claude", "projects"), onlyFiles: true })) return true
      return false
    }).pipe(Effect.orElseSucceed(() => false))
    return achou ? Option.some(session_id) : Option.none<string>()
  })

/** Um pane para o maestro: o último usado, se ainda existir e estiver livre, ou uma aba/workspace novo. */
const paneLivre = (config: MaestroConfig) =>
  Effect.gen(function*() {
    const fs = yield* FileSystem.FileSystem
    const p = paths(config.dir)
    const salvo = yield* fs.readFileString(p.pane).pipe(Effect.flatMap(Schema.decodeUnknownEffect(PaneSalvo)), Effect.option)
    if (Option.isSome(salvo)) {
      const pane = yield* getPane(salvo.value.pane_id)
      if (Option.isSome(pane) && pane.value.agent === undefined) return pane.value.pane_id
    }
    const env = { JARVIS_MAESTRO_DIR: config.dir, JARVIS_MAESTRO_NOME: config.name }
    const workspace = (yield* listWorkspaces).find((w) => w.label === config.workspace)
    const pane = workspace
      ? yield* createTab({ workspace: workspace.workspace_id, cwd: p.casa, env })
      : yield* createWorkspace({ cwd: p.casa, label: config.workspace, env })
    gravar(p.pane, JSON.stringify({ pane_id: pane }) + "\n")
    return pane
  })

/**
 * Uma passada do vigia: se o agente `config.name` sumiu do herdr, recria o
 * pane e inicia o claude retomando a sessão salva. Se a retomada falhar,
 * começa uma sessão nova no mesmo pane; o estado.md volta pelo hook.
 */
export const vigiar = (config: MaestroConfig) =>
  Effect.gen(function*() {
    const vivo = yield* findAgent(config.name)
    if (Option.isSome(vivo)) return { _tag: "Vivo", pane: vivo.value.pane_id } satisfies Resultado as Resultado

    yield* prepararCasa(config)
    const resume = yield* sessaoSalva(config.dir)
    const pane = yield* paneLivre(config)
    const base = { sistema: paths(config.dir).sistema, fala: config.fala, settings: config.settings }
    const iniciar = (resume: Option.Option<string>) =>
      startClaude({ name: config.name, pane, args: argsClaude({ ...base, resume }) })

    const retomou = yield* iniciar(resume).pipe(
      Effect.as(resume),
      Effect.catch((error: HerdrError | HerdrRejected) => {
        if (Option.isNone(resume)) return Effect.fail(error)
        return Effect.logWarning(`Não retomei a sessão ${resume.value} (${error.message}); começo uma nova.`).pipe(
          Effect.andThen(Effect.sleep("2 seconds")),
          Effect.andThen(findAgent(config.name)),
          // O claude pode ter subido mesmo com o start reclamando (por exemplo, bloqueado num diálogo).
          Effect.flatMap((agora) => Option.isSome(agora) ? Effect.succeed(resume) : iniciar(Option.none()).pipe(Effect.as(Option.none<string>())))
        )
      })
    )
    return { _tag: "Recriado", pane, retomou } satisfies Resultado as Resultado
  })
