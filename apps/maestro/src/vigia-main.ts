#!/usr/bin/env bun
/**
 * Vigia do maestro (LaunchAgent dev.jarvis.maestro, StartInterval 60): uma
 * passada por execução. Se o agente sumiu do herdr, recria o pane e retoma a
 * sessão salva. Com o herdr fora do ar, só registra e sai.
 */
import { BunRuntime, BunServices } from "@effect/platform-bun"
import { Cause, Effect, Layer, Logger, Option } from "effect"
import { MaestroConfig } from "./config.ts"
import { Herdr } from "./herdr.ts"
import { vigiar } from "./vigia.ts"

const main = Effect.gen(function*() {
  const config = yield* MaestroConfig
  const resultado = yield* vigiar(config).pipe(Effect.provide(Herdr.layer(config.herdr)))
  if (resultado._tag === "Recriado") {
    const sessao = Option.match(resultado.retomou, { onNone: () => "sessão nova", onSome: (id) => `retomou ${id}` })
    yield* Effect.logInfo(`Agente ${config.name} recriado no pane ${resultado.pane} (${sessao}).`)
  }
})

Effect.sync(() => process.umask(0o077)).pipe(
  Effect.andThen(main),
  Effect.tapCause((cause) => Cause.hasInterruptsOnly(cause) ? Effect.void : Effect.logError(Cause.squash(cause))),
  Effect.provide(Layer.mergeAll(BunServices.layer, Layer.succeed(Logger.LogToStderr, true))),
  BunRuntime.runMain({ disableErrorReporting: true })
)
