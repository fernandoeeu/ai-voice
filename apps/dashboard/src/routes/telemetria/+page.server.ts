import { readPainel } from '#lib/server/telemetria.js';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async () => ({ painel: await readPainel() });
