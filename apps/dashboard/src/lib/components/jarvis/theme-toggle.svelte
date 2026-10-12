<script lang="ts">
	import { mode, toggleMode } from 'mode-watcher';
	import Moon from 'phosphor-svelte/lib/Moon';
	import Sun from 'phosphor-svelte/lib/Sun';

	const dark = $derived(mode.current === 'dark');
</script>

<button
	type="button"
	onclick={toggleMode}
	class="grid size-9 place-items-center rounded-md text-ink-soft transition-[color,background-color,scale] duration-150 ease-out hover:bg-accent hover:text-ink active:scale-[0.96]"
	aria-label={dark ? 'Usar tema claro' : 'Usar tema escuro'}
	title={dark ? 'Tema claro' : 'Tema escuro'}
>
	{#if dark}
		<Sun class="size-[1.125rem]" weight="bold" />
	{:else}
		<Moon class="size-[1.125rem]" weight="bold" />
	{/if}
</button>
