<script lang="ts">
	import { toneBar, toneOf, type AgendaRange } from './agenda-time.js';

	let {
		range,
		calendar,
		calendars
	}: { range: AgendaRange; calendar: string | null; calendars: ReadonlyArray<string> } = $props();

	const href = (next: { range?: AgendaRange; calendar?: string | null }) => {
		const params = new URLSearchParams();
		const r = next.range ?? range;
		const c = next.calendar === undefined ? calendar : next.calendar;
		if (r === 'semana') params.set('periodo', 'semana');
		if (c) params.set('calendario', c);
		const query = params.toString();
		return query ? `?${query}` : '?';
	};

	const ranges: ReadonlyArray<{ id: AgendaRange; label: string }> = [
		{ id: 'hoje', label: 'Hoje' },
		{ id: 'semana', label: 'Próximos 7 dias' }
	];

	const chip =
		'inline-flex h-8 items-center gap-2 rounded-md border px-3 text-[0.8125rem] font-medium transition-[background-color,border-color,color,scale] duration-150 ease-out active:scale-[0.97]';
</script>

<div class="flex flex-wrap items-center justify-between gap-x-6 gap-y-3">
	<nav aria-label="Período" class="inline-flex rounded-md border border-rule-strong bg-paper-sunk p-0.5">
		{#each ranges as item (item.id)}
			{@const current = item.id === range}
			<a
				href={href({ range: item.id })}
				data-sveltekit-noscroll
				aria-current={current ? 'page' : undefined}
				class={[
					'rounded-[calc(var(--radius-md)-1px)] px-3.5 py-1.5 text-sm font-semibold transition-[background-color,color,box-shadow,scale] duration-150 ease-out active:scale-[0.97]',
					current
						? 'bg-paper text-ink shadow-[0_0_0_1px_var(--rule-strong)]'
						: 'text-ink-soft hover:text-ink'
				]}
			>
				{item.label}
			</a>
		{/each}
	</nav>

	{#if calendars.length > 1}
		<nav aria-label="Calendário" class="flex flex-wrap items-center gap-1.5">
			<span class="field-label mr-1">Calendário</span>
			<a
				href={href({ calendar: null })}
				data-sveltekit-noscroll
				aria-current={calendar === null ? 'page' : undefined}
				class={[
					chip,
					calendar === null
						? 'border-ink bg-ink text-paper'
						: 'border-rule-strong bg-paper text-ink hover:bg-accent'
				]}
			>
				Todos
			</a>
			{#each calendars as name (name)}
				{@const current = name === calendar}
				<a
					href={href({ calendar: name })}
					data-sveltekit-noscroll
					aria-current={current ? 'page' : undefined}
					class={[
						chip,
						current ? 'border-ink bg-ink text-paper' : 'border-rule-strong bg-paper text-ink hover:bg-accent'
					]}
				>
					<span class={['size-2 rounded-full', toneBar[toneOf(name)]]} aria-hidden="true"></span>
					{name}
				</a>
			{/each}
		</nav>
	{/if}
</div>
