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

<!-- The browser chrome matches the shell's second neutral layer (--chrome). -->
<ModeWatcher themeColors={{ light: 'oklch(0.963 0.0035 85)', dark: 'oklch(0.153 0.004 70)' }} />

<AppShell>
	{@render children()}
</AppShell>
