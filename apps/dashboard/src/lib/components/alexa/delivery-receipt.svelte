<script lang="ts">
	import { prefersReducedMotion } from 'svelte/motion';
	import type { TransitionConfig } from 'svelte/transition';

	type Receipt = { device: string; chunks: number; text: string; at: number };

	let { receipt }: { receipt: Receipt } = $props();

	const time = $derived(
		new Intl.DateTimeFormat('pt-BR', { hour: '2-digit', minute: '2-digit', second: '2-digit' }).format(receipt.at)
	);

	// The slip tears down from under the form; reduced motion keeps only the fade.
	function tear(_node: Element): TransitionConfig {
		const still = prefersReducedMotion.current;
		return {
			duration: still ? 160 : 280,
			easing: (t) => 1 - Math.pow(1 - t, 4),
			css: (t, u) =>
				still ? `opacity: ${t}` : `opacity: ${Math.min(1, t * 1.6)}; clip-path: inset(0 0 ${u * 100}% 0)`
		};
	}

	function stamp(_node: Element): TransitionConfig {
		const still = prefersReducedMotion.current;
		return {
			delay: still ? 0 : 180,
			duration: still ? 160 : 220,
			easing: (t) => 1 - Math.pow(1 - t, 3),
			css: (t, u) => (still ? `opacity: ${t}` : `opacity: ${t}; transform: rotate(${-7 - 5 * u}deg) scale(${1 + 0.18 * u})`)
		};
	}
</script>

<section
	in:tear
	aria-labelledby="receipt-title"
	class="perforated relative pt-5"
>
	<div class="relative grid gap-4 rounded-b-md border border-t-0 border-rule bg-paper-sunk px-5 pt-4 pb-5">
		<div
			in:stamp
			aria-hidden="true"
			class="pointer-events-none absolute top-3 right-4 -rotate-7 rounded-[4px] border-2 border-stamp px-2.5 py-1 text-center text-stamp outline-[1.5px] outline-offset-2 outline-stamp [font-stretch:80%]"
		>
			<span class="block text-[1.125rem] leading-none font-extrabold tracking-[0.12em] uppercase">Aceito</span>
			<span class="tabular mt-1 block font-mono text-[0.625rem] leading-none">{time}</span>
		</div>

		<h2 id="receipt-title" class="pr-28 text-[0.9375rem] font-semibold text-ink">
			Aceito pela Alexa — a reprodução não é confirmada.
		</h2>

		<dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
			<dt class="field-label self-center">Echo</dt>
			<dd class="text-ink">{receipt.device}</dd>
			<dt class="field-label self-center">Partes</dt>
			<dd class="tabular text-ink">{receipt.chunks}</dd>
			<dt class="field-label self-center">Reprodução</dt>
			<dd class="text-ink-soft">Não confirmada pela Amazon</dd>
		</dl>

		<blockquote class="line-clamp-3 border-l border-rule-strong pl-3 text-sm text-ink-soft">
			{receipt.text}
		</blockquote>
	</div>
</section>
