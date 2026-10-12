import type { Handle } from '@sveltejs/kit/hooks';
import { recordOperation } from '@jarvis/telemetry';
import { alexaRuntime } from '#lib/server/alexa-runtime.js';

// adapter-node emits this once every connection has closed, so in-flight requests finish first.
process.once('sveltekit:shutdown', () => alexaRuntime.dispose());

/**
 * Every request lands in `jarvis_operacao_ms` as `dashboard.requisicao`, labelled with the route id
 * (never the URL) and the method, so the Telemetria panel shows the dashboard's latency and errors.
 */
export const handle: Handle = async ({ event, resolve }) => {
	const start = performance.now();
	let status = 500;
	try {
		const response = await resolve(event);
		status = response.status;
		return response;
	} finally {
		const rota = event.route.id ?? 'sem-rota';
		alexaRuntime.runFork(
			recordOperation('dashboard.requisicao', performance.now() - start, status >= 500 ? 'erro' : 'ok', {
				servidor: 'dashboard',
				rota,
				metodo: event.request.method,
				status: String(status)
			})
		);
	}
};
