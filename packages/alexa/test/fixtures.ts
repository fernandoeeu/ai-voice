import { Clock, Effect, FileSystem, Layer, Option, Path, PlatformError } from "effect"
import { TestClock } from "effect/testing"
import { HttpClient, HttpClientResponse } from "effect/http"
import { Alexa, AlexaConfig, makeAlexaRemote, type Session, type SpeechSequence } from "../src/index.ts"

export const STATE_DIR = "/state"
export const SESSION_FILE = `${STATE_DIR}/session.json`

export interface Entry {
  readonly directory: boolean
  readonly mode: number
  readonly content?: string
}

// In-memory FileSystem covering the calls the package makes, recording file modes.
export const makeMemoryFileSystem = () => {
  const entries = new Map<string, Entry>()
  const notFound = (method: string, path: string) =>
    PlatformError.systemError({ _tag: "NotFound", module: "FileSystem", method, pathOrDescriptor: path })
  const layer = FileSystem.layerNoop({
    makeDirectory: (path, options) =>
      Effect.sync(() => {
        if (!entries.has(path)) entries.set(path, { directory: true, mode: options?.mode ?? 0o755 })
      }),
    chmod: (path, mode) =>
      Effect.suspend(() => {
        const entry = entries.get(path)
        if (entry === undefined) return Effect.fail(notFound("chmod", path))
        entries.set(path, { ...entry, mode })
        return Effect.void
      }),
    writeFileString: (path, content, options) =>
      Effect.sync(() => {
        entries.set(path, { directory: false, content, mode: entries.get(path)?.mode ?? options?.mode ?? 0o644 })
      }),
    readFileString: (path) =>
      Effect.suspend(() => {
        const content = entries.get(path)?.content
        return content === undefined ? Effect.fail(notFound("readFileString", path)) : Effect.succeed(content)
      }),
    rename: (from, to) =>
      Effect.suspend(() => {
        const entry = entries.get(from)
        if (entry === undefined) return Effect.fail(notFound("rename", from))
        entries.delete(from)
        entries.set(to, entry)
        return Effect.void
      }),
    remove: (path) => Effect.sync(() => void entries.delete(path))
  })
  const writeSession = (session: Session) =>
    entries.set(SESSION_FILE, { directory: false, mode: 0o600, content: JSON.stringify(session) })
  const readSession = (): Session => JSON.parse(entries.get(SESSION_FILE)?.content ?? "null")
  return { entries, layer: Layer.merge(layer, Path.layer), writeSession, readSession }
}

export const testConfig = (overrides: Partial<AlexaConfig["Service"]> = {}): AlexaConfig["Service"] =>
  AlexaConfig.of({
    stateDir: STATE_DIR,
    amazonPage: "amazon.com",
    locale: "pt-BR",
    alexaHost: Option.none(),
    speechEnabled: true,
    defaultDevice: Option.none(),
    ...overrides
  })

export const echoDevice = (online = true) => ({
  serialNumber: "TEST-ECHO",
  accountName: "Test Echo",
  deviceType: "TEST-TYPE",
  deviceFamily: "ECHO",
  deviceOwnerCustomerId: "TEST-OWNER",
  online
})

export const NOW = Date.UTC(2026, 9, 1)

export interface FakeResponse {
  readonly status: number
  readonly body: string
}

// Fake Alexa web API behind HttpClient, plus a real alexa-remote2 instance for cookie state and
// sequence nodes (createSequenceNode is the library's).
export const makeFixture = Effect.fnUntraced(function*(options: {
  readonly online?: boolean
  readonly config?: Partial<AlexaConfig["Service"]>
} = {}) {
  yield* TestClock.setTime(NOW)
  const remote = makeAlexaRemote()
  const counters = { stops: 0, sends: 0, checks: 0 }
  remote.stop = () => void counters.stops++
  const sent: Array<SpeechSequence> = []
  const api = {
    authStatus: (): number => 200,
    devices: (): FakeResponse => ({
      status: 200,
      body: JSON.stringify({ devices: [echoDevice(options.online), { deviceFamily: "WHA", serialNumber: "GROUP" }] })
    }),
    preview: (): Effect.Effect<FakeResponse> => Effect.succeed({ status: 200, body: "" })
  }
  const requests: Array<{ readonly url: URL; readonly headers: Record<string, string> }> = []
  const client = HttpClient.make((request, url) => {
    requests.push({ url, headers: request.headers })
    const respond = (response: FakeResponse) =>
      HttpClientResponse.fromWeb(request, new Response(response.body, { status: response.status }))
    switch (url.pathname) {
      case "/api/customer-status":
        counters.checks++
        return Effect.succeed(respond({ status: api.authStatus(), body: "{}" }))
      case "/api/endpoints":
        return Effect.succeed(respond({ status: 200, body: "{\"websiteApiUrl\":\"https://alexa.example\"}" }))
      case "/api/devices-v2/device":
        return Effect.succeed(respond(api.devices()))
      case "/api/behaviors/preview": {
        counters.sends++
        const body = request.body._tag === "Uint8Array" ? new TextDecoder().decode(request.body.body) : "{}"
        sent.push(JSON.parse(JSON.parse(body).sequenceJson))
        return Effect.map(api.preview(), respond)
      }
      default:
        return Effect.succeed(respond({ status: 404, body: "" }))
    }
  })
  const fs = makeMemoryFileSystem()
  fs.writeSession({
    localCookie: "csrf=TEST;",
    refreshToken: "fake",
    macDms: {},
    amazonPage: "amazon.com.br",
    tokenDate: yield* Clock.currentTimeMillis
  })
  const alexa = yield* Alexa.make({ makeRemote: () => remote }).pipe(
    Effect.provide(Layer.mergeAll(
      Layer.succeed(AlexaConfig, testConfig(options.config)),
      Layer.succeed(HttpClient.HttpClient, client),
      fs.layer
    ))
  )
  return { alexa, remote, sent, api, requests, counters, fs }
})
