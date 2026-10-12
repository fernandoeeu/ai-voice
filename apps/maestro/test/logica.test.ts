import { describe, expect, it } from "@effect/vitest"
import { Option } from "effect"
import { mkdirSync, mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { decidir } from "../src/entrega.ts"
import { formatarEntrega, limparOrigem } from "../src/fila.ts"
import { atualizarAuto, AUTO_FIM, AUTO_INICIO, ehMaestro, renderizar, resumirTranscript } from "../src/memoria.ts"
import { devidas, type Rotina } from "../src/rotinas.ts"
import { argsClaude } from "../src/vigia.ts"

describe("decidir", () => {
  const base = { pendentes: 2, avisado: false }
  it("entrega só com o maestro idle ou done", () => {
    expect(decidir({ ...base, status: Option.some("idle") })).toBe("entregar")
    expect(decidir({ ...base, status: Option.some("done") })).toBe("entregar")
    expect(decidir({ ...base, status: Option.some("working") })).toBe("esperar")
    expect(decidir({ ...base, status: Option.some("unknown") })).toBe("esperar")
    expect(decidir({ ...base, status: Option.none() })).toBe("esperar")
  })
  it("avisa uma vez por bloqueio", () => {
    expect(decidir({ ...base, status: Option.some("blocked") })).toBe("avisar")
    expect(decidir({ ...base, avisado: true, status: Option.some("blocked") })).toBe("esperar")
  })
  it("fila vazia nunca faz nada", () => {
    expect(decidir({ pendentes: 0, avisado: false, status: Option.some("idle") })).toBe("esperar")
  })
})

describe("fila", () => {
  it("junta os eventos com o marcador", () => {
    const eventos = [
      { id: "1", origem: "rotina:bom-dia", texto: "Resuma a agenda. ", recebido: "" },
      { id: "2", origem: "teste", texto: "responda só ok", recebido: "" }
    ]
    expect(formatarEntrega(eventos)).toBe("[evento: rotina:bom-dia] Resuma a agenda.\n\n[evento: teste] responda só ok")
  })
  it("limpa a origem", () => {
    expect(limparOrigem("  a]b\n[c  ")).toBe("a b c")
    expect(limparOrigem("")).toBe("desconhecida")
    expect(limparOrigem("x".repeat(100))).toHaveLength(60)
  })
})

describe("rotinas", () => {
  const rotina: Rotina = { nome: "bom-dia", horario: "08:30", texto: "oi" }
  // 2026-10-12 é uma segunda-feira.
  const em = (h: number, m: number) => new Date(2026, 9, 12, h, m)
  it("dispara no horário e até 10 min depois, uma vez por dia", () => {
    expect(devidas([rotina], em(8, 29), {})).toHaveLength(0)
    expect(devidas([rotina], em(8, 30), {})).toHaveLength(1)
    expect(devidas([rotina], em(8, 39), {})).toHaveLength(1)
    expect(devidas([rotina], em(8, 40), {})).toHaveLength(0)
    expect(devidas([rotina], em(8, 31), { "bom-dia": "2026-10-12" })).toHaveLength(0)
    expect(devidas([rotina], em(8, 31), { "bom-dia": "2026-10-11" })).toHaveLength(1)
  })
  it("respeita os dias da semana", () => {
    expect(devidas([{ ...rotina, dias: ["seg"] }], em(8, 30), {})).toHaveLength(1)
    expect(devidas([{ ...rotina, dias: ["sab", "dom"] }], em(8, 30), {})).toHaveLength(0)
  })
})

describe("memória", () => {
  it("substitui só o bloco automático", () => {
    const primeiro = atualizarAuto("# Estado\n\n## Pendências\n- x\n", "bloco 1")
    expect(primeiro).toContain(`${AUTO_INICIO}\nbloco 1\n${AUTO_FIM}`)
    const segundo = atualizarAuto(primeiro, "bloco 2")
    expect(segundo).toContain("## Pendências\n- x")
    expect(segundo).toContain("bloco 2")
    expect(segundo).not.toContain("bloco 1")
  })
  it("resume o transcript", () => {
    const linhas = [
      { type: "user", message: { content: "<system-reminder>ignorar</system-reminder>" } },
      { type: "user", message: { content: "corrija o build" } },
      { type: "user", isMeta: true, message: { content: "meta" } },
      {
        type: "assistant",
        message: {
          content: [
            { type: "tool_use", name: "Agent", input: { description: "Corrigir build" } },
            { type: "text", text: "Lancei um subagente." }
          ]
        }
      },
      { type: "user", message: { content: [{ type: "tool_result", content: "x" }] } }
    ].map((l) => JSON.stringify(l)).join("\n") + "\nlixo\n"
    expect(resumirTranscript(linhas)).toEqual({
      pedidos: ["corrija o build"],
      subagentes: ["Corrigir build"],
      ultimaResposta: "Lancei um subagente."
    })
  })
  it("reconhece só a casa do maestro", () => {
    const dir = mkdtempSync(join(tmpdir(), "maestro-"))
    mkdirSync(join(dir, "casa", "sub"), { recursive: true })
    expect(ehMaestro(join(dir, "casa"), dir)).toBe(true)
    expect(ehMaestro(join(dir, "casa", "sub"), dir)).toBe(true)
    expect(ehMaestro(dir, dir)).toBe(false)
    expect(ehMaestro(join(dir, "casa2"), dir)).toBe(false)
    expect(ehMaestro(undefined, dir)).toBe(false)
  })
  it("renderiza os modelos", () => {
    expect(renderizar("@NOME@ fala com @FALA@ @NOME@", { NOME: "m", FALA: "f" })).toBe("m fala com f m")
  })
})

describe("vigia", () => {
  it("monta os argumentos do claude", () => {
    const base = { sistema: "/s.md", fala: "/bin/fala", settings: "" }
    expect(argsClaude({ ...base, resume: Option.none() })).toEqual([
      "--append-system-prompt-file", "/s.md", "--allowedTools", "Bash(/bin/fala:*)"
    ])
    expect(argsClaude({ ...base, settings: "/h.json", resume: Option.some("abc") })).toEqual([
      "--resume", "abc", "--append-system-prompt-file", "/s.md", "--allowedTools", "Bash(/bin/fala:*)",
      "--settings", "/h.json"
    ])
  })
})
