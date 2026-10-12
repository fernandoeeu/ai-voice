import * as BunServices from "@effect/platform-bun/BunServices"
import { assert, describe, it } from "@effect/vitest"
import { Effect, Schema } from "effect"
import { initializeParams, stdioClient, ToolList, ToolResult } from "./mcp-client.ts"

const decodeToolList = Schema.decodeUnknownEffect(ToolList)
const decodeToolResult = Schema.decodeUnknownEffect(ToolResult)

describe("stdio transport", () => {
  it.live("serves the voice tools without a session or audio", () =>
    Effect.gen(function*() {
      const client = yield* stdioClient()

      const initialized = yield* client.request("initialize", initializeParams)
      assert.isUndefined(initialized.error)
      assert.include(JSON.stringify(initialized.result), "Prefer short messages in the language of locale pt-BR")
      yield* client.notify("notifications/initialized")

      const { tools } = yield* client.request("tools/list").pipe(
        Effect.flatMap((response) => decodeToolList(response.result))
      )
      assert.deepStrictEqual(tools.map((tool) => tool.name).sort(), ["list_devices", "speak"])
      const speakTool = tools.find((tool) => tool.name === "speak")
      assert.deepStrictEqual(speakTool?.annotations, {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: false,
        openWorldHint: true
      })
      assert.include(speakTool?.description, "locale pt-BR")
      assert.include(JSON.stringify(speakTool?.inputSchema), "device_serial")
      assert.include(JSON.stringify(speakTool?.inputSchema), "4000")
      assert.strictEqual(tools.find((tool) => tool.name === "list_devices")?.annotations?.readOnlyHint, true)

      const speak = yield* client.request("tools/call", { name: "speak", arguments: { text: "Test" } }).pipe(
        Effect.flatMap((response) => decodeToolResult(response.result))
      )
      assert.isTrue(speak.isError)
      assert.match(speak.content[0].text, /^Speech is disabled/)

      const devices = yield* client.request("tools/call", { name: "list_devices", arguments: {} }).pipe(
        Effect.flatMap((response) => decodeToolResult(response.result))
      )
      assert.isTrue(devices.isError)
      assert.match(devices.content[0].text, /^Amazon session is missing/)
    }).pipe(Effect.provide(BunServices.layer)), 30_000)
})
