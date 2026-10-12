import { assert, describe, it } from "@effect/vitest"
import { ConfigProvider, Duration, Effect, Exit, Fiber, FileSystem, Layer, Path, Sink, Stream } from "effect"
import { PlatformError, SystemError } from "effect/PlatformError"
import { HttpClient, HttpClientError, HttpClientResponse } from "effect/http"
import { ChildProcess, ChildProcessSpawner } from "effect/process"
import { TestClock } from "effect/testing"
import { AFPLAY, LOCKF, MacVoice, MacVoiceConfig, QUEUE_TIMEOUT, SAY, speechTimeout, SYNTHESIS_TIMEOUT } from "../src/index.ts"

interface Reply {
  readonly exitCode?: number
  readonly hang?: boolean
  readonly spawnFails?: boolean
  /** How the fake TTS server answers. Defaults to "down", so these tests take the `say` path. */
  readonly tts?: "down" | "hang" | "wav" | "http500" | "notWav"
}

const LOCK = "/state/jarvis/fala.lock"
const TTS_URL = "http://127.0.0.1:7349"
const WAV_FILE = "/tmp/jarvis-tts-1.wav"
const WAV = new Uint8Array([...new TextEncoder().encode("RIFF"), ...new Array(60).fill(0)])

// Never spawns anything: records each command and answers with a canned exit code.
const fakeSpawner = (reply: Reply = {}) => {
  const commands: Array<ChildProcess.StandardCommand> = []
  const directories: Array<string> = []
  const state = { released: 0 }
  const spawner = ChildProcessSpawner.make((command) =>
    reply.spawnFails === true
      ? Effect.fail(new PlatformError(new SystemError({ _tag: "NotFound", module: "ChildProcess", method: "spawn" })))
      : Effect.acquireRelease(
        Effect.sync(() => {
          if (!ChildProcess.isStandardCommand(command)) throw new Error("unexpected piped command")
          commands.push(command)
          return ChildProcessSpawner.makeHandle({
            pid: ChildProcessSpawner.ProcessId(1),
            exitCode: reply.hang === true
              ? Effect.never
              : Effect.succeed(ChildProcessSpawner.ExitCode(reply.exitCode ?? 0)),
            isRunning: Effect.succeed(reply.hang === true),
            kill: () => Effect.void,
            stdin: Sink.drain,
            stdout: Stream.empty,
            stderr: Stream.empty,
            all: Stream.empty,
            getInputFd: () => Sink.drain,
            getOutputFd: () => Stream.empty,
            unref: Effect.succeed(Effect.void)
          })
        }),
        () => Effect.sync(() => void state.released++)
      )
  )
  // The fake TTS server: never reaches the network.
  const requests: Array<{ readonly url: string; readonly body: unknown }> = []
  const client = HttpClient.make((request, url) =>
    Effect.gen(function*() {
      const body = request.body._tag === "Uint8Array" ? JSON.parse(new TextDecoder().decode(request.body.body)) : null
      requests.push({ url: url.toString(), body })
      switch (reply.tts ?? "down") {
        case "down":
          return yield* Effect.fail(
            new HttpClientError.HttpClientError({
              reason: new HttpClientError.TransportError({ request, description: "connection refused" })
            })
          )
        case "hang":
          return yield* Effect.never
        case "http500":
          return HttpClientResponse.fromWeb(request, new Response("boom", { status: 500 }))
        case "notWav":
          return HttpClientResponse.fromWeb(request, new Response("{}", { status: 200 }))
        case "wav":
          return HttpClientResponse.fromWeb(request, new Response(WAV, { status: 200 }))
      }
    })
  )
  const written: Array<{ readonly path: string; readonly bytes: Uint8Array }> = []
  const removed: Array<string> = []
  const fs = FileSystem.layerNoop({
    makeDirectory: (path) => Effect.sync(() => void directories.push(path)),
    makeTempFileScoped: () =>
      Effect.acquireRelease(Effect.succeed(WAV_FILE), (path) => Effect.sync(() => void removed.push(path))),
    writeFile: (path, bytes) => Effect.sync(() => void written.push({ path, bytes }))
  })
  const layer = MacVoice.layer.pipe(
    Layer.provide(Layer.mergeAll(
      Layer.succeed(ChildProcessSpawner.ChildProcessSpawner, spawner),
      Layer.succeed(
        MacVoiceConfig,
        MacVoiceConfig.of({ voice: "Luciana", lock: LOCK, ttsUrl: TTS_URL, ttsVoice: "pf_dora" })
      ),
      Layer.succeed(HttpClient.HttpClient, client),
      fs,
      Path.layer
    ))
  )
  return { commands, directories, layer, removed, requests, state, written }
}

const speak = (text: string) => MacVoice.use((voice) => voice.speak(text))

describe("MacVoice.speak through say", () => {
  it.effect("runs say under lockf with the exact argv, absolute paths and no shell", () => {
    const fake = fakeSpawner()
    return Effect.gen(function*() {
      const played = yield* speak("  -v Fred; rm -rf ~ `x` $(y)  ")
      assert.deepStrictEqual(played, { status: "accepted", device: "Mac", chunks: 1, playbackConfirmed: true })
      assert.strictEqual(fake.commands.length, 1)
      const [command] = fake.commands
      assert.strictEqual(command?.command, LOCKF)
      assert.strictEqual(LOCKF, "/usr/bin/lockf")
      assert.strictEqual(SAY, "/usr/bin/say")
      assert.deepStrictEqual(command?.args, [
        "-k",
        "-t",
        "120",
        LOCK,
        "/usr/bin/say",
        "-v",
        "Luciana",
        "--",
        "-v Fred; rm -rf ~ `x` $(y)"
      ])
      assert.isUndefined(command?.options.shell)
      assert.strictEqual(command?.options.stdin, "ignore")
      assert.deepStrictEqual(fake.directories, ["/state/jarvis"])
      assert.strictEqual(fake.state.released, 1)
    }).pipe(Effect.provide(fake.layer))
  })

  it.effect("refuses invalid text without spawning", () => {
    const fake = fakeSpawner()
    return Effect.gen(function*() {
      for (const text of ["", "   ", "a".repeat(4001), "<speak>oi</speak>", "1 > 0"]) {
        const error = yield* Effect.flip(speak(text))
        assert.match(error.message, /^Use plain text between 1 and 4000 characters/)
      }
      assert.strictEqual(fake.commands.length, 0)
      yield* speak("a".repeat(4000))
      assert.strictEqual(fake.commands.length, 1)
    }).pipe(Effect.provide(fake.layer))
  })

  it.effect("reports a failing say once, without retrying", () => {
    const fake = fakeSpawner({ exitCode: 1 })
    return Effect.gen(function*() {
      const error = yield* Effect.flip(speak("Olá"))
      assert.strictEqual(error._tag, "MacVoiceError")
      assert.match(error.message, /^say failed with exit code 1/)
      assert.strictEqual(fake.commands.length, 1)
    }).pipe(Effect.provide(fake.layer))
  })

  it.effect("reports a lock held past the queue timeout as busy", () => {
    const fake = fakeSpawner({ exitCode: 75 })
    return Effect.gen(function*() {
      const error = yield* Effect.flip(speak("Olá"))
      assert.match(error.message, /^Mac ocupado: another program kept speaking for 2m\./)
      assert.strictEqual(fake.commands.length, 1)
    }).pipe(Effect.provide(fake.layer))
  })

  it.effect("reports a spawn failure", () => {
    const fake = fakeSpawner({ spawnFails: true })
    return Effect.gen(function*() {
      const error = yield* Effect.flip(speak("Olá"))
      assert.match(error.message, /^Could not run \/usr\/bin\/lockf/)
    }).pipe(Effect.provide(fake.layer))
  })

  it("budgets the queue wait plus playback proportional to the text", () => {
    assert.strictEqual(Duration.toSeconds(QUEUE_TIMEOUT), 120)
    // 120 s of queue + 60 s of margin + 250 ms per character.
    assert.strictEqual(Duration.toSeconds(speechTimeout("Olá")), 180.75)
    // A maximal speech of 4000 characters (about 5 minutes of audio) gets 20 minutes.
    assert.strictEqual(Duration.toSeconds(speechTimeout("a".repeat(4000))), 1180)
  })

  it.effect("does not cut a long speech after the old 90 seconds", () => {
    const fake = fakeSpawner({ hang: true })
    const text = "a".repeat(1500)
    return Effect.gen(function*() {
      const fiber = yield* Effect.forkChild(speak(text))
      yield* TestClock.adjust(Duration.seconds(540))
      assert.isUndefined(fiber.pollUnsafe())
      assert.strictEqual(fake.state.released, 0)
      yield* Fiber.interrupt(fiber)
    }).pipe(Effect.provide(fake.layer))
  })

  it.effect("stops a stuck player only after the proportional budget", () => {
    const fake = fakeSpawner({ hang: true })
    return Effect.gen(function*() {
      const fiber = yield* Effect.forkChild(speak("Olá"))
      yield* TestClock.adjust(Duration.subtract(speechTimeout("Olá"), Duration.millis(1)))
      assert.isUndefined(fiber.pollUnsafe())
      yield* TestClock.adjust(Duration.millis(1))
      const exit = yield* Fiber.await(fiber)
      assert.isTrue(Exit.isFailure(exit))
      const error = yield* Effect.flip(Fiber.join(fiber))
      assert.match(error.message, /did not finish within 3m 750ms and was stopped/)
      // The process scope closed, so the spawner kills the process group.
      assert.strictEqual(fake.state.released, 1)
      assert.strictEqual(fake.commands.length, 1)
    }).pipe(Effect.provide(fake.layer))
  })
})

describe("MacVoice.speak through the TTS server", () => {
  it.effect("plays the synthesized WAV with afplay under the same lock", () => {
    const fake = fakeSpawner({ tts: "wav" })
    return Effect.gen(function*() {
      const played = yield* speak("  Olá, Fernando  ")
      assert.deepStrictEqual(played, { status: "accepted", device: "Mac", chunks: 1, playbackConfirmed: true })
      assert.deepStrictEqual(fake.requests, [
        { url: `${TTS_URL}/synthesize`, body: { text: "Olá, Fernando", voice: "pf_dora" } }
      ])
      assert.deepStrictEqual(fake.written, [{ path: WAV_FILE, bytes: WAV }])
      assert.strictEqual(fake.commands.length, 1)
      const [command] = fake.commands
      assert.strictEqual(command?.command, "/usr/bin/lockf")
      assert.strictEqual(AFPLAY, "/usr/bin/afplay")
      assert.deepStrictEqual(command?.args, ["-k", "-t", "120", LOCK, "/usr/bin/afplay", WAV_FILE])
      assert.isUndefined(command?.options.shell)
      // The temporary WAV is removed once afplay exited.
      assert.deepStrictEqual(fake.removed, [WAV_FILE])
    }).pipe(Effect.provide(fake.layer))
  })

  for (const tts of ["down", "http500", "notWav"] as const) {
    it.effect(`falls back to say once when the server answers ${tts}`, () => {
      const fake = fakeSpawner({ tts })
      return Effect.gen(function*() {
        yield* speak("Olá")
        assert.strictEqual(fake.requests.length, 1)
        assert.strictEqual(fake.commands.length, 1)
        assert.deepStrictEqual(fake.commands[0]?.args, ["-k", "-t", "120", LOCK, SAY, "-v", "Luciana", "--", "Olá"])
      }).pipe(Effect.provide(fake.layer))
    })
  }

  it.effect("falls back to say after 10 seconds without an answer", () => {
    const fake = fakeSpawner({ tts: "hang" })
    return Effect.gen(function*() {
      const fiber = yield* Effect.forkChild(speak("Olá"))
      yield* TestClock.adjust(Duration.subtract(SYNTHESIS_TIMEOUT, Duration.millis(1)))
      assert.strictEqual(fake.commands.length, 0)
      yield* TestClock.adjust(Duration.millis(1))
      yield* Fiber.join(fiber)
      assert.strictEqual(fake.requests.length, 1)
      assert.strictEqual(fake.commands[0]?.args[4], SAY)
    }).pipe(Effect.provide(fake.layer))
  })

  it.effect("reports a failing afplay without falling back to say", () => {
    const fake = fakeSpawner({ tts: "wav", exitCode: 1 })
    return Effect.gen(function*() {
      const error = yield* Effect.flip(speak("Olá"))
      assert.match(error.message, /^afplay failed with exit code 1/)
      assert.strictEqual(fake.commands.length, 1)
      assert.strictEqual(fake.commands[0]?.args[4], AFPLAY)
    }).pipe(Effect.provide(fake.layer))
  })
})

describe("MacVoiceConfig", () => {
  const load = (env: Record<string, string>) =>
    Effect.gen(function*() {
      return yield* MacVoiceConfig
    }).pipe(
      Effect.provide(MacVoiceConfig.layer.pipe(
        Layer.provide(Layer.merge(Path.layer, ConfigProvider.layer(ConfigProvider.fromEnv({ env }))))
      ))
    )

  it.effect("defaults to Luciana and the jarvis-fala lock", () =>
    Effect.gen(function*() {
      const config = yield* load({ HOME: "/Users/test" })
      assert.deepStrictEqual(config, {
        voice: "Luciana",
        lock: "/Users/test/.local/share/jarvis/fala.lock",
        ttsUrl: "http://127.0.0.1:7349",
        ttsVoice: "pf_dora"
      })
    }))

  it.effect("reads AI_VOICE_MAC_VOICE, AI_VOICE_MAC_LOCK, AI_VOICE_TTS_URL and AI_VOICE_TTS_VOICE", () =>
    Effect.gen(function*() {
      const config = yield* load({
        HOME: "/Users/test",
        AI_VOICE_MAC_VOICE: "Felipe",
        AI_VOICE_MAC_LOCK: "/tmp/x/y.lock",
        AI_VOICE_TTS_URL: "http://127.0.0.1:9000/",
        AI_VOICE_TTS_VOICE: "pm_alex"
      })
      assert.deepStrictEqual(config, {
        voice: "Felipe",
        lock: "/tmp/x/y.lock",
        ttsUrl: "http://127.0.0.1:9000",
        ttsVoice: "pm_alex"
      })
    }))
})
