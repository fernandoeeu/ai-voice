import { Alexa, AlexaConfig, type AlexaError, type Echo, type SpeechAccepted } from '@jarvis/alexa';
import { Cause, type Effect, Exit, Option } from 'effect';
import type { AlexaFailure, Outcome } from '#lib/alexa/failure.js';
import { alexaRuntime } from './alexa-runtime.js';

export type AlexaSettings = {
	readonly speechEnabled: boolean;
	readonly defaultDevice: string | null;
};

const kinds = {
	SessionError: 'session',
	DeviceError: 'device',
	SpeechError: 'speech'
} as const satisfies Record<AlexaError['_tag'], AlexaFailure['kind']>;

const toFailure = (cause: Cause.Cause<unknown>): AlexaFailure => {
	const error = Cause.findErrorOption(cause);
	if (Option.isSome(error)) {
		const value = error.value;
		if (isAlexaError(value)) return { kind: kinds[value._tag], message: value.message };
		if (isConfigError(value)) {
			return { kind: 'config', message: 'As variáveis AI_VOICE_* não puderam ser lidas.' };
		}
	}
	// Defects can carry cookies or tokens from the Amazon client: log them here, never send them.
	console.error(Cause.pretty(cause));
	return { kind: 'unexpected', message: 'Algo falhou no servidor. Veja o terminal do dashboard.' };
};

const isAlexaError = (value: unknown): value is AlexaError =>
	typeof value === 'object' && value !== null && '_tag' in value && Object.hasOwn(kinds, value._tag as string);

const isConfigError = (value: unknown) =>
	typeof value === 'object' && value !== null && '_tag' in value && value._tag === 'ConfigError';

const run = async <A>(effect: Effect.Effect<A, AlexaError, Alexa | AlexaConfig>): Promise<Outcome<A>> => {
	const exit = await alexaRuntime.runPromiseExit(effect);
	return Exit.isSuccess(exit) ? { ok: true, value: exit.value } : { ok: false, failure: toFailure(exit.cause) };
};

export const readSettings = () =>
	run(
		AlexaConfig.useSync(
			(config): AlexaSettings => ({
				speechEnabled: config.speechEnabled,
				defaultDevice: Option.getOrNull(config.defaultDevice)
			})
		)
	);

export const listDevices = (): Promise<Outcome<ReadonlyArray<Echo>>> =>
	run(Alexa.use((alexa) => alexa.devices));

export const speak = (input: { readonly text: string; readonly deviceSerial?: string }): Promise<Outcome<SpeechAccepted>> =>
	run(Alexa.use((alexa) => alexa.speak(input)));
