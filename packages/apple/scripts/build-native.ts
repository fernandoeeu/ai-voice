// Compiles the EventKit Calendar helper into the cache outside the repository
// (`~/.local/share/jarvis/bin`, or `$JARVIS_BIN_DIR`). The Calendar service also
// does this on first use; run it ahead of time to avoid that first-call delay.
import { BunServices } from "@effect/platform-bun"
import { Effect } from "effect"
import { ChildProcessSpawner } from "effect/process"
import { buildHelper } from "../src/calendar-helper.ts"

const program = Effect.gen(function*() {
  const spawner = yield* ChildProcessSpawner.ChildProcessSpawner
  const path = yield* buildHelper(spawner)
  yield* Effect.log(`Calendar helper: ${path}`)
})

Effect.runPromise(program.pipe(Effect.provide(BunServices.layer))).catch((error: unknown) => {
  console.error(error)
  process.exit(1)
})
