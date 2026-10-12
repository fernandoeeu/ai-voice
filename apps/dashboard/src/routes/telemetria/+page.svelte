<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import ArrowSquareOut from 'phosphor-svelte/lib/ArrowSquareOut';
	import CopyCommand from '#lib/components/jarvis/copy-command.svelte';
	import ModuleHeader from '#lib/components/jarvis/module-header.svelte';
	import Notice from '#lib/components/jarvis/notice.svelte';
	import HerdrAgentes from '#lib/components/telemetria/herdr-agentes.svelte';
	import Interrupcoes from '#lib/components/telemetria/interrupcoes.svelte';
	import Latencias from '#lib/components/telemetria/latencias.svelte';
	import ServicosSaude from '#lib/components/telemetria/servicos-saude.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { count, duration } from '#lib/conversa/format.js';
	import type { Painel } from '#lib/telemetria/model.js';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// The server renders the first read; after that the page follows the collector over server-sent events.
	let painel = $state<Painel>(untrack(() => data.painel));
	let connection = $state<'conectando' | 'ao-vivo' | 'reconectando'>('conectando');
	let now = $state(untrack(() => data.painel.geradoEm));

	onMount(() => {
		now = Date.now();
		const tick = setInterval(() => (now = Date.now()), 1000);
		const source = new EventSource('/telemetria/ao-vivo');
		source.onopen = () => (connection = 'ao-vivo');
		source.onmessage = (event) => {
			try {
				painel = JSON.parse(event.data) as Painel;
				connection = 'ao-vivo';
			} catch {
				// A broken frame: the next one replaces it.
			}
		};
		source.onerror = () => (connection = 'reconectando');
		return () => {
			clearInterval(tick);
			source.close();
		};
	});

	const estado = $derived(painel.estado);
	const servicos = $derived(estado?.servicos.length ? estado.servicos : painel.servicosProm);
	const vivos = $derived(servicos.filter((s) => s.vivo).length);
	const atencao = $derived((estado?.herdr?.agentes ?? []).filter((a) => a.estado === 'blocked').length);
	const entrega = $derived(painel.latencias.find((l) => l.id === 'escuta-entrega')?.p50 ?? estado?.escuta?.entregaMs?.p50 ?? null);
	const aoVivo = $derived(connection === 'ao-vivo' && painel.coletor === 'ok');
</script>

<svelte:head>
	<title>Telemetria · Jarvis</title>
</svelte:head>

<div class="mx-auto grid max-w-[72rem] grid-cols-[minmax(0,1fr)] gap-8">
	<ModuleHeader title="Telemetria">
		{#snippet summary()}
			{#if !estado && servicos.length === 0}
				Sem leitura do coletor agora.
			{:else}
				<span class="whitespace-nowrap"><span class="tabular">{vivos}</span> de <span class="tabular">{servicos.length}</span> serviços no ar.</span>
				{#if atencao > 0}
					<span class="whitespace-nowrap"><span class="tabular">{count(atencao)}</span> {atencao === 1 ? 'agente esperando' : 'agentes esperando'} você.</span>
				{/if}
				{#if entrega !== null}
					<span class="whitespace-nowrap">Pedidos entregues em <span class="tabular">{duration(entrega)}</span> (p50).</span>
				{/if}
			{/if}
		{/snippet}
		{#snippet actions()}
			<p
				class={[
					'flex shrink-0 items-center gap-2 rounded-full border px-2.5 py-1 text-micro font-semibold',
					aoVivo ? 'border-rule bg-sunken text-ink-soft' : 'border-caution/30 bg-caution-soft text-caution'
				]}
				aria-live="polite"
			>
				<span class={['size-1.5 rounded-full', aoVivo ? 'bg-positive' : 'bg-caution']} aria-hidden="true"></span>
				{#if connection === 'conectando'}Conectando…{:else if connection === 'reconectando'}Reconectando…{:else if painel.coletor === 'fora'}Coletor fora{:else}Ao vivo{/if}
			</p>
			<Button variant="outline" href={painel.painelUrl} target="_blank" rel="noopener noreferrer">
				Abrir painel completo
				<ArrowSquareOut class="size-4" weight="bold" aria-hidden="true" />
			</Button>
		{/snippet}
	</ModuleHeader>

	{#if painel.coletor === 'fora'}
		<Notice tone="info" title="O coletor ao vivo não está respondendo">
			<p>
				Esta aba lê <code>{painel.coletorUrl}/estado</code>. Assim que ele subir, os serviços, os agentes e as interrupções
				aparecem aqui sozinhos, sem recarregar. O histórico continua no painel completo.
			</p>
			<div class="mt-2.5"><CopyCommand command="curl -s {painel.coletorUrl}/estado" /></div>
		</Notice>
	{/if}

	<ServicosSaude {servicos} fonte={estado?.servicos.length ? 'ao-vivo' : 'prometheus'} />

	<div class="grid items-start gap-x-10 gap-y-8 lg:grid-cols-[minmax(0,1fr)_minmax(18rem,22rem)] xl:gap-x-12">
		<div class="grid min-w-0 gap-8">
			<HerdrAgentes herdr={estado?.herdr ?? null} historico={painel.historico} {now} />
		</div>
		<aside class="grid min-w-0 gap-6 lg:sticky lg:top-10" aria-label="Voz e escuta">
			<Latencias latencias={painel.latencias} prometheus={painel.prometheus} entregaColetor={estado?.escuta?.entregaMs ?? null} />
			<Interrupcoes escuta={estado?.escuta ?? null} {now} />
		</aside>
	</div>
</div>
