<script lang="ts">
	import type { Echo } from '@jarvis/alexa';
	import { RadioGroup } from 'bits-ui';
	import WifiHigh from 'phosphor-svelte/lib/WifiHigh';
	import WifiSlash from 'phosphor-svelte/lib/WifiSlash';

	let {
		devices,
		value = $bindable(''),
		name = 'device'
	}: { devices: ReadonlyArray<Echo>; value?: string; name?: string } = $props();
</script>

<fieldset class="min-w-0">
	<legend class="field-label mb-2.5">Para</legend>

	{#if devices.length === 0}
		<p class="rounded-md border border-dashed border-rule-strong px-4 py-6 text-sm text-ink-soft">
			Nenhum Echo nesta conta da Amazon. Configure um no app Alexa e atualize esta página.
		</p>
	{:else}
		<RadioGroup.Root bind:value {name} required class="divide-y divide-rule rounded-md border border-rule bg-paper">
			{#each devices as device (device.serial)}
				<RadioGroup.Item
					value={device.serial}
					disabled={!device.online}
					class="group/echo relative grid w-full grid-cols-[auto_1fr_auto] items-center gap-x-3 px-3.5 py-3 text-left transition-colors duration-150 first:rounded-t-md last:rounded-b-md hover:bg-accent/60 focus-visible:z-10 data-[state=checked]:bg-accent disabled:cursor-not-allowed disabled:hover:bg-transparent"
				>
					{#snippet children({ checked })}
						<span
							aria-hidden="true"
							class={[
								'grid size-[1.125rem] place-items-center rounded-full border-[1.5px] transition-colors duration-150',
								checked ? 'border-ink' : 'border-rule-strong group-disabled/echo:border-dashed'
							]}
						>
							<span
								class={[
									'size-2 rounded-full bg-ink transition-[scale,opacity] duration-200 ease-[var(--ease-out)]',
									checked ? 'scale-100 opacity-100' : 'scale-50 opacity-0'
								]}
							></span>
						</span>

						<span class="min-w-0">
							<span class="flex items-center gap-2">
								<span class={['truncate font-semibold', device.online ? 'text-ink' : 'text-ink-soft']}>
									{device.name}
								</span>
								{#if device.isDefault}
									<span
										class="shrink-0 -rotate-2 rounded-[3px] border-[1.5px] border-stamp px-1.5 py-px text-[0.625rem] leading-tight font-bold tracking-[0.08em] text-stamp uppercase [font-stretch:85%]"
									>
										Padrão
									</span>
								{/if}
							</span>
							<span class="tabular mt-0.5 block truncate font-mono text-[0.75rem] text-ink-soft">{device.serial}</span>
						</span>

						<span
							class={[
								'flex items-center gap-1.5 text-xs font-medium',
								device.online ? 'text-signal' : 'text-ink-soft'
							]}
						>
							{#if device.online}
								<WifiHigh class="size-4" weight="bold" aria-hidden="true" /> Online
							{:else}
								<WifiSlash class="size-4" weight="bold" aria-hidden="true" /> Offline
							{/if}
						</span>
					{/snippet}
				</RadioGroup.Item>
			{/each}
		</RadioGroup.Root>
	{/if}
</fieldset>
