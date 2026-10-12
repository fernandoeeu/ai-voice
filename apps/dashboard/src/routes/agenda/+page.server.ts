import { Calendar } from '@jarvis/apple';
import { RANGE_DAYS, addDays, dayKeyOf, dayStartMs, type AgendaRange } from '#lib/components/agenda/agenda-time.js';
import { runApple } from '#lib/server/apple-gateway.js';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ url }) => {
	const range: AgendaRange = url.searchParams.get('periodo') === 'semana' ? 'semana' : 'hoje';
	const requested = url.searchParams.get('calendario')?.trim() || null;

	const now = Date.now();
	const today = dayKeyOf(now);
	const from = new Date(dayStartMs(today)).toISOString();
	const to = new Date(dayStartMs(addDays(today, RANGE_DAYS[range]))).toISOString();

	// Sequential on purpose: the first call is the one macOS may prompt for.
	const calendars = await runApple(Calendar.use((calendar) => calendar.calendars));
	const events = calendars.ok
		? await runApple(
				Calendar.use((calendar) =>
					calendar.events(requested ? { from, to, calendar: requested } : { from, to })
				)
			)
		: ({ ok: false, failure: calendars.failure } as const);

	return { range, calendar: requested, now, today, calendars, events };
};
