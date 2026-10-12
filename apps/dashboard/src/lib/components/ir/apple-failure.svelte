<script lang="ts">
	import type { AppleFailure } from '#lib/apple/failure.js';
	import Notice from '#lib/components/jarvis/notice.svelte';

	let { failure, nothing }: { failure: AppleFailure; nothing: string } = $props();
</script>

{#if failure.kind === 'permission'}
	<Notice tone="caution" title="O macOS bloqueou a automação. {nothing}">
		<p>
			Abra Ajustes do Sistema, Privacidade e Segurança, Automação, e permita que o app que roda o dashboard (Terminal, iTerm ou
			bun) controle o aplicativo pedido. Depois tente de novo.
		</p>
	</Notice>
{:else}
	<Notice tone="alert" title={failure.kind === 'automation' ? `O macOS recusou o pedido. ${nothing}` : 'O pedido falhou no servidor.'}>
		<p>Tente de novo. Se repetir, veja o log do dashboard.</p>
		<p class="mt-1 text-xs">Resposta: {failure.message}</p>
	</Notice>
{/if}
