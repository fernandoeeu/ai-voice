<script lang="ts">
	import CircleNotch from 'phosphor-svelte/lib/CircleNotch';
	import PhoneCall from 'phosphor-svelte/lib/PhoneCall';
	import PhoneIcon from 'phosphor-svelte/lib/Phone';
	import { Button } from '#lib/components/ui/button/index.js';

	let {
		confirming,
		pending,
		ready,
		display,
		viaLabel,
		oncancel
	}: {
		confirming: boolean;
		pending: boolean;
		ready: boolean;
		display: string;
		viaLabel: string;
		oncancel: () => void;
	} = $props();
</script>

{#if confirming}
	<div role="group" aria-labelledby="call-confirm-title" class="grid gap-4 rounded-md border border-stamp/50 bg-stamp/6 px-4 py-4">
		<div class="grid gap-1">
			<p id="call-confirm-title" class="text-base font-semibold text-ink">
				Ligar para <span class="tabular font-mono">{display}</span> pelo {viaLabel}?
			</p>
			<p class="text-sm text-ink-soft">
				Isso inicia uma chamada de verdade. O macOS vai pedir confirmação mais uma vez antes de discar.
			</p>
		</div>
		<input type="hidden" name="confirmed" value="yes" />
		<div class="flex flex-wrap justify-end gap-2">
			<Button type="button" variant="outline" size="lg" onclick={oncancel} disabled={pending}>Voltar e editar</Button>
			<Button type="submit" size="lg" disabled={pending} aria-busy={pending}>
				{#if pending}
					<CircleNotch class="size-4 animate-spin" weight="bold" aria-hidden="true" />
					Enviando ao macOS…
				{:else}
					<PhoneCall class="size-4" weight="bold" aria-hidden="true" />
					Sim, ligar
				{/if}
			</Button>
		</div>
	</div>
{:else}
	<div class="flex justify-end">
		<!-- A plain submit button on purpose: without JS this posts ?/review, which shows the confirm step. -->
		<Button type="submit" formaction="?/review" size="lg" disabled={!ready}>
			<PhoneIcon class="size-4" weight="bold" aria-hidden="true" />
			Revisar chamada
		</Button>
	</div>
{/if}
