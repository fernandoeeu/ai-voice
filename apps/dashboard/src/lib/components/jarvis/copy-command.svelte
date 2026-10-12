<script lang="ts">
	import Check from 'phosphor-svelte/lib/Check';
	import Copy from 'phosphor-svelte/lib/Copy';

	let { command }: { command: string } = $props();

	let copied = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;

	async function copy() {
		await navigator.clipboard.writeText(command);
		copied = true;
		clearTimeout(timer);
		timer = setTimeout(() => (copied = false), 1600);
	}
</script>

<div class="flex items-stretch overflow-hidden rounded-md border border-rule-strong bg-paper">
	<code class="flex-1 overflow-x-auto px-3 py-2.5 text-[0.8125rem] whitespace-nowrap text-ink">
		<span class="text-ink-soft select-none" aria-hidden="true">$ </span>{command}
	</code>
	<button
		type="button"
		onclick={copy}
		class="flex items-center gap-1.5 border-l border-rule-strong px-3 text-xs font-semibold text-carbon transition-[background-color,scale] duration-150 ease-out hover:bg-accent active:scale-[0.97]"
	>
		{#if copied}
			<Check class="size-4" weight="bold" aria-hidden="true" /> Copiado
		{:else}
			<Copy class="size-4" weight="bold" aria-hidden="true" /> Copiar
		{/if}
	</button>
	<span class="sr-only" aria-live="polite">{copied ? 'Comando copiado' : ''}</span>
</div>
