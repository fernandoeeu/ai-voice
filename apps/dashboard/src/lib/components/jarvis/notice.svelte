<script lang="ts">
	import type { Snippet } from 'svelte';
	import Warning from 'phosphor-svelte/lib/Warning';
	import WarningOctagon from 'phosphor-svelte/lib/WarningOctagon';
	import Info from 'phosphor-svelte/lib/Info';

	type Tone = 'caution' | 'alert' | 'info';

	let {
		tone = 'info',
		title,
		children,
		class: className
	}: { tone?: Tone; title: string; children?: Snippet; class?: string } = $props();

	const icons = { caution: Warning, alert: WarningOctagon, info: Info };
	const Icon = $derived(icons[tone]);
</script>

<div
	role={tone === 'alert' ? 'alert' : 'status'}
	class={[
		'grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 rounded-md border px-4 py-3.5 text-sm',
		tone === 'caution' && 'border-caution/40 bg-caution/8',
		tone === 'alert' && 'border-alert/40 bg-alert/8',
		tone === 'info' && 'border-rule bg-paper-sunk',
		className
	]}
>
	<Icon
		class={[
			'mt-px size-[1.125rem]',
			tone === 'caution' && 'text-caution',
			tone === 'alert' && 'text-alert',
			tone === 'info' && 'text-carbon'
		]}
		weight="bold"
		aria-hidden="true"
	/>
	<p class="font-semibold text-ink">{title}</p>
	{#if children}
		<div class="col-start-2 text-ink-soft [&_code]:text-ink">{@render children()}</div>
	{/if}
</div>
