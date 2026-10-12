/**
 * The Telemetria tab's view of the Jarvis: what the live collector (`/estado` on 127.0.0.1:7352) says right now,
 * plus the latency percentiles read from Prometheus.
 *
 * The collector's JSON is read through `normalizeEstado`, which accepts a few spellings of each field (pt and en,
 * camelCase and snake_case) and drops anything it cannot read. This is the provisional schema: when the
 * collector's own type (apps/coletor/src/estado.ts, `Retrato`) changes, adjust the readers below and keep `Estado` as the page's contract.
 */

export type Servico = {
	/** Loaded in launchd (false: installed but booted out). Null when unknown. */
	readonly carregado?: boolean | null;
	/** Short name: callback, dashboard, escuta, tts, telemetria… */
	readonly nome: string;
	/** launchd label, such as dev.jarvis.escuta. */
	readonly label: string;
	readonly vivo: boolean;
	readonly pid: number | null;
	readonly reinicios: number | null;
	readonly ramBytes: number | null;
	readonly cpuPct: number | null;
	/** Health check answered (HTTP 200), when the service has one. */
	readonly httpOk: boolean | null;
	readonly healthMs: number | null;
};

export type AgenteEstado = 'working' | 'blocked' | 'idle' | 'done' | 'unknown';

export type Agente = {
	readonly id: string;
	readonly nome: string;
	readonly workspace: string | null;
	readonly estado: AgenteEstado;
	/** When it entered the current state (ms since epoch). */
	readonly desde: number | null;
	readonly ramBytes: number | null;
	readonly cpuPct: number | null;
};

export type Herdr = {
	/** The herdr server answered. */
	readonly vivo: boolean;
	readonly versao: string | null;
	readonly workspaces: number | null;
	readonly agentes: ReadonlyArray<Agente>;
	readonly erro: string | null;
};

export type Interrupcao = {
	readonly at: number;
	/** What interrupted: fala, painel, novo pedido… */
	readonly motivo: string | null;
	/** What Jarvis was saying when cut. */
	readonly texto: string | null;
	/** From the interruption to the voice actually stopping. */
	readonly killMs: number | null;
};

export type Escuta = {
	readonly pedidos: number | null;
	readonly interrupcoesTotal: number | null;
	readonly entregaMs: Percentis | null;
	/** From the interruption to the voice stopping. */
	readonly killMs: Percentis | null;
	readonly interrupcoes: ReadonlyArray<Interrupcao>;
};

export type Percentis = { readonly p50: number | null; readonly p95: number | null; readonly n?: number | null };

export type Estado = {
	readonly geradoEm: number;
	readonly servicos: ReadonlyArray<Servico>;
	readonly herdr: Herdr | null;
	readonly escuta: Escuta | null;
};

export type Latencia = {
	readonly id: string;
	readonly grupo: 'voz' | 'entrega';
	readonly rotulo: string;
	readonly detalhe: string;
	readonly p50: number | null;
	readonly p95: number | null;
	/** How many samples went into the window. */
	readonly amostras: number | null;
};

export type Ponto = { readonly t: number; readonly cpu: number | null; readonly ram: number | null };

/** What the page receives on load and on every live frame. */
export type Painel = {
	readonly geradoEm: number;
	/** The collector answered with something readable. */
	readonly coletor: 'ok' | 'fora';
	readonly prometheus: 'ok' | 'fora';
	readonly estado: Estado | null;
	/** The probe's last reading of each dev.jarvis.* service from Prometheus; used when the collector is down. */
	readonly servicosProm: ReadonlyArray<Servico>;
	readonly latencias: ReadonlyArray<Latencia>;
	/** Recent CPU/RAM samples per agent id, oldest first. */
	readonly historico: Readonly<Record<string, ReadonlyArray<Ponto>>>;
	/** Where "Abrir painel completo" goes. */
	readonly painelUrl: string;
	/** Collector URL, shown in the empty state. */
	readonly coletorUrl: string;
};

/* ------------------------------------------------------------------------------------------------ */
/* Lenient readers                                                                                   */
/* ------------------------------------------------------------------------------------------------ */

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === 'object' && v !== null && !Array.isArray(v);

function pick(o: Obj, keys: ReadonlyArray<string>): unknown {
	for (const k of keys) if (o[k] !== undefined && o[k] !== null) return o[k];
	return undefined;
}

function num(v: unknown): number | null {
	if (typeof v === 'number' && Number.isFinite(v)) return v;
	if (typeof v === 'string' && v.trim() !== '' && Number.isFinite(Number(v))) return Number(v);
	return null;
}

function str(v: unknown): string | null {
	if (typeof v === 'string' && v.trim() !== '') return v;
	if (typeof v === 'number') return String(v);
	return null;
}

function bool(v: unknown): boolean | null {
	if (typeof v === 'boolean') return v;
	if (v === 1 || v === '1' || v === 'true' || v === 'running' || v === 'ok') return true;
	if (v === 0 || v === '0' || v === 'false' || v === 'stopped' || v === 'off') return false;
	return null;
}

/** Seconds or milliseconds since epoch, or an ISO string, to milliseconds. */
function instant(v: unknown): number | null {
	const n = num(v);
	if (n !== null) return n < 1e11 ? n * 1000 : n;
	if (typeof v === 'string') {
		const parsed = Date.parse(v);
		return Number.isNaN(parsed) ? null : parsed;
	}
	return null;
}

function list(v: unknown): Obj[] {
	if (Array.isArray(v)) return v.filter(isObj);
	// A map keyed by name: { escuta: {...}, tts: {...} }.
	if (isObj(v)) return Object.entries(v).flatMap(([k, x]) => (isObj(x) ? [{ __chave: k, ...x }] : []));
	return [];
}

const RAM_BYTES = ['ramBytes', 'ram_bytes', 'rssBytes', 'rss_bytes', 'rss', 'memoria', 'ram'];
const RAM_MB = ['ramMb', 'ram_mb', 'rssMb', 'rss_mb'];
const CPU = ['cpuPct', 'cpu_pct', 'cpu', 'cpuPercent'];

function ram(o: Obj): number | null {
	const bytes = num(pick(o, RAM_BYTES));
	if (bytes !== null) return bytes;
	const mb = num(pick(o, RAM_MB));
	return mb === null ? null : mb * 1024 * 1024;
}

function servico(o: Obj): Servico | null {
	const label = str(pick(o, ['label', 'servicoLabel', 'launchd'])) ?? null;
	const nome =
		str(pick(o, ['servico', 'nome', 'name', '__chave'])) ?? (label ? label.replace(/^dev\.jarvis\./, '') : null);
	if (!nome) return null;
	const pid = num(pick(o, ['pid']));
	const carregado = bool(pick(o, ['carregado', 'loaded']));
	const vivo = bool(pick(o, ['vivo', 'alive', 'up', 'rodando', 'running', 'estado', 'state'])) ?? pid !== null;
	return {
		carregado,
		nome: nome.replace(/^dev\.jarvis\./, ''),
		label: label ?? (nome.startsWith('dev.jarvis.') ? nome : `dev.jarvis.${nome}`),
		vivo,
		pid,
		reinicios: num(pick(o, ['reinicios', 'restarts', 'execucoes', 'runs'])),
		ramBytes: ram(o),
		cpuPct: num(pick(o, CPU)),
		httpOk: bool(pick(o, ['httpOk', 'http_ok', 'saudavel', 'healthy'])),
		healthMs: num(pick(o, ['healthMs', 'health_ms', 'latenciaMs']))
	};
}

function agenteEstado(v: unknown): AgenteEstado {
	const s = typeof v === 'string' ? v.toLowerCase() : '';
	if (['working', 'trabalhando', 'busy', 'running'].includes(s)) return 'working';
	if (['blocked', 'bloqueado', 'waiting', 'aguardando', 'needs_input'].includes(s)) return 'blocked';
	if (['idle', 'ocioso', 'parado'].includes(s)) return 'idle';
	if (['done', 'pronto', 'concluido', 'concluído', 'finished'].includes(s)) return 'done';
	return 'unknown';
}

function agente(o: Obj, now: number, workspace: string | null): Agente | null {
	const id = str(pick(o, ['id', 'pane', 'pane_id', 'agente', 'nome', 'name', '__chave']));
	if (!id) return null;
	const desdeDireto = instant(pick(o, ['desde', 'since', 'estadoDesde', 'estado_desde', 'changedAt']));
	const noEstadoS = num(pick(o, ['segundos_no_estado', 'tempoNoEstadoS', 'tempo_no_estado_s', 'segundosNoEstado']));
	// The agent's own process plus what it spawned (node, git, builds): what it really costs the Mac.
	const filhos = pick(o, ['filhos', 'children']);
	const somar = (a: number | null, b: number | null) => (a === null ? b : b === null ? a : a + b);
	const noEstadoMs = num(pick(o, ['tempoNoEstadoMs', 'tempo_no_estado_ms', 'noEstadoMs']));
	return {
		id,
		nome: str(pick(o, ['nome', 'name', 'titulo', 'title', 'agente'])) ?? id,
		workspace: str(pick(o, ['workspace', 'workspaceNome', 'ws'])) ?? workspace,
		estado: agenteEstado(pick(o, ['estado', 'state', 'status'])),
		desde: desdeDireto ?? (noEstadoMs !== null ? now - noEstadoMs : noEstadoS !== null ? now - noEstadoS * 1000 : null),
		ramBytes: somar(ram(o), isObj(filhos) ? ram(filhos) : null),
		cpuPct: somar(num(pick(o, CPU)), isObj(filhos) ? num(pick(filhos, CPU)) : null)
	};
}

function herdr(v: unknown, now: number): Herdr | null {
	if (!isObj(v)) return null;
	const ws = pick(v, ['workspaces']);
	const agentes: Agente[] = [];
	// Workspaces may come as a list holding their agents, or as a count beside a flat agent list.
	for (const w of Array.isArray(ws) ? list(ws) : []) {
		const nomeWs = str(pick(w, ['nome', 'name', 'id', '__chave']));
		for (const a of list(pick(w, ['agentes', 'agents', 'panes']))) {
			const parsed = agente(a, now, nomeWs);
			if (parsed) agentes.push(parsed);
		}
	}
	for (const a of list(pick(v, ['agentes', 'agents']))) {
		const parsed = agente(a, now, null);
		if (parsed && !agentes.some((x) => x.id === parsed.id)) agentes.push(parsed);
	}
	return {
		vivo: bool(pick(v, ['vivo', 'alive', 'up'])) ?? true,
		versao: str(pick(v, ['versao', 'version'])),
		workspaces: Array.isArray(ws) ? ws.length : num(ws),
		agentes,
		erro: str(pick(v, ['erro', 'error']))
	};
}

function percentis(v: unknown): Percentis | null {
	if (!isObj(v)) return null;
	const p50 = num(pick(v, ['p50', 'mediana', 'p50Ms', 'p50_ms']));
	const p95 = num(pick(v, ['p95', 'p95Ms', 'p95_ms']));
	if (num(pick(v, ['amostras', 'n', 'count'])) === 0) return null;
	if (p50 === null && p95 === null) return null;
	return { p50, p95, n: num(pick(v, ['n', 'amostras', 'count'])) };
}

function interrupcao(o: Obj): Interrupcao | null {
	const at = instant(pick(o, ['at', 'ts', 'em', 'timestamp', 'quando']));
	if (at === null) return null;
	const dono = str(pick(o, ['owner', 'motivo', 'reason', 'origem']));
	return {
		at,
		motivo: dono ? `voz: ${dono}` : null,
		texto: str(pick(o, ['heard', 'texto', 'text', 'fala'])),
		killMs: num(pick(o, ['kill_ms', 'killMs', 'paradaMs']))
	};
}

function escuta(v: unknown): Escuta | null {
	if (!isObj(v)) return null;
	const lat = pick(v, ['latencias', 'latencies']);
	const contadores = pick(v, ['contadores', 'counters']);
	const c = isObj(contadores) ? contadores : v;
	const explicitas = list(pick(v, ['interrupcoes', 'interruptions', 'ultimasInterrupcoes']));
	// The collector keeps the escuta's last events; the barge_in ones are the interruptions.
	const dosEventos = list(pick(v, ['ultimos_eventos', 'eventos'])).filter((e) => pick(e, ['type', 'tipo']) === 'barge_in');
	const interrupcoes = [...explicitas, ...dosEventos]
		.map(interrupcao)
		.filter((i): i is Interrupcao => i !== null)
		.sort((a, b) => b.at - a.at);
	const pedidos = pick(c, ['pedidos', 'requests']);
	return {
		pedidos: num(isObj(pedidos) ? pick(pedidos, ['total', 'hoje']) : pedidos),
		interrupcoesTotal: num(Array.isArray(pick(c, ['interrupcoes'])) ? null : pick(c, ['interrupcoes', 'interrupcoesTotal', 'interrupcoes_total'])),
		entregaMs: percentis(pick(v, ['entrega_ms', 'entregaMs']) ?? (isObj(lat) ? pick(lat, ['entrega', 'entregaMs', 'delivery']) : pick(v, ['entrega']))),
		killMs: percentis(pick(v, ['interrupcao_kill_ms', 'killMs'])),
		interrupcoes
	};
}

/** Reads whatever the collector sent; null when it is not an object at all. */
export function normalizeEstado(raw: unknown, now = Date.now()): Estado | null {
	if (!isObj(raw)) return null;
	const servicos = list(pick(raw, ['servicos', 'services']))
		.map(servico)
		.filter((s): s is Servico => s !== null);
	return {
		geradoEm: instant(pick(raw, ['geradoEm', 'gerado_em', 'at', 'ts', 'timestamp'])) ?? now,
		servicos,
		herdr: herdr(pick(raw, ['herdr']), now),
		escuta: escuta(pick(raw, ['escuta']))
	};
}

/* ------------------------------------------------------------------------------------------------ */
/* Formatting                                                                                        */
/* ------------------------------------------------------------------------------------------------ */

const oneDecimal = new Intl.NumberFormat('pt-BR', { maximumFractionDigits: 1 });
const whole = new Intl.NumberFormat('pt-BR', { maximumFractionDigits: 0 });

/** 312 MB · 1,4 GB */
export function bytes(n: number | null): string {
	if (n === null) return '—';
	const mb = n / (1024 * 1024);
	if (mb < 1) return `${whole.format(n / 1024)} KB`;
	if (mb < 1024) return `${whole.format(mb)} MB`;
	return `${oneDecimal.format(mb / 1024)} GB`;
}

export const pct = (n: number | null) => (n === null ? '—' : `${oneDecimal.format(n)}%`);

export const ESTADO_ROTULO: Record<AgenteEstado, string> = {
	working: 'Trabalhando',
	blocked: 'Esperando você',
	idle: 'Ocioso',
	done: 'Concluído',
	unknown: 'Sem estado'
};

/** Order on screen: what needs attention first. */
export const ESTADO_ORDEM: Record<AgenteEstado, number> = { blocked: 0, working: 1, done: 2, idle: 3, unknown: 4 };

export const vazio = (painelUrl: string, coletorUrl: string): Painel => ({
	geradoEm: Date.now(),
	coletor: 'fora',
	prometheus: 'fora',
	estado: null,
	servicosProm: [],
	latencias: [],
	historico: {},
	painelUrl,
	coletorUrl
});
