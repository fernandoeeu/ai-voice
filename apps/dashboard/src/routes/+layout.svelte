<script lang="ts">
	import './layout.css';
	import { ModeWatcher } from 'mode-watcher';
	import favicon from '#lib/assets/favicon.svg';
	import AppShell from '#lib/components/jarvis/app-shell.svelte';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<ModeWatcher themeColors={{ light: 'oklch(0.948 0.012 252)', dark: 'oklch(0.172 0.036 272)' }} />

<AppShell>
	{@render children()}
</AppShell>
