import { AlexaConfig } from "@jarvis/alexa"
import { Effect, Layer } from "effect"
import { McpProtocol, McpServer } from "effect/ai"
import pkg from "../package.json" with { type: "json" }
import { layer as AppleTools } from "./apple-toolkit.ts"
import { makeVoiceToolkit, voiceHandlers } from "./voice-toolkit.ts"

export const name = "Jarvis Voice"

export const { version } = pkg

// Newest first: an initialize offering an unknown version falls back to the first stateful adapter.
export const protocols = [
  McpProtocol.v2025_11_25,
  McpProtocol.v2025_06_18,
  McpProtocol.v2025_03_26,
  McpProtocol.v2024_11_05
] as const

export const instructions = (locale: string) =>
  `Use speak when the user asks for a spoken reply. Prefer short messages in the language of locale ${locale}. ` +
  "Listing devices plays no sound. Do not send unprompted test speech and do not repeat speech after an error or timeout. " +
  "Success means the API accepted the request, not that the user heard it."

/**
 * The MCP server identity, with instructions for the configured speech locale.
 */
export const serverOptions = AlexaConfig.useSync(({ locale }) => ({
  name,
  version,
  instructions: instructions(locale),
  protocols
}))

/**
 * Registers the voice toolkit on whichever `McpServer` transport is provided.
 */
export const VoiceTools = Layer.unwrap(AlexaConfig.useSync(({ locale }) => {
  const toolkit = makeVoiceToolkit(locale)
  return McpServer.toolkit(toolkit).pipe(Layer.provide(voiceHandlers(toolkit)))
}))

/**
 * Serves MCP over stdin/stdout, the default transport for agents that spawn the server.
 *
 * Only stdio carries the Apple tools (reminders, calendar, maps, phone_call): the
 * client that spawned the process already runs as the user on this Mac. The HTTP
 * transport has no authentication, so it stays voice-only.
 */
export const StdioTransport = Layer.unwrap(Effect.map(serverOptions, (options) =>
  Layer.mergeAll(VoiceTools, AppleTools).pipe(Layer.provide(McpServer.layerStdio(options)))
))
