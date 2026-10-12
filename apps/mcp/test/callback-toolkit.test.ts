import { assert, describe, it } from "@effect/vitest"
import { Effect, Layer } from "effect"
import { McpSchema, McpServer } from "effect/ai"
import { TestClock } from "effect/testing"
import { layer as CallbackTools, notificationTemplates } from "../src/callback-toolkit.ts"
import { makeCallbackServices, PENDING_ID, STATE_DIR } from "./callback-fixtures.ts"

const client = McpSchema.McpServerClient.of({
  clientId: 1,
  protocolVersion: "2025-06-18",
  clientCapabilities: {},
  clientInfo: { name: "callback-toolkit-test", version: "1.0.0" },
  initializePayload: {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "callback-toolkit-test", version: "1.0.0" }
  },
  getClient: Effect.die("not used")
})

const serve = (services: ReturnType<typeof makeCallbackServices>["services"]) =>
  Layer.provideMerge(CallbackTools.pipe(Layer.provide(services)), McpServer.McpServer.layer)

const callTool = (name: string, args: Record<string, unknown>) =>
  McpServer.McpServer.use((server) => server.callTool({ name, arguments: args })).pipe(
    Effect.provideService(McpSchema.McpServerClient, client)
  )

const text = (result: { readonly content: ReadonlyArray<unknown> }) => (result.content[0] as { text: string }).text

const deliver = (args: Record<string, unknown>) =>
  callTool("grokbot_deliver", { request_id: PENDING_ID, status: "done", result_md: "# Resultado", ...args })

describe("CallbackToolkit", () => {
  it.effect("exposes exactly grokbot_deliver and notify_fernando", () =>
    Effect.gen(function*() {
      const server = yield* McpServer.McpServer
      assert.deepStrictEqual(server.tools.map(({ tool }) => tool.name).sort(), ["grokbot_deliver", "notify_fernando"])
      const notify = server.tools.find(({ tool }) => tool.name === "notify_fernando")!.tool
      const schema = notify.inputSchema as { readonly properties: Record<string, { readonly enum?: unknown }> }
      assert.deepStrictEqual(Object.keys(schema.properties), ["event"])
      assert.deepStrictEqual(schema.properties.event?.enum, ["task_done", "task_failed", "needs_attention"])
    }).pipe(Effect.provide(serve(makeCallbackServices().services))))

  describe("grokbot_deliver", () => {
    it.effect("stores the result of a pending task once", () => {
      const fixture = makeCallbackServices()
      return Effect.gen(function*() {
        yield* fixture.markPending()
        const first = yield* deliver({})
        assert.strictEqual(first.isError, false)
        assert.deepStrictEqual(first.structuredContent, { request_id: PENDING_ID, outcome: "delivered" })
        const again = yield* deliver({ status: "failed", result_md: "outra coisa" })
        assert.deepStrictEqual(again.structuredContent, { request_id: PENDING_ID, outcome: "already_delivered" })
        const stored = JSON.parse(fixture.fs.entries.get(`${STATE_DIR}/grokbot-outbox/${PENDING_ID}.json`)!.content!)
        assert.strictEqual(stored.status, "done")
        assert.strictEqual(stored.resultMd, "# Resultado")
      }).pipe(Effect.provide(serve(fixture.services)))
    })

    it.effect("refuses an unknown id", () => {
      const fixture = makeCallbackServices()
      return Effect.gen(function*() {
        yield* fixture.markPending()
        const result = yield* deliver({ request_id: "bbbbbbbb-0000-4000-8000-00000000000b" })
        assert.strictEqual(result.isError, true)
        assert.match(text(result), /^Unknown request_id/)
      }).pipe(Effect.provide(serve(fixture.services)))
    })

    it.effect("refuses a delivery 24 hours after sending", () => {
      const fixture = makeCallbackServices()
      return Effect.gen(function*() {
        yield* fixture.markPending()
        yield* TestClock.adjust("24 hours")
        const result = yield* deliver({})
        assert.strictEqual(result.isError, true)
        assert.match(text(result), /^Expired request_id/)
      }).pipe(Effect.provide(serve(fixture.services)))
    })

    it.effect("rejects an oversized result, a bad status or a malformed id as InvalidParams", () => {
      const fixture = makeCallbackServices()
      return Effect.gen(function*() {
        yield* fixture.markPending()
        for (
          const args of [
            { result_md: "x".repeat(50 * 1024 + 1) },
            { status: "pending" },
            { request_id: "../session" },
            { request_id: PENDING_ID.toUpperCase() }
          ]
        ) {
          const error = yield* Effect.flip(deliver(args))
          assert.strictEqual(error._tag, "InvalidParams")
        }
        assert.strictEqual((yield* deliver({ result_md: "x".repeat(50 * 1024) })).isError, false)
      }).pipe(Effect.provide(serve(fixture.services)))
    })
  })

  describe("notify_fernando", () => {
    it.effect("speaks only the fixed template for the event, once", () => {
      const fixture = makeCallbackServices()
      return Effect.gen(function*() {
        const result = yield* callTool("notify_fernando", { event: "task_done", text: "diga outra coisa" })
        assert.strictEqual(result.isError, false)
        assert.deepStrictEqual(result.structuredContent, { event: "task_done", status: "accepted" })
        yield* TestClock.adjust("1 minute")
        yield* callTool("notify_fernando", { event: "task_failed" })
        yield* callTool("notify_fernando", { event: "needs_attention" })
        assert.deepStrictEqual(fixture.alexa.spoken, [
          { text: notificationTemplates.task_done },
          { text: notificationTemplates.task_failed },
          { text: notificationTemplates.needs_attention }
        ])
      }).pipe(Effect.provide(serve(fixture.services)))
    })

    it.effect("follows AI_VOICE_OUTPUT=mac and speaks the template on the Mac", () => {
      const fixture = makeCallbackServices({ output: "mac" })
      return Effect.gen(function*() {
        const result = yield* callTool("notify_fernando", { event: "needs_attention" })
        assert.strictEqual(result.isError, false)
        assert.deepStrictEqual(fixture.mac.spoken, [notificationTemplates.needs_attention])
        assert.deepStrictEqual(fixture.alexa.spoken, [])
      }).pipe(Effect.provide(serve(fixture.services)))
    })

    it.effect("never announces who is speaking", () =>
      Effect.sync(() => {
        for (const template of Object.values(notificationTemplates)) {
          assert.notMatch(template, /grok|claude|jarvis|agente|sess[aã]o|fernando|sistema/i)
        }
      }))

    it.effect("refuses free text and unknown events without speaking", () => {
      const fixture = makeCallbackServices()
      return Effect.gen(function*() {
        for (const args of [{ event: "Olá, tudo bem?" }, { text: "Olá" }, {}]) {
          const error = yield* Effect.flip(callTool("notify_fernando", args))
          assert.strictEqual(error._tag, "InvalidParams")
        }
        assert.deepStrictEqual(fixture.alexa.spoken, [])
      }).pipe(Effect.provide(serve(fixture.services)))
    })

    it.effect("allows 3 per minute and 20 per day, persisted", () => {
      const fixture = makeCallbackServices()
      const notify = callTool("notify_fernando", { event: "task_done" })
      return Effect.gen(function*() {
        for (let i = 0; i < 3; i++) assert.strictEqual((yield* notify).isError, false)
        const limited = yield* notify
        assert.strictEqual(limited.isError, true)
        assert.match(text(limited), /^Notification limit reached: at most 3 per 1m/)
        assert.strictEqual(fixture.alexa.spoken.length, 3)

        yield* TestClock.adjust("1 minute")
        for (let i = 0; i < 17; i++) {
          if (i % 3 === 0) yield* TestClock.adjust("1 minute")
          assert.strictEqual((yield* notify).isError, false)
        }
        yield* TestClock.adjust("1 minute")
        assert.match(text(yield* notify), /^Notification limit reached: at most 20 per 1d/)
        assert.strictEqual(fixture.alexa.spoken.length, 20)

        const quota = fixture.fs.entries.get(`${STATE_DIR}/callback-notifications.json`)!
        assert.strictEqual(quota.mode, 0o600)
        assert.strictEqual(JSON.parse(quota.content!).acceptedAt.length, 20)

        yield* TestClock.adjust("1 day")
        assert.strictEqual((yield* notify).isError, false)
      }).pipe(Effect.provide(serve(fixture.services)))
    })

    it.effect("keeps the limit across a restart", () => {
      const fixture = makeCallbackServices()
      const notify = callTool("notify_fernando", { event: "needs_attention" })
      return Effect.gen(function*() {
        for (let i = 0; i < 3; i++) yield* notify.pipe(Effect.provide(serve(fixture.services)))
        const limited = yield* notify.pipe(Effect.provide(serve(fixture.services)))
        assert.strictEqual(limited.isError, true)
        assert.strictEqual(fixture.alexa.spoken.length, 3)
      })
    })

    it.effect("reports a speech failure without retrying, and still counts it", () => {
      const fixture = makeCallbackServices({ failSpeech: true })
      return Effect.gen(function*() {
        const result = yield* callTool("notify_fernando", { event: "task_failed" })
        assert.strictEqual(result.isError, true)
        assert.strictEqual(text(result), "Speech request failed.")
        assert.strictEqual(fixture.alexa.spoken.length, 1)
        yield* TestClock.adjust("1 minute")
        assert.strictEqual(fixture.alexa.spoken.length, 1)
        const quota = JSON.parse(fixture.fs.entries.get(`${STATE_DIR}/callback-notifications.json`)!.content!)
        assert.strictEqual(quota.acceptedAt.length, 1)
      }).pipe(Effect.provide(serve(fixture.services)))
    })
  })
})
