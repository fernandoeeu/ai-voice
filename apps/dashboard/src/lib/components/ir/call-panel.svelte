<script lang="ts">
	import { untrack } from 'svelte';
	import { enhance, type SubmitFunction } from '$app/forms';
	import CircleNotch from 'phosphor-svelte/lib/CircleNotch';
	import Phone from 'phosphor-svelte/lib/Phone';
	import PhoneCall from 'phosphor-svelte/lib/PhoneCall';
	import CallConfirm from './call-confirm.svelte';
	import AppleFailure from './apple-failure.svelte';
	import { checkPhone, type CallVia, callViaLabels } from './phone.js';
	import type { IrResult } from './results.js';

	let { result }: { result: IrResult } = $props();

	const mine = $derived(result?.panel === 'phone' ? result : undefined);

	// Seeded once from the last submission (no-JS round trip); the user owns the fields afterwards.
	const seed = untrack(() => (result?.panel === 'phone' ? result : undefined));
	let number = $state(seed?.number ?? '');
	let via = $state<CallVia>(seed?.via ?? 'phone');
	let confirming = $state(seed?.review === true);
	let pending = $state(false);

	const check = $derived(checkPhone(number));
	const touched = $derived(number.trim() !== '');
	const vias = Object.entries(callViaLabels) as [CallVia, string][];

	// Any edit invalidates a confirmation that was given for the previous number.
	let last = untrack(() => `${number}|${via}`);
	$effect(() => {
		const now = `${number}|${via}`;
		if (now === last) return;
		last = now;
		confirming = false;
	});

	const submit: SubmitFunction = ({ action, cancel }) => {
		if (action.search === '?/review') {
			cancel();
			if (check.ok) confirming = true;
			return;
		}
		pending = true;
		return async ({ update }) => {
			await update({ reset: false, refreshAll: false });
			confirming = false;
			pending = false;
		};
	};

	const time = (at: number) =>
		new Intl.DateTimeFormat('pt-BR', { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(at);

	const input =
		'h-11 w-full rounded-md border border-rule-strong bg-paper px-3 font-mono text-[1.0625rem] text-ink outline-none transition-[border-color,box-shadow] duration-150 placeholder:text-ink-soft/80 focus-visible:border-carbon focus-visible:ring-2 focus-visible:ring-carbon/25 read-only:bg-paper-sunk aria-invalid:border-alert';
</script>

<section aria-labelledby="call-title" class="grid min-w-0 content-start gap-5">
	<div class="flex items-center gap-2.5 border-b border-rule pb-3">
		<Phone class="size-5 text-carbon" weight="bold" aria-hidden="true" />
		<h2 id="call-title" class="display text-xl text-ink">Ligar</h2>
		<span class="ml-auto text-xs text-ink-soft">sai do seu iPhone ou FaceTime</span>
	</div>

	<form method="POST" action="?/call" use:enhance={submit} class="grid gap-5">
		<div class="grid gap-1.5">
			<label for="call-number" class="field-label">Número</label>
			<input
				id="call-number"
				name="number"
				type="tel"
				inputmode="tel"
				autocomplete="off"
				bind:value={number}
				readonly={confirming}
				required
				placeholder="11 99999-8888"
				aria-invalid={mine?.invalid || (touched && !check.ok && number.length > 3) ? 'true' : undefined}
				aria-describedby="call-hint"
				class={input}
			/>
			<p id="call-hint" class="min-h-5 text-sm" aria-live="polite">
				{#if !touched}
					<span class="text-ink-soft">DDD e número. Sem código de país, assumimos +55 (Brasil).</span>
				{:else if check.ok}
					<span class="text-ink-soft">Será discado como <span class="tabular font-mono font-semibold text-signal">{check.display}</span></span>
				{:else}
					<span class="text-ink-soft"
						><span class="tabular font-mono text-ink">{check.display}</span> · {check.hint}</span
					>
				{/if}
			</p>
		</div>

		<fieldset class="grid gap-1.5" disabled={confirming}>
			<legend class="field-label mb-1.5">Ligar por</legend>
			<div class="grid grid-cols-2 gap-1 rounded-md border border-rule-strong bg-paper-sunk p-1">
				{#each vias as [value, label] (value)}
					<label
						class="cursor-pointer rounded-[5px] px-3 py-1.5 text-center text-sm font-semibold text-ink-soft transition-[background-color,color] duration-150 ease-out has-checked:bg-paper has-checked:text-ink has-checked:shadow-[0_1px_0_var(--rule-strong)] has-focus-visible:outline-2 has-focus-visible:outline-offset-1 has-focus-visible:outline-carbon"
					>
						<input type="radio" name="via" {value} bind:group={via} class="sr-only" />
						{label}
					</label>
				{/each}
			</div>
		</fieldset>

		{#if mine?.invalid}
			<p class="text-sm font-medium text-alert">{mine.invalid}</p>
		{/if}

		<CallConfirm {confirming} {pending} ready={check.ok} display={check.display} viaLabel={callViaLabels[via]} oncancel={() => (confirming = false)} />
	</form>

	<div aria-live="polite">
		{#if mine?.failure}
			<AppleFailure failure={mine.failure} nothing="Nenhuma ligação foi iniciada." />
		{:else if mine?.placed}
			{#key mine.placed.at}
				<div class="perforated pt-4">
					<div class="grid gap-1 rounded-b-md border border-t-0 border-rule bg-paper-sunk px-4 pt-3 pb-4">
						<p class="flex items-center gap-2 text-sm font-semibold text-ink">
							<PhoneCall class="size-4 text-signal" weight="bold" aria-hidden="true" />
							Número entregue ao macOS às <span class="tabular">{time(mine.placed.at)}</span>.
						</p>
						<p class="text-sm text-ink-soft">
							<span class="tabular font-mono">{mine.placed.display}</span> aguarda a confirmação do macOS. A chamada só começa
							depois que você aceitar lá; não sabemos se foi atendida.
						</p>
					</div>
				</div>
			{/key}
		{:else}
			<p class="perforated pt-4 text-sm text-ink-soft">
				Nada é discado só por este formulário: o macOS ainda pede a sua confirmação antes de ligar.
			</p>
		{/if}
	</div>
</section>
