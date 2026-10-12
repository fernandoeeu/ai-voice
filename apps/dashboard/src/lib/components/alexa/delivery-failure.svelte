<script lang="ts">
	import type { AlexaFailure } from '#lib/alexa/failure.js';
	import Notice from '#lib/components/jarvis/notice.svelte';

	let { failure }: { failure: AlexaFailure } = $props();

	const titles: Record<AlexaFailure['kind'], string> = {
		session: 'A sessão da Amazon expirou. Nada foi enviado.',
		device: 'O Echo não pôde receber. Nada foi enviado.',
		speech: 'A Alexa recusou o pedido. Nada foi enviado.',
		config: 'A configuração do servidor está incompleta.',
		unexpected: 'O envio falhou no servidor.'
	};

	const recovery: Record<AlexaFailure['kind'], string> = {
		session: 'Rode bun apps/mcp/src/main.ts login no terminal e tente de novo.',
		device: 'Confira se o Echo está ligado e online, depois tente de novo.',
		speech: 'Revise o texto ou espere o envio anterior terminar.',
		config: 'Revise as variáveis AI_VOICE_* e reinicie o dashboard.',
		unexpected: 'Tente de novo. Se repetir, veja o log do servidor.'
	};
</script>

<Notice tone="alert" title={titles[failure.kind]}>
	<p>{recovery[failure.kind]}</p>
	<p class="mt-1 text-xs">Resposta: {failure.message}</p>
</Notice>
