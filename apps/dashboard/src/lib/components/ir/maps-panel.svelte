<script lang="ts">
	import { untrack } from 'svelte';
	import { enhance, type SubmitFunction } from '$app/forms';
	import ArrowRight from 'phosphor-svelte/lib/ArrowRight';
	import CircleNotch from 'phosphor-svelte/lib/CircleNotch';
	import MapPin from 'phosphor-svelte/lib/MapPin';
	import ArrowSquareOut from 'phosphor-svelte/lib/ArrowSquareOut';
	import Check from 'phosphor-svelte/lib/Check';
	import Copy from 'phosphor-svelte/lib/Copy';
	import { Button } from '#lib/components/ui/button/index.js';
	import AppleFailure from './apple-failure.svelte';
	import type { IrResult } from './results.js';

	let { result }: { result: IrResult } = $props();

	const mine = $derived(result?.panel === 'maps' ? result : undefined);

	// Seeded once from the last submission (no-JS round trip); the user owns the fields afterwards.
	const seed = untrack(() => (result?.panel === 'maps' ? result : undefined));
	let kind = $state<'search' | 'directions'>(seed?.kind ?? 'search');
	let query = $state(seed?.query ?? '');
	let to = $state(seed?.to ?? '');
	let from = $state(seed?.from ?? '');
	let mode = $state<'driving' | 'walking' | 'transit'>(seed?.mode ?? 'driving');
	let pending = $state(false);
	let copied = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;

	const modes = [
		{ value: 'driving', label: 'Carro' },
		{ value: 'walking', label: 'A pé' },
		{ value: 'transit', label: 'Transporte' }
	] as const;

	const ready = $derived((kind === 'search' ? query.trim() : to.trim()) !== '' && !pending);

	const submit: SubmitFunction = () => {
		pending = true;
		return async ({ update }) => {
			await update({ reset: false, refreshAll: false });
			pending = false;
		};
	};

	async function copy(url: string) {
		await navigator.clipboard.writeText(url);
		copied = true;
		clearTimeout(timer);
		timer = setTimeout(() => (copied = false), 1600);
	}

	const time = (at: number) =>
		new Intl.DateTimeFormat('pt-BR', { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(at);

	const input =
		'h-10 w-full rounded-md border border-rule-strong bg-paper px-3 text-[0.9375rem] text-ink outline-none transition-[border-color,box-shadow] duration-150 placeholder:text-ink-soft/80 focus-visible:border-carbon focus-visible:ring-2 focus-visible:ring-carbon/25 aria-invalid:border-alert';
</script>

<section aria-labelledby="maps-title" class="grid min-w-0 content-start gap-5">
	<div class="flex items-center gap-2.5 border-b border-rule pb-3">
		<MapPin class="size-5 text-carbon" weight="bold" aria-hidden="true" />
		<h2 id="maps-title" class="display text-xl text-ink">Maps</h2>
		<span class="ml-auto text-xs text-ink-soft">abre no Mac</span>
	</div>

	<form method="POST" action="?/{kind}" use:enhance={submit} class="grid gap-5">
		<fieldset class="grid grid-cols-2 gap-1 rounded-md border border-rule-strong bg-paper-sunk p-1">
			<legend class="sr-only">Tipo de pedido</legend>
			{#each [{ value: 'search', label: 'Buscar lugar' }, { value: 'directions', label: 'Traçar rota' }] as const as option (option.value)}
				<label
					class="cursor-pointer rounded-[5px] px-3 py-1.5 text-center text-sm font-semibold text-ink-soft transition-[background-color,color] duration-150 ease-out has-checked:bg-paper has-checked:text-ink has-checked:shadow-[0_1px_0_var(--rule-strong)] has-focus-visible:outline-2 has-focus-visible:outline-offset-1 has-focus-visible:outline-carbon"
				>
					<input type="radio" name="kind" value={option.value} bind:group={kind} class="sr-only" />
					{option.label}
				</label>
			{/each}
		</fieldset>

		{#if kind === 'search'}
			<div class="grid gap-1.5">
				<label for="maps-query" class="field-label">Lugar ou endereço</label>
				<input
					id="maps-query"
					name="query"
					bind:value={query}
					required
					maxlength="500"
					autocomplete="off"
					placeholder="Padaria perto do Ibirapuera"
					aria-invalid={mine?.invalid ? 'true' : undefined}
					class={input}
				/>
			</div>
		{:else}
			<div class="grid gap-4">
				<div class="grid gap-1.5">
					<label for="maps-from" class="field-label">Partida <span class="font-normal normal-case">(opcional)</span></label>
					<input
						id="maps-from"
						name="from"
						bind:value={from}
						maxlength="500"
						autocomplete="off"
						placeholder="Deixe vazio para usar o local atual"
						class={input}
					/>
				</div>
				<div class="grid gap-1.5">
					<label for="maps-to" class="field-label">Destino</label>
					<input
						id="maps-to"
						name="to"
						bind:value={to}
						required
						maxlength="500"
						autocomplete="off"
						placeholder="Aeroporto de Congonhas"
						aria-invalid={mine?.invalid ? 'true' : undefined}
						class={input}
					/>
				</div>
				<fieldset class="grid gap-1.5">
					<legend class="field-label mb-1.5">Como ir</legend>
					<div class="grid grid-cols-3 gap-1 rounded-md border border-rule-strong bg-paper-sunk p-1">
						{#each modes as option (option.value)}
							<label
								class="cursor-pointer rounded-[5px] px-2 py-1.5 text-center text-sm font-semibold text-ink-soft transition-[background-color,color] duration-150 ease-out has-checked:bg-paper has-checked:text-ink has-checked:shadow-[0_1px_0_var(--rule-strong)] has-focus-visible:outline-2 has-focus-visible:outline-offset-1 has-focus-visible:outline-carbon"
							>
								<input type="radio" name="mode" value={option.value} bind:group={mode} class="sr-only" />
								{option.label}
							</label>
						{/each}
					</div>
				</fieldset>
			</div>
		{/if}

		{#if mine?.invalid}
			<p class="text-sm font-medium text-alert">{mine.invalid}</p>
		{/if}

		<div class="flex justify-end">
			<Button type="submit" size="lg" disabled={!ready} aria-busy={pending}>
				{#if pending}
					<CircleNotch class="size-4 animate-spin" weight="bold" aria-hidden="true" />
					Abrindo o Maps…
				{:else}
					{kind === 'search' ? 'Buscar no Maps' : 'Traçar rota no Maps'}
					<ArrowRight class="size-4" weight="bold" aria-hidden="true" />
				{/if}
			</Button>
		</div>
	</form>

	<div aria-live="polite">
		{#if mine?.failure}
			<AppleFailure failure={mine.failure} nothing="O Maps não foi aberto." />
		{:else if mine?.opened}
			{#key mine.opened.at}
				<div class="perforated pt-4">
					<div class="grid gap-2 rounded-b-md border border-t-0 border-rule bg-paper-sunk px-4 pt-3 pb-4">
						<p class="text-sm font-semibold text-ink">
							Pedido aberto no Maps do Mac às <span class="tabular">{time(mine.opened.at)}</span>.
						</p>
						<div class="flex items-stretch overflow-hidden rounded-md border border-rule-strong bg-paper">
							<a
								href={mine.opened.url}
								target="_blank"
								rel="noreferrer"
								class="flex min-w-0 flex-1 items-center gap-2 px-3 py-2.5 font-mono text-[0.8125rem] text-carbon underline-offset-4 hover:underline"
							>
								<ArrowSquareOut class="size-4 shrink-0" weight="bold" aria-hidden="true" />
								<span class="truncate">{mine.opened.url}</span>
							</a>
							<button
								type="button"
								onclick={() => copy(mine.opened!.url)}
								class="flex items-center gap-1.5 border-l border-rule-strong px-3 text-xs font-semibold text-carbon transition-[background-color,scale] duration-150 ease-out hover:bg-accent active:scale-[0.97]"
							>
								{#if copied}
									<Check class="size-4" weight="bold" aria-hidden="true" /> Copiado
								{:else}
									<Copy class="size-4" weight="bold" aria-hidden="true" /> Copiar
								{/if}
							</button>
						</div>
					</div>
				</div>
			{/key}
		{:else}
			<p class="perforated pt-4 text-sm text-ink-soft">
				O link do maps.apple.com aparece aqui depois do pedido, para você abrir em outro aparelho.
			</p>
		{/if}
	</div>
</section>
