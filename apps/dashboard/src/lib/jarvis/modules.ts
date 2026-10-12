import type { Component } from 'svelte';
import CalendarBlank from 'phosphor-svelte/lib/CalendarBlank';
import ListChecks from 'phosphor-svelte/lib/ListChecks';
import NavigationArrow from 'phosphor-svelte/lib/NavigationArrow';
import SpeakerHigh from 'phosphor-svelte/lib/SpeakerHigh';

/** A Jarvis module: one entry in the shell's navigation, one route. Add new modules here. */
export type JarvisModule = {
	readonly id: string;
	readonly label: string;
	readonly href: string;
	readonly icon: Component<{ class?: string; weight?: 'regular' | 'bold' | 'fill' | 'duotone' }>;
};

export const modules: ReadonlyArray<JarvisModule> = [
	{ id: 'alexa', label: 'Alexa', href: '/', icon: SpeakerHigh },
	{ id: 'lembretes', label: 'Lembretes', href: '/lembretes', icon: ListChecks },
	{ id: 'agenda', label: 'Agenda', href: '/agenda', icon: CalendarBlank },
	{ id: 'ir', label: 'Ir & ligar', href: '/ir', icon: NavigationArrow }
];
