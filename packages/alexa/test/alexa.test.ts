import { assert, describe, it } from "@effect/vitest"
import { Deferred, Effect, Exit, Fiber, Layer, Option, Scope } from "effect"
import { TestClock } from "effect/testing"
import { HttpClient } from "effect/http"
import { Alexa, AlexaConfig, type SequenceNode } from "../src/index.ts"
import { makeFixture, makeMemoryFileSystem, NOW, SESSION_FILE, STATE_DIR, testConfig } from "./fixtures.ts"

const failure = <A, E extends { readonly _tag: string; readonly message: string }>(effect: Effect.Effect<A, E>) =>
  Effect.flip(effect)

const payload = (node: SequenceNode) => node.operationPayload

const noNetwork = Layer.succeed(HttpClient.HttpClient, HttpClient.make(() => assert.fail("must not run")))

describe("Alexa", () => {
  it.effect("refuses speech when disabled, before authenticating or opening the network", () =>
    Effect.gen(function*() {
      const fs = makeMemoryFileSystem()
      const alexa = yield* Alexa.make({ makeRemote: () => assert.fail("must not run") }).pipe(
        Effect.provide(Layer.mergeAll(Layer.succeed(AlexaConfig, testConfig({ speechEnabled: false })), noNetwork, fs.layer))
      )
      const error = yield* failure(alexa.speak({ deviceSerial: "TEST", text: "Test" }))
      assert.strictEqual(error._tag, "SpeechError")
      assert.match(error.message, /Speech is disabled/)
    }))

  it.effect("rejects empty text, SSML and overlong text before connecting", () =>
    Effect.gen(function*() {
      const fs = makeMemoryFileSystem()
      const alexa = yield* Alexa.make({ makeRemote: () => assert.fail("must not run") }).pipe(
        Effect.provide(Layer.mergeAll(Layer.succeed(AlexaConfig, testConfig()), noNetwork, fs.layer))
      )
      for (const text of ["", "  ", "<speak>Hello</speak>", "a".repeat(4001)]) {
        const error = yield* failure(alexa.speak({ deviceSerial: "TEST", text }))
        assert.match(error.message, /plain text/)
      }
    }))

  it.effect("reports a missing session without leaking its contents", () =>
    Effect.gen(function*() {
      const { alexa, fs } = yield* makeFixture()
      fs.entries.delete(SESSION_FILE)
      const error = yield* failure(alexa.devices)
      assert.strictEqual(error._tag, "SessionError")
      assert.match(error.message, /Amazon session is missing or invalid/)
      fs.entries.set(SESSION_FILE, { directory: false, mode: 0o600, content: "{\"localCookie\":\"SECRET\"}" })
      const invalid = yield* failure(alexa.devices)
      assert.notInclude(invalid.message, "SECRET")
    }))

  // Non-.com marketplace and non-English text: the library must not rewrite decimals or the locale.
  it.effect("builds speech for a single Echo, preserving the configured locale and numbers", () =>
    Effect.gen(function*() {
      const { alexa, sent } = yield* makeFixture()
      const text = "Olá! O valor é 1.25 e a versão é 2.3."
      const result = yield* alexa.speak({ deviceSerial: "TEST-ECHO", text })
      const node = sent[0]!.startNode as SequenceNode
      assert.strictEqual(node.type, "Alexa.Speak")
      assert.deepInclude(node.operationPayload, {
        deviceSerialNumber: "TEST-ECHO",
        deviceType: "TEST-TYPE",
        customerId: "TEST-OWNER",
        locale: "pt-BR",
        textToSpeak: text
      })
      assert.deepStrictEqual(result, { status: "accepted", device: "Test Echo", chunks: 1, playbackConfirmed: false })
    }))

  it.effect("uses the default Echo and sends every chunk in a single SerialNode", () =>
    Effect.gen(function*() {
      const { alexa, sent } = yield* makeFixture({ config: { defaultDevice: Option.some("TEST-ECHO") } })
      const text = "This is a text that will be spoken in ordered parts. ".repeat(12).trim()
      const result = yield* alexa.speak({ text })
      assert.isTrue((yield* alexa.devices)[0]!.isDefault)
      const start = sent[0]!.startNode
      assert.strictEqual(start["@type"], "com.amazon.alexa.behaviors.model.SerialNode")
      const nodes = (start as { readonly nodesToExecute: ReadonlyArray<SequenceNode> }).nodesToExecute
      assert.strictEqual(nodes.length, result.chunks)
      assert.strictEqual(nodes.map((node) => payload(node).textToSpeak).join(" "), text)
      assert.isTrue(nodes.every((node) =>
        payload(node).locale === "pt-BR" && payload(node).deviceSerialNumber === "TEST-ECHO"
      ))
    }))

  it.effect("does not pick a target automatically when there is no default Echo", () =>
    Effect.gen(function*() {
      const { alexa, counters } = yield* makeFixture()
      const error = yield* failure(alexa.speak({ text: "Test" }))
      assert.strictEqual(error._tag, "DeviceError")
      assert.match(error.message, /AI_VOICE_DEFAULT_DEVICE/)
      assert.strictEqual(counters.sends, 0)
    }))

  it.effect("renews an old session and persists the tokens without opening a proxy", () =>
    Effect.gen(function*() {
      const { alexa, remote, fs, requests } = yield* makeFixture()
      const previous = { ...fs.readSession(), tokenDate: NOW - 25 * 60 * 60 * 1000, registration: { kept: true } }
      fs.writeSession(previous)
      let refreshes = 0
      remote.refreshCookie = (callback) => {
        assert.strictEqual(remote._options.setupProxy, false)
        refreshes++
        callback(null, {
          ...previous,
          localCookie: "csrf=RENEWED;",
          refreshToken: "new-token",
          macDms: { token: "new-jwt" },
          tokenDate: NOW
        })
      }
      yield* alexa.devices
      assert.strictEqual(refreshes, 1)
      assert.strictEqual(fs.readSession().refreshToken, "new-token")
      assert.deepStrictEqual(fs.readSession().registration, { kept: true })
      assert.strictEqual(fs.entries.get(SESSION_FILE)!.mode, 0o600)
      assert.strictEqual(fs.entries.get(STATE_DIR)!.mode, 0o700)
      assert.strictEqual(remote.csrf, "RENEWED")
      assert.deepStrictEqual(remote.macDms, { token: "new-jwt" })
      assert.strictEqual(remote.baseUrl, "alexa.example")
      assert.strictEqual(requests[0]!.url.host, "alexa.amazon.com.br")
      assert.strictEqual(requests[0]!.headers.cookie, "csrf=RENEWED;")
      assert.strictEqual(requests.at(-1)!.url.host, "alexa.example")
    }))

  it.effect("refresh after an invalid cookie happens once; failure does not loop", () =>
    Effect.gen(function*() {
      const { alexa, remote, fs, counters, api } = yield* makeFixture()
      let refreshes = 0
      api.authStatus = () => 401
      remote.refreshCookie = (callback) => {
        refreshes++
        callback(null, fs.readSession())
      }
      const error = yield* failure(alexa.devices)
      assert.match(error.message, /Amazon session is invalid/)
      assert.strictEqual(refreshes, 1)
      assert.strictEqual(counters.stops, 1)
    }))

  it.effect("long-running process checks authentication again after 30 minutes", () =>
    Effect.gen(function*() {
      const { alexa, counters } = yield* makeFixture()
      yield* alexa.devices
      yield* alexa.devices
      assert.strictEqual(counters.checks, 1)
      yield* TestClock.adjust("31 minutes")
      yield* alexa.devices
      assert.strictEqual(counters.checks, 2)
      assert.strictEqual(counters.stops, 1)
    }))

  it.effect("renewal failure asks for a new login and allows retrying", () =>
    Effect.gen(function*() {
      const { alexa, remote, api } = yield* makeFixture()
      api.authStatus = () => 401
      remote.refreshCookie = (callback) => callback(new Error("SECRET-TOKEN"))
      const error = yield* failure(alexa.devices)
      assert.match(error.message, /jarvis-voice login/)
      assert.notInclude(error.message, "SECRET")
      api.authStatus = () => 200
      assert.strictEqual((yield* alexa.devices).length, 1)
    }))

  it.effect("a failed device listing drops the connection so the next call checks the session again", () =>
    Effect.gen(function*() {
      const { alexa, api, counters } = yield* makeFixture()
      const devices = api.devices
      api.devices = () => ({ status: 503, body: "" })
      const error = yield* failure(alexa.devices)
      assert.strictEqual(error._tag, "DeviceError")
      assert.strictEqual(counters.stops, 1)
      api.devices = devices
      yield* alexa.devices
      assert.strictEqual(counters.checks, 2)
    }))

  it.effect("rejects concurrent requests and never repeats speech when the transport fails", () =>
    Effect.gen(function*() {
      const { alexa, api, counters } = yield* makeFixture()
      const started = yield* Deferred.make<void>()
      const outcome = yield* Deferred.make<void, never>()
      api.preview = () =>
        Deferred.succeed(started, undefined).pipe(
          Effect.andThen(Deferred.await(outcome)),
          Effect.as({ status: 500, body: "" })
        )
      const first = yield* Effect.forkChild(alexa.speak({ deviceSerial: "TEST-ECHO", text: "Test" }))
      yield* Deferred.await(started)
      const concurrent = yield* failure(alexa.speak({ deviceSerial: "TEST-ECHO", text: "Another test" }))
      assert.match(concurrent.message, /in progress/)
      yield* Deferred.succeed(outcome, undefined)
      const exit = yield* Fiber.await(first)
      assert.isTrue(Exit.isFailure(exit))
      assert.strictEqual(counters.sends, 1)
      api.preview = () => Effect.succeed({ status: 200, body: "" })
      yield* alexa.speak({ deviceSerial: "TEST-ECHO", text: "Test" })
      assert.strictEqual(counters.sends, 2)
    }))

  it.effect("excludes groups and rejects unknown/offline devices without sending", () =>
    Effect.gen(function*() {
      const { alexa, counters } = yield* makeFixture({ online: false })
      assert.deepStrictEqual(yield* alexa.devices, [
        { name: "Test Echo", serial: "TEST-ECHO", online: false, isDefault: false }
      ])
      assert.match((yield* failure(alexa.speak({ deviceSerial: "GROUP", text: "Test" }))).message, /not found/)
      assert.match((yield* failure(alexa.speak({ deviceSerial: "TEST-ECHO", text: "Test" }))).message, /offline/)
      assert.strictEqual(counters.sends, 0)
    }))

  it.effect("distinguishes an unreachable Alexa from a rejected session", () =>
    Effect.gen(function*() {
      const { alexa, api, remote } = yield* makeFixture()
      remote.refreshCookie = (callback) => callback(new Error("offline"))
      api.authStatus = () => 503
      assert.match((yield* failure(alexa.devices)).message, /Could not connect to Alexa/)
      api.authStatus = () => 302
      assert.match((yield* failure(alexa.devices)).message, /could not be renewed/)
    }))

  it.effect("stops the remote when the service is released", () =>
    Effect.gen(function*() {
      const scope = yield* Scope.make()
      const { alexa, counters } = yield* makeFixture().pipe(Scope.provide(scope))
      yield* alexa.devices
      assert.strictEqual(counters.stops, 0)
      yield* Scope.close(scope, Exit.void)
      assert.strictEqual(counters.stops, 1)
    }))
})
