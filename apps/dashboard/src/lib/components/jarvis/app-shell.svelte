<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import { modules } from '#lib/jarvis/modules.js';
	import ThemeToggle from './theme-toggle.svelte';

	let { children }: { children: Snippet } = $props();

	const isCurrent = (href: string) =>
		href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
</script>

<div class="grid min-h-dvh grid-rows-[auto_1fr] md:grid-cols-[13.5rem_1fr] md:grid-rows-1">
	<header
		class="sticky top-0 z-20 flex items-center gap-3 border-b bg-paper-sunk/95 px-4 py-2.5 backdrop-blur-sm md:static md:h-dvh md:flex-col md:items-stretch md:gap-8 md:border-r md:border-b-0 md:bg-paper-sunk md:px-4 md:py-6 md:backdrop-blur-none md:sticky"
	>
		<a href="/" class="flex items-baseline gap-2 rounded-sm md:px-2">
			<span class="display text-[1.375rem] leading-none text-ink">Jarvis</span>
		</a>

		<nav aria-label="Módulos" class="flex-1">
			<ul class="flex gap-1 md:flex-col">
				{#each modules as module (module.id)}
					{@const current = isCurrent(module.href)}
					<li>
						<a
							href={module.href}
							aria-current={current ? 'page' : undefined}
							class={[
								'flex items-center gap-2.5 rounded-md px-2.5 py-2 text-sm font-medium transition-colors duration-150',
								current ? 'bg-paper text-ink shadow-[inset_0_0_0_1px_var(--rule)]' : 'text-ink-soft hover:bg-accent hover:text-ink'
							]}
						>
							<module.icon class="size-[1.125rem]" weight={current ? 'fill' : 'regular'} />
							{module.label}
						</a>
					</li>
				{/each}
			</ul>
		</nav>

		<div class="md:px-0.5">
			<ThemeToggle />
		</div>
	</header>

	<main class="min-w-0 px-4 pt-6 pb-16 sm:px-6 md:px-10 md:pt-10">
		{@render children()}
	</main>
</div>
