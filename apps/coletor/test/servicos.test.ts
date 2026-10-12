import { describe, expect, it } from "@effect/vitest"
import { parsePs, Tabela } from "../src/processos.ts"
import { montarServicos, parseLaunchctlList, parseLaunchctlPrint } from "../src/servicos.ts"

const LIST = `PID\tStatus\tLabel
11223\t0\tdev.jarvis.dashboard
-\t0\tdev.jarvis.maestro
80087\t0\tdev.jarvis.escuta
69666\t0\tapplication.dev.jarvis.desktop.1.2
123\t0\tcom.apple.algo
`

const PRINT = `gui/501/dev.jarvis.escuta = {
\tactive count = 1
\tpath = /Users/f/Library/LaunchAgents/dev.jarvis.escuta.plist
\tstate = running
\tprogram = /x/Escuta
\truns = 4
\tpid = 80087
\tlast exit code = (never exited)
\tendpoints = {
\t\tstate = active
\t}
}
`

describe("launchctl", () => {
  it("lê só os dev.jarvis.* do list", () => {
    expect(parseLaunchctlList(LIST)).toEqual([
      { label: "dev.jarvis.dashboard", pid: 11223, ultimaSaida: 0 },
      { label: "dev.jarvis.maestro", pid: null, ultimaSaida: 0 },
      { label: "dev.jarvis.escuta", pid: 80087, ultimaSaida: 0 }
    ])
  })

  it("lê runs, pid e estado do print sem confundir com chaves aninhadas", () => {
    expect(parseLaunchctlPrint(PRINT)).toEqual({
      estado: "running",
      execucoes: 4,
      pid: 80087,
      ultimaSaida: "(never exited)"
    })
  })

  it("junta plists, launchctl e ps", () => {
    const tabela = new Tabela(parsePs("80087 1 2048 1.5 /x/Escuta\n"))
    const servicos = montarServicos(
      ["dev.jarvis.escuta", "dev.jarvis.tts"],
      parseLaunchctlList(LIST),
      new Map([["dev.jarvis.escuta", parseLaunchctlPrint(PRINT)]]),
      tabela
    )
    expect(servicos.map((s) => s.label)).toEqual([
      "dev.jarvis.dashboard",
      "dev.jarvis.escuta",
      "dev.jarvis.maestro",
      "dev.jarvis.tts"
    ])
    expect(servicos[1]).toEqual({
      label: "dev.jarvis.escuta",
      carregado: true,
      vivo: true,
      pid: 80087,
      reinicios: 3,
      ultima_saida: "(never exited)",
      rss_bytes: 2048 * 1024,
      cpu_pct: 1.5
    })
    expect(servicos[0]).toMatchObject({ vivo: false, pid: 11223, reinicios: null })
    expect(servicos[3]).toMatchObject({ carregado: false, vivo: false, pid: null })
  })
})
