import { alexaRuntime } from '#lib/server/alexa-runtime.js';

// adapter-node emits this once every connection has closed, so in-flight requests finish first.
process.once('sveltekit:shutdown', () => alexaRuntime.dispose());
