import type { AutomationError } from '@jarvis/apple';
import { Cause, type Effect, Exit, Option } from 'effect';
import type { AppleFailure, AppleOutcome } from '#lib/apple/failure.js';
import { type AppleServices, appleRuntime } from './apple-runtime.js';

// Must track the hint written by @jarvis/apple's Osascript for -1743/-1744.
const PERMISSION_DENIED = /denied Automation permission/i;
const TIMED_OUT = /did not finish within/i;

const toFailure = (cause: Cause.Cause<unknown>): AppleFailure => {
	const error = Cause.findErrorOption(cause);
	if (Option.isSome(error) && isAutomationError(error.value)) {
		const { message } = error.value;
		if (PERMISSION_DENIED.test(message)) {
			return { kind: 'permission', message: 'O macOS negou ao Jarvis a permissão de Automação para este app.' };
		}
		if (TIMED_OUT.test(message)) {
			return {
				kind: 'timeout',
				message: 'O app não respondeu a tempo. Se o macOS mostrou um pedido de permissão, responda e tente de novo.'
			};
		}
		return { kind: 'automation', message };
	}
	console.error(Cause.pretty(cause));
	return { kind: 'unexpected', message: 'Algo falhou no servidor. Veja o terminal do dashboard.' };
};

const isAutomationError = (value: unknown): value is AutomationError =>
	typeof value === 'object' && value !== null && '_tag' in value && value._tag === 'AutomationError';

/** Runs an effect against the macOS apps and returns a result that is safe to hand to the page. */
export const runApple = async <A>(
	effect: Effect.Effect<A, AutomationError, AppleServices>
): Promise<AppleOutcome<A>> => {
	const exit = await appleRuntime.runPromiseExit(effect);
	return Exit.isSuccess(exit) ? { ok: true, value: exit.value } : { ok: false, failure: toFailure(exit.cause) };
};
