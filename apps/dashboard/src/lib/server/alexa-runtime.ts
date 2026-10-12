import { NodeServices } from '@effect/platform-node';
import { Alexa, AlexaConfig } from '@jarvis/alexa';
import { Layer, ManagedRuntime } from 'effect';
import { FetchHttpClient } from 'effect/http';

const AlexaLive = Alexa.layer.pipe(
	Layer.provideMerge(AlexaConfig.layer),
	Layer.provide(Layer.mergeAll(NodeServices.layer, FetchHttpClient.layer))
);

/** The one Effect runtime of the dashboard: holds the authenticated Alexa connection between requests. */
export const alexaRuntime = ManagedRuntime.make(AlexaLive);

// Vite re-evaluates this module on edit; release the old connection instead of leaking it.
import.meta.hot?.dispose(() => alexaRuntime.dispose());
