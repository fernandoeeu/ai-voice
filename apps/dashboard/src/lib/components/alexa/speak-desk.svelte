<script lang="ts">
	import type { Echo } from '@jarvis/alexa';
	import { untrack } from 'svelte';
	import { enhance, type SubmitFunction } from '$app/forms';
	import CircleNotch from 'phosphor-svelte/lib/CircleNotch';
	import PaperPlaneTilt from 'phosphor-svelte/lib/PaperPlaneTilt';
	import type { AlexaFailure } from '#lib/alexa/failure.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import Notice from '#lib/components/jarvis/notice.svelte';
	import DeliveryFailure from './delivery-failure.svelte';
	import DeliveryReceipt from './delivery-receipt.svelte';
	import DeviceLedger from './device-ledger.svelte';

	type Result =
		| { text: string; device: string; invalid: string }
		| { text: string; device: string; failure: AlexaFailure }
		| { accepted: { device: string; chunks: number; text: string; at: number } }
		| null
		| undefined;

	let {
		devices,
		speechEnabled,
		maxLength,
		result
	}: { devices: ReadonlyArray<Echo>; speechEnabled: boolean; maxLength: number; result: Result } = $props();

	const preferred = () =>
		devices.find((d) => d.isDefault && d.online)?.serial ?? devices.find((d) => d.online)?.serial ?? '';

	// Seeded once from the last submission (no-JS round trip) or the default Echo; the user owns it after that.
	const seed = untrack(() => result);
	let device = $state(seed && 'device' in seed && seed.device ? seed.device : untrack(preferred));
	let text = $state(seed && 'text' in seed ? seed.text : '');
	let pending = $state(false);
	let formElement = $state<HTMLFormElement>();

	const target = $derived(devices.find((d) => d.serial === device));
	const length = $derived(text.length);
	const over = $derived(length > maxLength);
	const blank = $derived(text.trim() === '');
	const anyOnline = $derived(devices.some((d) => d.online));
	const ready = $derived(speechEnabled && !!target?.online && !blank && !over && !pending);

	const invalid = $derived(result && 'invalid' in result ? result.invalid : undefined);
	const failure = $derived(result && 'failure' in result ? result.failure : undefined);
	const accepted = $derived(result && 'accepted' in result ? result.accepted : undefined);

	const submit: SubmitFunction = () => {
		pending = true;
		return async ({ result, update }) => {
			// The device list does not change because something was spoken: skip the extra Amazon round trip.
			await update({ reset: false, refreshAll: false });
			if (result.type === 'success') text = '';
			pending = false;
		};
	};

	function onkeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && ready) {
			event.preventDefault();
			formElement?.requestSubmit();
		}
	}
</script>

<form
	bind:this={formElement}
	method="POST"
	action="?/speak"
	use:enhance={submit}
	class="grid items-start gap-x-10 gap-y-8 lg:grid-cols-[minmax(16rem,22rem)_minmax(0,1fr)] xl:gap-x-14"
>
	<DeviceLedger {devices} bind:value={device} />

	<div class="grid min-w-0 gap-5">
		<div class="rounded-md border border-rule bg-paper shadow-[0_1px_0_var(--rule),0_12px_32px_-24px_color-mix(in_oklch,var(--ink)_40%,transparent)]">
			<div class="flex items-baseline justify-between gap-4 border-b border-rule px-5 pt-4 pb-3">
				<label for="speak-text" class="field-label">Mensagem</label>
				<span class="truncate text-sm text-ink-soft">
					{#if target}
						para <span class="font-semibold text-ink">{target.name}</span>
					{:else}
						escolha um Echo
					{/if}
				</span>
			</div>

			<textarea
				id="speak-text"
				name="text"
				bind:value={text}
				{onkeydown}
				rows="5"
				required
				maxlength={maxLength + 200}
				aria-invalid={invalid || over ? 'true' : undefined}
				aria-describedby="speak-meta{invalid ? ' speak-invalid' : ''}"
				placeholder="O que a Alexa deve falar?"
				disabled={!speechEnabled}
				class="ruled block min-h-[8.75rem] w-full resize-y bg-transparent px-5 pt-1.5 pb-0 text-[1.0625rem] text-ink outline-none placeholder:text-ink-soft/80 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-60"
			></textarea>

			<div class="flex flex-wrap items-center justify-between gap-3 border-t border-rule px-5 py-3">
				<p id="speak-meta" class="flex items-center gap-3 text-xs text-ink-soft">
					<span class={['tabular font-mono', over && 'font-semibold text-alert']}>
						{length.toLocaleString('pt-BR')} / {maxLength.toLocaleString('pt-BR')}
					</span>
					<span class="hidden sm:inline">
						<kbd class="rounded-sm border border-rule-strong px-1 text-[0.6875rem]">⌘</kbd>
						<kbd class="rounded-sm border border-rule-strong px-1 text-[0.6875rem]">Enter</kbd>
						envia
					</span>
				</p>
				<Button type="submit" size="lg" class="ml-auto" disabled={!ready} aria-busy={pending}>
					{#if pending}
						<CircleNotch class="size-4 animate-spin" weight="bold" aria-hidden="true" />
						Enviando…
					{:else}
						<PaperPlaneTilt class="size-4" weight="bold" aria-hidden="true" />
						Enviar para a Alexa
					{/if}
				</Button>
			</div>
		</div>

		{#if invalid}
			<p id="speak-invalid" class="text-sm font-medium text-alert">{invalid}</p>
		{/if}

		{#if !speechEnabled}
			<Notice tone="caution" title="A fala está desativada neste servidor.">
				Defina <code>AI_VOICE_ENABLE_SPEECH=1</code> e reinicie o dashboard para enviar mensagens.
			</Notice>
		{:else if !anyOnline && devices.length > 0}
			<Notice tone="caution" title="Nenhum Echo está online agora.">
				Ligue um Echo ou confira a rede dele, depois atualize a lista.
			</Notice>
		{/if}

		<div aria-live="polite">
			{#if failure}
				<DeliveryFailure {failure} />
			{:else if accepted}
				{#key accepted.at}
					<DeliveryReceipt receipt={accepted} />
				{/key}
			{:else if speechEnabled}
				<p class="perforated pt-4 text-sm text-ink-soft">
					O comprovante aparece aqui depois do envio. A Amazon confirma que aceitou o pedido, não que o Echo tocou o áudio.
				</p>
			{/if}
		</div>
	</div>
</form>
