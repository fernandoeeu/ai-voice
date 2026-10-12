import type { CalendarEvent } from '@jarvis/apple';

export const ZONE = 'America/Sao_Paulo';
const DAY_MS = 24 * 60 * 60 * 1000;
// Brazil has no DST since 2019, so a fixed offset is exact for day boundaries.
const OFFSET = '-03:00';

export type AgendaRange = 'hoje' | 'semana';
export const RANGE_DAYS: Record<AgendaRange, number> = { hoje: 1, semana: 7 };

const partsFormat = new Intl.DateTimeFormat('en-CA', {
	timeZone: ZONE,
	year: 'numeric',
	month: '2-digit',
	day: '2-digit',
	hour: '2-digit',
	minute: '2-digit',
	hourCycle: 'h23'
});

const parts = (ms: number) => {
	const map: Record<string, string> = {};
	for (const part of partsFormat.formatToParts(ms)) map[part.type] = part.value;
	return map;
};

/** `YYYY-MM-DD` of an instant, as seen in São Paulo. */
export const dayKeyOf = (ms: number) => {
	const p = parts(ms);
	return `${p.year}-${p.month}-${p.day}`;
};

export const dayStartMs = (key: string) => Date.parse(`${key}T00:00:00${OFFSET}`);

export const addDays = (key: string, days: number) => dayKeyOf(dayStartMs(key) + days * DAY_MS + DAY_MS / 2);

/** Minutes since local midnight. */
export const minutesOfDay = (ms: number) => {
	const p = parts(ms);
	return Number(p.hour) * 60 + Number(p.minute);
};

const timeFormat = new Intl.DateTimeFormat('pt-BR', {
	timeZone: ZONE,
	hour: '2-digit',
	minute: '2-digit',
	hourCycle: 'h23'
});
export const formatTime = (ms: number) => timeFormat.format(ms);

export const formatHour = (hour: number) => `${String(hour).padStart(2, '0')}:00`;

const longDay = new Intl.DateTimeFormat('pt-BR', {
	timeZone: ZONE,
	weekday: 'long',
	day: 'numeric',
	month: 'long'
});
const weekdayShort = new Intl.DateTimeFormat('pt-BR', { timeZone: ZONE, weekday: 'short' });
const dayMonth = new Intl.DateTimeFormat('pt-BR', { timeZone: ZONE, day: 'numeric', month: 'short' });

export const formatLongDay = (key: string) => longDay.format(dayStartMs(key) + DAY_MS / 2);
export const formatWeekday = (key: string) => weekdayShort.format(dayStartMs(key) + DAY_MS / 2).replace('.', '');
export const formatDayMonth = (key: string) => dayMonth.format(dayStartMs(key) + DAY_MS / 2).replace('.', '');

export type Occurrence = {
	readonly event: CalendarEvent;
	readonly start: number;
	readonly end: number;
};

export const occurrences = (events: ReadonlyArray<CalendarEvent>): Array<Occurrence> =>
	events.map((event) => {
		const start = Date.parse(event.start);
		return { event, start, end: Math.max(Date.parse(event.end), start + 1) };
	});

/** Events that touch the given local day. */
export const forDay = (items: ReadonlyArray<Occurrence>, key: string) => {
	const from = dayStartMs(key);
	const to = from + DAY_MS;
	return items.filter((item) => item.start < to && item.end > from);
};

/** Stable pick among a few ink tones, so a calendar keeps its color everywhere. */
export const TONES = ['carbon', 'signal', 'caution', 'soft'] as const;
export type Tone = (typeof TONES)[number];
export const toneOf = (calendar: string): Tone => {
	let hash = 0;
	for (const char of calendar) hash = (hash * 31 + char.codePointAt(0)!) >>> 0;
	return TONES[hash % TONES.length]!;
};

export type Placed = {
	readonly item: Occurrence;
	/** Minutes from the top of the visible window. */
	readonly top: number;
	readonly height: number;
	readonly lane: number;
	readonly lanes: number;
};

const MIN_VISUAL = 30;

/** Splits overlapping events into side-by-side lanes, clamped to the given day. */
export const layoutDay = (items: ReadonlyArray<Occurrence>, key: string, windowStart: number): Array<Placed> => {
	const dayStart = dayStartMs(key);
	const clamped = items
		.map((item) => {
			const start = Math.max(0, Math.round((item.start - dayStart) / 60000));
			const end = Math.min(24 * 60, Math.round((item.end - dayStart) / 60000));
			return { item, start, end: Math.max(end, start + MIN_VISUAL) };
		})
		.sort((a, b) => a.start - b.start || b.end - a.end);

	const placed: Array<Placed> = [];
	let cluster: typeof clamped = [];
	let clusterEnd = -1;

	const flush = () => {
		const laneEnds: Array<number> = [];
		const assigned = cluster.map((entry) => {
			let lane = laneEnds.findIndex((end) => end <= entry.start);
			if (lane === -1) lane = laneEnds.length;
			laneEnds[lane] = entry.end;
			return { entry, lane };
		});
		for (const { entry, lane } of assigned) {
			placed.push({
				item: entry.item,
				top: entry.start - windowStart,
				height: entry.end - entry.start,
				lane,
				lanes: laneEnds.length
			});
		}
		cluster = [];
		clusterEnd = -1;
	};

	for (const entry of clamped) {
		if (cluster.length > 0 && entry.start >= clusterEnd) flush();
		cluster.push(entry);
		clusterEnd = Math.max(clusterEnd, entry.end);
	}
	if (cluster.length > 0) flush();
	return placed;
};

export const toneBar: Record<Tone, string> = {
	carbon: 'bg-carbon',
	signal: 'bg-signal',
	caution: 'bg-caution',
	soft: 'bg-ink-soft'
};

/** How an occurrence reads inside one day's row, trimming the parts that belong to other days. */
export const timeLabel = (item: Occurrence, key: string) => {
	if (item.event.allDay) return 'Dia inteiro';
	const from = dayStartMs(key);
	const to = from + DAY_MS;
	if (item.start < from) return `até ${formatTime(item.end)}`;
	if (item.end > to) return `${formatTime(item.start)} →`;
	return `${formatTime(item.start)}–${formatTime(item.end)}`;
};
