<script lang="ts">
	import { refreshAll } from '$app/navigation';
	import ArrowClockwise from 'phosphor-svelte/lib/ArrowClockwise';
	import { Button } from '#lib/components/ui/button/index.js';
	import CopyCommand from '#lib/components/jarvis/copy-command.svelte';

	let { message }: { message: string } = $props();

	let checking = $state(false);

	async function check() {
		checking = true;
		await refreshAll().finally(() => (checking = false));
	}
</script>

<section aria-labelledby="session-title" class="max-w-[38rem]">
	<h2 id="session-title" class="display text-[1.375rem] leading-tight text-ink">Entre na sua conta da Amazon</h2>
	<p class="mt-2 text-[0.9375rem] text-ink-soft">
		O Jarvis ainda não tem uma sessão válida para falar com a Alexa. O login é feito uma vez, pelo terminal, na raiz do projeto.
	</p>

	<ol class="mt-6 grid gap-5">
		<li class="grid gap-2.5">
			<span class="text-sm font-semibold text-ink">1. Rode o login e siga as instruções</span>
			<CopyCommand command="bun apps/mcp/src/main.ts login" />
		</li>
		<li class="grid gap-2.5">
			<span class="text-sm font-semibold text-ink">2. Volte aqui e confira</span>
			<div>
				<Button variant="outline" onclick={check} disabled={checking}>
					<ArrowClockwise class={['size-4', checking && 'animate-spin']} weight="bold" aria-hidden="true" />
					{checking ? 'Verificando…' : 'Verificar de novo'}
				</Button>
			</div>
		</li>
	</ol>

	<p class="mt-6 border-t border-rule pt-3 text-xs text-ink-soft">
		Resposta da Amazon: <span class="text-ink">{message}</span>
	</p>
</section>
