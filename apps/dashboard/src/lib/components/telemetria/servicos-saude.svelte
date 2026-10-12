<script lang="ts">
	import { bytes, pct, type Servico } from '#lib/telemetria/model.js';
	import { count, duration } from '#lib/conversa/format.js';

	/** One card per dev.jarvis.* LaunchAgent: alive, pid, restarts, memory and CPU. */
	let { servicos, fonte }: { servicos: ReadonlyArray<Servico>; fonte: 'ao-vivo' | 'prometheus' } = $props();

	const tom = (s: Servico) => (!s.vivo ? 'alert' : s.httpOk === false ? 'caution' : 'positive');
	const rotulo = (s: Servico) => (s.carregado === false ? 'Descarregado' : !s.vivo ? 'Parado' : s.httpOk === false ? 'Sem resposta' : 'No ar');
	const ordenados = $derived([...servicos].sort((a, b) => Number(a.vivo) - Number(b.vivo) || a.nome.localeCompare(b.nome)));
</script>

<section aria-labelledby="servicos-title" class="grid gap-3">
	<div class="flex items-baseline justify-between gap-4">
		<h2 id="servicos-title" class="text-heading font-semibold text-ink">Serviços</h2>
		{#if fonte === 'prometheus' && servicos.length > 0}
			<p class="meta">última leitura da sonda, via Prometheus</p>
		{/if}
	</div>
	{#if servicos.length === 0}
		<p class="meta">O coletor ainda não informou nenhum serviço dev.jarvis.*.</p>
	{:else}
		<ul class="grid grid-cols-[repeat(auto-fill,minmax(13.5rem,1fr))] gap-3">
			{#each ordenados as s (s.nome)}
				{@const t = tom(s)}
				<li class="sheet grid gap-3 px-4 py-3.5">
					<div class="flex items-start justify-between gap-3">
						<div class="min-w-0">
							<p class="truncate text-body font-semibold text-ink capitalize">{s.nome}</p>
							<p class="truncate font-mono text-micro text-ink-faint">{s.label}</p>
						</div>
						<span
							class={[
								'inline-flex shrink-0 items-center gap-1.5 text-micro font-semibold',
								t === 'positive' && 'text-positive',
								t === 'caution' && 'text-caution',
								t === 'alert' && 'text-alert'
							]}
						>
							<span class="status-dot" aria-hidden="true"></span>
							{rotulo(s)}
						</span>
					</div>
					<dl class="grid grid-cols-2 gap-x-4 gap-y-1.5 text-meta">
						<div class="flex justify-between gap-2">
							<dt class="text-ink-soft">RAM</dt>
							<dd class="tabular font-medium text-ink">{bytes(s.ramBytes)}</dd>
						</div>
						<div class="flex justify-between gap-2">
							<dt class="text-ink-soft">CPU</dt>
							<dd class="tabular font-medium text-ink">{pct(s.cpuPct)}</dd>
						</div>
						<div class="flex justify-between gap-2">
							<dt class="text-ink-soft">PID</dt>
							<dd class="tabular font-medium text-ink">{s.pid ?? '—'}</dd>
						</div>
						<div class="flex justify-between gap-2">
							<dt class="text-ink-soft">Reinícios</dt>
							<dd class={['tabular font-medium', (s.reinicios ?? 0) > 3 ? 'text-caution' : 'text-ink']}>
								{s.reinicios === null ? '—' : count(s.reinicios)}
							</dd>
						</div>
						{#if s.healthMs !== null}
							<div class="col-span-2 flex justify-between gap-2">
								<dt class="text-ink-soft">Health</dt>
								<dd class="tabular font-medium text-ink">{duration(s.healthMs)}</dd>
							</div>
						{/if}
					</dl>
				</li>
			{/each}
		</ul>
	{/if}
</section>
