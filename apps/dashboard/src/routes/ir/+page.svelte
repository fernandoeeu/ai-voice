<script lang="ts">
	import CallPanel from '#lib/components/ir/call-panel.svelte';
	import MapsPanel from '#lib/components/ir/maps-panel.svelte';
	import ModuleHeader from '#lib/components/jarvis/module-header.svelte';
	import type { PageProps } from './$types';

	let { form }: PageProps = $props();
</script>

<svelte:head>
	<title>Ir & ligar · Jarvis</title>
</svelte:head>

<div class="mx-auto grid max-w-[72rem] gap-8">
	<ModuleHeader title="Ir & ligar">
		{#snippet summary()}
			Abra o Maps do Mac com uma busca ou rota e inicie ligações pelo iPhone ou FaceTime.
		{/snippet}
	</ModuleHeader>

	<div class="grid items-start gap-x-14 gap-y-10 lg:grid-cols-2">
		<MapsPanel result={form} />
		<CallPanel result={form} />
	</div>
</div>
