<script lang="ts">
	import { refreshAll } from '$app/navigation';
	import ArrowClockwise from 'phosphor-svelte/lib/ArrowClockwise';
	import DeliveryFailure from '#lib/components/alexa/delivery-failure.svelte';
	import SessionMissing from '#lib/components/alexa/session-missing.svelte';
	import SpeakDesk from '#lib/components/alexa/speak-desk.svelte';
	import ModuleHeader from '#lib/components/jarvis/module-header.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	let refreshing = $state(false);

	async function refresh() {
		refreshing = true;
		await refreshAll().finally(() => (refreshing = false));
	}

	const devices = $derived(data.devices.ok ? data.devices.value : []);
	const online = $derived(devices.filter((d) => d.online).length);
	const speechEnabled = $derived(data.settings.ok && data.settings.value.speechEnabled);
</script>

<svelte:head>
	<title>Alexa · Jarvis</title>
</svelte:head>

<div class="mx-auto grid max-w-[72rem] gap-8">
	<ModuleHeader title="Alexa">
		{#snippet summary()}
			{#if data.devices.ok}
				<span class="tabular">{devices.length}</span>
				{devices.length === 1 ? 'Echo' : 'Echos'} na conta,
				<span class="tabular">{online}</span>
				{online === 1 ? 'online' : 'online'}.
				{speechEnabled ? 'Fala ativada.' : 'Fala desativada.'}
			{:else}
				Sem acesso aos Echos agora.
			{/if}
		{/snippet}
		{#snippet actions()}
			<Button variant="outline" onclick={refresh} disabled={refreshing}>
				<ArrowClockwise class={['size-4', refreshing && 'animate-spin']} weight="bold" aria-hidden="true" />
				{refreshing ? 'Atualizando…' : 'Atualizar'}
			</Button>
		{/snippet}
	</ModuleHeader>

	<div class={['transition-opacity duration-200', refreshing && 'opacity-60']} aria-busy={refreshing}>
		{#if !data.settings.ok}
			<DeliveryFailure failure={data.settings.failure} />
		{:else if !data.devices.ok && data.devices.failure.kind === 'session'}
			<SessionMissing message={data.devices.failure.message} />
		{:else if !data.devices.ok}
			<DeliveryFailure failure={data.devices.failure} />
		{:else}
			<SpeakDesk {devices} {speechEnabled} maxLength={data.maxLength} result={form} />
		{/if}
	</div>
</div>
