import { createHash } from "node:crypto"
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync } from "node:fs"
import { homedir } from "node:os"
import { isAbsolute, join } from "node:path"
import { fileURLToPath } from "node:url"
import { Context, Duration, Effect, Layer, Semaphore } from "effect"
import { ChildProcessSpawner } from "effect/process"
import { AutomationError } from "./apple-error.ts"
import { command, type Completed, execute } from "./process.ts"

// EventKit answers a bounded range in tens of milliseconds; the deadline only
// covers a cold calaccessd and the first-run access prompt.
const QUERY_TIMEOUT = Duration.seconds(10)
const BUILD_TIMEOUT = Duration.minutes(2)
const SWIFTC = "/usr/bin/swiftc"
const CODESIGN = "/usr/bin/codesign"
const BUNDLE_ID = "dev.jarvis.calendar-events"

const SOURCE = fileURLToPath(new URL("../native/calendar-events.swift", import.meta.url))
const INFO_PLIST = fileURLToPath(new URL("../native/Info.plist", import.meta.url))

/** Where compiled helpers are cached, outside the repository. */
export const helperDirectory = (): string => {
  const override = process.env["JARVIS_BIN_DIR"]
  return override !== undefined && isAbsolute(override) ? override : join(homedir(), ".local", "share", "jarvis", "bin")
}

const sourceDigest = () =>
  createHash("sha256").update(readFileSync(SOURCE)).update(readFileSync(INFO_PLIST)).digest("hex").slice(0, 12)

const target = () => `${process.arch === "x64" ? "x86_64" : "arm64"}-apple-macos14`

const step = (spawner: ChildProcessSpawner.ChildProcessSpawner["Service"], file: string, args: ReadonlyArray<string>) =>
  execute(spawner, command(file, args), "Calendar helper build", BUILD_TIMEOUT).pipe(
    Effect.filterOrElse(
      ({ exitCode }) => exitCode === ChildProcessSpawner.ExitCode(0),
      ({ stderr }) =>
        Effect.fail(
          new AutomationError({
            message: `Could not build the Calendar helper (needs Xcode Command Line Tools): ${stderr.trim().slice(-500)}`
          })
        )
    )
  )

/** Returns the absolute path of the compiled helper, compiling it if missing. */
export const buildHelper = (spawner: ChildProcessSpawner.ChildProcessSpawner["Service"]) =>
  Effect.gen(function*() {
    const directory = helperDirectory()
    const binary = join(directory, `calendar-events-${yield* Effect.try({
      try: sourceDigest,
      catch: () => new AutomationError({ message: "The Calendar helper sources are missing." })
    })}`)
    if (existsSync(binary)) return binary
    const partial = `${binary}.partial-${process.pid}`
    yield* Effect.try({
      try: () => mkdirSync(directory, { recursive: true, mode: 0o700 }),
      catch: () => new AutomationError({ message: `Could not create ${directory}.` })
    })
    yield* Effect.gen(function*() {
      yield* step(spawner, SWIFTC, [
        "-O",
        "-target",
        target(),
        SOURCE,
        "-Xlinker",
        "-sectcreate",
        "-Xlinker",
        "__TEXT",
        "-Xlinker",
        "__info_plist",
        "-Xlinker",
        INFO_PLIST,
        "-o",
        partial
      ])
      // Re-sign ad hoc with a stable identifier so the embedded Info.plist is bound.
      yield* step(spawner, CODESIGN, ["--force", "--sign", "-", "--identifier", BUNDLE_ID, partial])
      yield* Effect.try({
        try: () => renameSync(partial, binary),
        catch: () => new AutomationError({ message: "Could not install the Calendar helper." })
      })
    }).pipe(Effect.onError(() => Effect.sync(() => rmSync(partial, { force: true }))))
    return binary
  })

/**
 * Runs the EventKit helper (`native/calendar-events.swift`). The binary is
 * compiled on first use into `helperDirectory()`, named after a digest of its
 * sources so an edited helper is rebuilt instead of reusing a stale one.
 */
export class CalendarHelper extends Context.Service<CalendarHelper, {
  readonly run: (args: ReadonlyArray<string>) => Effect.Effect<Completed, AutomationError>
}>()("@jarvis/apple/CalendarHelper") {
  /** Uses an already-built helper at `path` (absolute); nothing is compiled. */
  static readonly layerAt = (path: string) =>
    Layer.effect(
      CalendarHelper,
      Effect.gen(function*() {
        if (!isAbsolute(path)) return yield* new AutomationError({ message: "The Calendar helper path must be absolute." })
        const spawner = yield* ChildProcessSpawner.ChildProcessSpawner
        return CalendarHelper.of({
          run: (args) => execute(spawner, command(path, args), "Calendar helper", QUERY_TIMEOUT)
        })
      })
    )

  /** Builds the helper on demand (Xcode Command Line Tools required) and runs it. */
  static readonly layer = Layer.effect(
    CalendarHelper,
    Effect.gen(function*() {
      const spawner = yield* ChildProcessSpawner.ChildProcessSpawner
      const lock = yield* Semaphore.make(1)
      const ensure = Semaphore.withPermits(lock, 1)(buildHelper(spawner))
      return CalendarHelper.of({
        run: (args) =>
          Effect.flatMap(ensure, (path) => execute(spawner, command(path, args), "Calendar helper", QUERY_TIMEOUT))
      })
    })
  )
}
