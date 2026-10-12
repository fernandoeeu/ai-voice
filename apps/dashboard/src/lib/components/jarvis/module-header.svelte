<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		summary,
		actions
	}: { title: string; summary?: Snippet; actions?: Snippet } = $props();
</script>

<header class="flex flex-wrap items-end justify-between gap-x-6 gap-y-3 border-b border-rule pb-5">
	<div class="min-w-0">
		<h1 class="display text-[2rem] leading-[1.05] text-ink sm:text-[2.5rem]">{title}</h1>
		{#if summary}
			<p class="mt-2 text-[0.9375rem] text-ink-soft">{@render summary()}</p>
		{/if}
	</div>
	{#if actions}
		<div class="flex items-center gap-2">{@render actions()}</div>
	{/if}
</header>
