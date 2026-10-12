import { Alexa, AlexaConfig, login } from "@jarvis/alexa"
import { Config, Console, Effect, Layer, Option, Schema } from "effect"
import { Command } from "effect/cli"
import { FetchHttpClient } from "effect/http"
import { HttpTransport } from "./http-transport.ts"
import { StdioTransport } from "./server.ts"
import { EchoDevice } from "./voice-toolkit.ts"

const AlexaLive = Alexa.layer.pipe(
  Layer.provideMerge(AlexaConfig.layer),
  Layer.provide(FetchHttpClient.layer)
)

const HttpOptions = Config.all({
  port: Config.option(Config.Port("AI_VOICE_HTTP_PORT")),
  hostname: Config.String("AI_VOICE_HTTP_HOST").pipe(Config.withDefault("127.0.0.1"))
})

const serve = Effect.gen(function*() {
  const { hostname, port } = yield* HttpOptions
  const transport = Option.match(port, {
    onNone: () => StdioTransport,
    onSome: (port) => HttpTransport({ hostname, port })
  })
  return yield* Layer.launch(transport)
})

const loginCommand = Command.make("login", {}, () => login).pipe(
  Command.withDescription("Sign in to Amazon through a local proxy at http://127.0.0.1:3456/ (no audio)."),
  Command.provide(AlexaConfig.layer)
)

const encodeDevices = Schema.encodeEffect(Schema.Array(EchoDevice))

const devicesCommand = Command.make("devices", {}, () =>
  Alexa.use((alexa) => alexa.devices).pipe(
    Effect.flatMap(encodeDevices),
    // stdout carries only the JSON so it can be piped; diagnostics go to stderr.
    Effect.flatMap((devices) => Console.log(JSON.stringify(devices, null, 2)))
  )).pipe(
    Command.withDescription("Print the Echo devices on the Amazon account as JSON, without playing audio."),
    Command.provide(AlexaLive)
  )

/**
 * `jarvis-voice` serves MCP (stdio, or HTTP when `AI_VOICE_HTTP_PORT` is set);
 * `login` and `devices` manage the local Amazon session.
 */
export const cli = Command.make("jarvis-voice", {}, () => serve).pipe(
  Command.withDescription("MCP server that lets AI agents speak through an Amazon Echo."),
  Command.provide(AlexaLive),
  Command.withSubcommands([loginCommand, devicesCommand])
)
