<script lang="ts">
	import { toneBar, toneOf, type Occurrence } from './agenda-time.js';

	let { items }: { items: ReadonlyArray<Occurrence> } = $props();
</script>

<section aria-label="Dia inteiro" class="grid gap-2 sm:grid-cols-[3.75rem_1fr] sm:gap-4">
	<h2 class="field-label pt-1.5">Dia todo</h2>
	<ul class="flex flex-wrap gap-2">
		{#each items as { event } (event.id + event.start)}
			<li
				class="relative flex max-w-full items-center gap-2 overflow-hidden rounded-md border border-rule bg-paper-sunk py-1.5 pr-3 pl-4"
			>
				<span class={['absolute inset-y-0 left-0 w-[3px]', toneBar[toneOf(event.calendar)]]} aria-hidden="true"
				></span>
				<span class="truncate text-sm font-semibold text-ink">{event.title}</span>
				<span class="shrink-0 text-xs text-ink-soft">{event.calendar}</span>
			</li>
		{/each}
	</ul>
</section>
