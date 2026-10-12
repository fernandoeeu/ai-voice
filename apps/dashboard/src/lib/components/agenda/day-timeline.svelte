<script lang="ts">
	import MapPin from 'phosphor-svelte/lib/MapPin';
	import {
		formatHour,
		formatTime,
		layoutDay,
		minutesOfDay,
		toneBar,
		toneOf,
		type Occurrence
	} from './agenda-time.js';

	let { items, dayKey, now }: { items: ReadonlyArray<Occurrence>; dayKey: string; now: number | null } =
		$props();

	const DAY = 24 * 60;

	const bounds = $derived.by(() => {
		const placedStart = Math.min(...items.map((i) => Math.floor(minutesOfDay(i.start) / 60)), 8);
		const latest = Math.max(
			...items.map((i) => (i.end - i.start >= DAY * 60000 ? 24 : Math.ceil(clampEnd(i) / 60))),
			19
		);
		return { first: Math.max(0, placedStart), last: Math.min(24, latest) };
	});

	// An event that runs past midnight is cut at the end of this day.
	function clampEnd(item: Occurrence) {
		const end = minutesOfDay(item.end);
		return end === 0 || item.end - item.start > DAY * 60000 ? DAY : Math.max(end, minutesOfDay(item.start) + 30);
	}

	const windowStart = $derived(bounds.first * 60);
	const hours = $derived(Array.from({ length: bounds.last - bounds.first + 1 }, (_, i) => bounds.first + i));
	const span = $derived(bounds.last - bounds.first);
	const placed = $derived(layoutDay(items, dayKey, windowStart));

	const nowMinutes = $derived(now === null ? null : minutesOfDay(now));
	const nowVisible = $derived(
		nowMinutes !== null && nowMinutes >= windowStart && nowMinutes <= bounds.last * 60
	);
</script>

<div
	class="relative grid grid-cols-[3.25rem_1fr] gap-x-3 [--hour:3.5rem] sm:grid-cols-[3.75rem_1fr] sm:gap-x-4"
	style="height: calc(var(--hour) * {span} + 1.25rem)"
>
	<div class="relative" aria-hidden="true">
		{#each hours as hour (hour)}
			<span
				class="tabular absolute right-0 -translate-y-1/2 text-xs text-ink-soft"
				style="top: calc(var(--hour) * {hour - bounds.first} + 0.625rem)">{formatHour(hour)}</span
			>
		{/each}
	</div>

	<div class="relative mt-2.5">
		{#each hours as hour (hour)}
			<div
				class="absolute inset-x-0 border-t border-rule"
				style="top: calc(var(--hour) * {hour - bounds.first})"
				aria-hidden="true"
			></div>
		{/each}

		<ul class="absolute inset-0">
			{#each placed as { item, top, height, lane, lanes } (item.event.id + item.event.start)}
				{@const past = now !== null && item.end < now}
				{@const compact = height < 45}
				<li
					class={[
						'absolute overflow-hidden rounded-md border border-rule-strong bg-paper-sunk transition-colors duration-150 hover:bg-accent',
						past && 'opacity-60'
					]}
					style="top: calc(var(--hour) * {top / 60} + 1px); height: calc(var(--hour) * {height / 60} - 3px); left: calc({lane} * 100% / {lanes}); width: calc(100% / {lanes} - 4px)"
					title="{item.event.title}, {formatTime(item.start)}–{formatTime(item.end)}"
				>
					<span class={['absolute inset-y-0 left-0 w-[3px]', toneBar[toneOf(item.event.calendar)]]} aria-hidden="true"
					></span>
					<div class={['h-full pl-3.5 pr-2', compact ? 'flex items-center gap-2' : 'py-1.5']}>
						<p class="tabular shrink-0 text-xs text-ink-soft">
							{formatTime(item.start)}{compact ? '' : `–${formatTime(item.end)}`}
						</p>
						<p class={['truncate text-sm font-semibold text-ink', !compact && 'mt-0.5']}>{item.event.title}</p>
						{#if !compact}
							<p class="mt-0.5 flex min-w-0 items-center gap-1 text-xs text-ink-soft">
								<span class="truncate">{item.event.calendar}</span>
								{#if item.event.location}
									<MapPin class="size-3 shrink-0" weight="bold" aria-hidden="true" />
									<span class="truncate">{item.event.location}</span>
								{/if}
							</p>
						{/if}
					</div>
				</li>
			{/each}
		</ul>

		{#if nowVisible && nowMinutes !== null}
			<div
				class="pointer-events-none absolute inset-x-0 z-10 flex items-center"
				style="top: calc(var(--hour) * {(nowMinutes - windowStart) / 60})"
			>
				<span
					class="tabular -ml-[4.25rem] mr-2 w-[3.25rem] shrink-0 rounded-sm bg-carbon px-1 py-px text-center text-[0.6875rem] font-semibold text-paper sm:-ml-[4.75rem] sm:w-[3.75rem]"
					>{formatTime(now!)}</span
				>
				<span class="h-px flex-1 bg-carbon"></span>
				<span class="sr-only">Agora, {formatTime(now!)}</span>
			</div>
		{/if}
	</div>
</div>
