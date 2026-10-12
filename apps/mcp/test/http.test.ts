import * as BunServices from "@effect/platform-bun/BunServices"
import { assert, describe, it } from "@effect/vitest"
import { Alexa, AlexaConfig, SpeechError } from "@jarvis/alexa"
import { Effect, Layer, Option, Schema, Stream } from "effect"
import { HttpRouter } from "effect/http"
import { createServer } from "node:net"
import { McpRoutes } from "../src/http-transport.ts"
import { httpClient, spawnServer, ToolList, ToolResult } from "./mcp-client.ts"

const decodeToolList = Schema.decodeUnknownEffect(ToolList)
const decodeToolResult = Schema.decodeUnknownEffect(ToolResult)

const FakeConfig = Layer.succeed(
  AlexaConfig,
  AlexaConfig.of({
    stateDir: "/nonexistent",
    amazonPage: "amazon.com.br",
    locale: "pt-BR",
    alexaHost: Option.none(),
    speechEnabled: true,
    defaultDevice: Option.some("ECHO-1")
  })
)

const FakeAlexa = Layer.succeed(
  Alexa,
  Alexa.of({
    devices: Effect.succeed([{ name: "Kitchen", serial: "ECHO-1", online: true, isDefault: true }]),
    speak: ({ deviceSerial, text }) =>
      text === "busy"
        ? Effect.fail(new SpeechError({ message: "Another speech request is in progress." }))
        : Effect.succeed({
          status: "accepted",
          device: deviceSerial ?? "ECHO-1",
          chunks: 1,
          playbackConfirmed: false
        } as const)
  })
)

// The server requires a real port (1-65535), so borrow one the OS knows is free.
const freePort = Effect.callback<number>((resume) => {
  const server = createServer().listen(0, "127.0.0.1", () => {
    const address = server.address()
    const port = typeof address === "object" && address !== null ? address.port : 0
    server.close(() => resume(Effect.succeed(port)))
  })
})

describe("HTTP transport", () => {
  it.live("serves the tools on /mcp and refuses browser requests", () =>
    Effect.gen(function*() {
      const port = yield* freePort
      const { handle } = yield* spawnServer({ AI_VOICE_HTTP_PORT: String(port) })
      const listening = yield* handle.stderr.pipe(
        Stream.decodeText,
        Stream.splitLines,
        Stream.filter((line) => line.includes("listening on")),
        Stream.runHead,
        Effect.timeout("15 seconds")
      )
      const url = Option.getOrThrow(listening).match(/(http:\/\/\S+\/mcp)$/)?.[1]
      assert.isDefined(url)
      assert.strictEqual(url, `http://127.0.0.1:${port}/mcp`)

      const client = httpClient(url, (request) => fetch(request))
      const initialized = yield* client.initialize
      assert.isUndefined(initialized.error)
      const { tools } = yield* client.request("tools/list").pipe(
        Effect.flatMap((response) => decodeToolList(response.result))
      )
      assert.deepStrictEqual(tools.map((tool) => tool.name).sort(), ["list_devices", "speak"])

      const speak = yield* client.request("tools/call", { name: "speak", arguments: { text: "Test" } }).pipe(
        Effect.flatMap((response) => decodeToolResult(response.result))
      )
      assert.isTrue(speak.isError)
      assert.match(speak.content[0].text, /^Speech is disabled/)

      const fromBrowser = yield* client.post(
        { id: 99, method: "tools/list", params: {} },
        { origin: "https://evil.example" }
      )
      assert.strictEqual(fromBrowser.status, 403)

      const elsewhere = yield* Effect.promise(() => fetch(url.replace(/\/mcp$/, "/other")))
      assert.strictEqual(elsewhere.status, 404)
    }).pipe(Effect.provide(BunServices.layer)), 30_000)

  it.live("keeps the snake_case wire format and reports Alexa errors as tool errors", () =>
    Effect.gen(function*() {
      const { dispose, handler } = HttpRouter.toWebHandler(
        McpRoutes.pipe(Layer.provide([FakeAlexa, FakeConfig])),
        { disableLogger: true }
      )
      yield* Effect.addFinalizer(() => Effect.promise(dispose))
      const client = httpClient("http://localhost/mcp", (request) => handler(request))
      yield* client.initialize

      const devices = yield* client.request("tools/call", { name: "list_devices", arguments: {} }).pipe(
        Effect.flatMap((response) => decodeToolResult(response.result))
      )
      assert.isFalse(devices.isError)
      assert.deepStrictEqual(JSON.parse(devices.content[0].text), [
        { name: "Kitchen", serial: "ECHO-1", online: true, is_default: true }
      ])

      const speak = yield* client.request("tools/call", {
        name: "speak",
        arguments: { device_serial: "ECHO-1", text: "Olá" }
      }).pipe(Effect.flatMap((response) => decodeToolResult(response.result)))
      assert.isFalse(speak.isError)
      assert.deepStrictEqual(JSON.parse(speak.content[0].text), {
        status: "accepted",
        device: "ECHO-1",
        chunks: 1,
        playback_confirmed: false
      })

      const busy = yield* client.request("tools/call", { name: "speak", arguments: { text: "busy" } }).pipe(
        Effect.flatMap((response) => decodeToolResult(response.result))
      )
      assert.deepStrictEqual(busy, {
        isError: true,
        content: [{ type: "text", text: "Another speech request is in progress." }]
      })

      const tooLong = yield* client.request("tools/call", { name: "speak", arguments: { text: "a".repeat(4001) } })
      assert.isDefined(tooLong.error)
    }))
})
