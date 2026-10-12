import { MAX_TEXT_LENGTH } from '@jarvis/alexa';
import { fail } from '@sveltejs/kit';
import type { AlexaFailure } from '#lib/alexa/failure.js';
import { listDevices, readSettings, speak } from '#lib/server/alexa-gateway.js';
import { parseSpeakForm } from '#lib/server/speak-form.js';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async () => {
	const [settings, devices] = await Promise.all([readSettings(), listDevices()]);
	return { settings, devices, maxLength: MAX_TEXT_LENGTH };
};

const status: Record<AlexaFailure['kind'], number> = {
	session: 401,
	device: 409,
	speech: 422,
	config: 424,
	unexpected: 424
};

export const actions = {
	speak: async ({ request }) => {
		const form = parseSpeakForm(await request.formData());
		if (!form.ok) return fail(400, { text: form.text, device: form.device, invalid: form.message });

		const outcome = await speak(form.input);
		if (!outcome.ok) {
			return fail(status[outcome.failure.kind], {
				text: form.text,
				device: form.device,
				failure: outcome.failure
			});
		}
		return { accepted: { ...outcome.value, text: form.input.text, at: Date.now() } };
	}
} satisfies Actions;
