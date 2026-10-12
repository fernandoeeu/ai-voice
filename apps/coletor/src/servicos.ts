import type { Tabela } from "./processos.ts"

/**
 * LaunchAgents `dev.jarvis.*`, só para o /estado. As métricas de saúde deles
 * (jarvis_servico_*) são da sonda do backend; o coletor não as exporta.
 */

export const PREFIXO = "dev.jarvis."

export interface LinhaLaunchctl {
  readonly label: string
  readonly pid: number | null
  readonly ultimaSaida: number | null
}

/** `launchctl list`: `PID\tStatus\tLabel`, com `-` quando não há PID. */
export const parseLaunchctlList = (saida: string): Array<LinhaLaunchctl> => {
  const linhas: Array<LinhaLaunchctl> = []
  for (const linha of saida.split("\n")) {
    const partes = linha.split("\t")
    if (partes.length < 3) continue
    const label = partes[2]!.trim()
    if (!label.startsWith(PREFIXO)) continue
    const pid = /^\d+$/.test(partes[0]!.trim()) ? Number(partes[0]) : null
    const status = /^-?\d+$/.test(partes[1]!.trim()) ? Number(partes[1]) : null
    linhas.push({ label, pid, ultimaSaida: status })
  }
  return linhas
}

export interface DetalheLaunchctl {
  readonly estado: string | null
  readonly execucoes: number | null
  readonly pid: number | null
  readonly ultimaSaida: string | null
}

/** `launchctl print gui/<uid>/<label>`: só as chaves do primeiro nível do serviço. */
export const parseLaunchctlPrint = (saida: string): DetalheLaunchctl => {
  const campo = (nome: string) => new RegExp(`^\\t${nome} = (.+)$`, "m").exec(saida)?.[1]?.trim() ?? null
  const runs = campo("runs")
  const pid = campo("pid")
  return {
    estado: campo("state"),
    execucoes: runs !== null && /^\d+$/.test(runs) ? Number(runs) : null,
    pid: pid !== null && /^\d+$/.test(pid) ? Number(pid) : null,
    ultimaSaida: campo("last exit code")
  }
}

export interface Servico {
  readonly label: string
  readonly carregado: boolean
  readonly vivo: boolean
  readonly pid: number | null
  /** Execuções menos a primeira, desde que o launchd carregou o serviço. */
  readonly reinicios: number | null
  readonly ultima_saida: string | null
  readonly rss_bytes: number
  readonly cpu_pct: number
}

export const montarServicos = (
  plists: ReadonlyArray<string>,
  lista: ReadonlyArray<LinhaLaunchctl>,
  detalhes: ReadonlyMap<string, DetalheLaunchctl>,
  tabela: Tabela
): Array<Servico> => {
  const porLabel = new Map(lista.map((l) => [l.label, l]))
  const labels = [...new Set([...plists, ...porLabel.keys()])].sort()
  return labels.map((label): Servico => {
    const linha = porLabel.get(label)
    const detalhe = detalhes.get(label)
    const pid = detalhe?.pid ?? linha?.pid ?? null
    const processo = pid !== null ? tabela.porPid.get(pid) : undefined
    const ultimaSaida = detalhe?.ultimaSaida ?? (linha?.ultimaSaida != null ? String(linha.ultimaSaida) : null)
    return {
      label,
      carregado: linha !== undefined,
      vivo: pid !== null && processo !== undefined,
      pid,
      reinicios: detalhe?.execucoes != null ? Math.max(0, detalhe.execucoes - 1) : null,
      ultima_saida: ultimaSaida,
      rss_bytes: processo?.rssBytes ?? 0,
      cpu_pct: processo?.cpuPct ?? 0
    }
  })
}
