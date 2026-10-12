import { AlexaConfig } from "@jarvis/alexa"
import {
  DeliveryOutcome,
  DeliveryStatus,
  GrokBotOutbox,
  GrokBotOutboxError,
  MAX_RESULT_BYTES,
  RequestId,
  ResultMarkdown
} from "@jarvis/grokbot"
import { Clock, Duration, Effect, FileSystem, Layer, Path, Random, Schema, Semaphore } from "effect"
import { McpServer, Tool, Toolkit } from "effect/ai"
import { SpeakError, Speaker } from "./speaker.ts"

export const NotifyEvent = Schema.Literals(["task_done", "task_failed", "needs_attention"])
export type NotifyEvent = typeof NotifyEvent.Type

/**
 * The only sentences `notify_fernando` can speak. Callers pick an event and
 * never supply text, so nothing a remote caller writes is ever spoken.
 */
export const notificationTemplates: Readonly<Record<NotifyEvent, string>> = {
  task_done: "Terminei aquela tarefa, o resultado já chegou.",
  task_failed: "Não consegui terminar aquela tarefa, dá uma olhada quando puder.",
  needs_attention: "Preciso da sua atenção num negócio."
}

export const NOTIFICATION_LIMITS = [
  { window: Duration.minutes(1), max: 3 },
  { window: Duration.days(1), max: 20 }
] as const

const QUOTA_FILE = "callback-notifications.json"

export class NotificationError extends Schema.TaggedError<NotificationError>()("NotificationError", {
  message: Schema.String
}) {}

const GrokBotDeliver = Tool.make("grokbot_deliver", {
  description: "Delivers the result of a task that Jarvis sent with a request_id. " +
    "Accepted once per request_id, within 24 hours of sending; a repeated delivery is reported as already_delivered " +
    "and does not replace the first.",
  parameters: Schema.Struct({
    requestId: RequestId.annotate({ description: "The request_id received with the task." }),
    status: DeliveryStatus.annotate({ description: "done when the task succeeded, failed otherwise." }),
    resultMd: ResultMarkdown.annotate({
      description: `The result as Markdown, up to ${MAX_RESULT_BYTES} bytes of UTF-8.`
    })
  }).pipe(Schema.encodeKeys({ requestId: "request_id", resultMd: "result_md" })),
  success: Schema.Struct({ requestId: Schema.String, outcome: DeliveryOutcome }).pipe(
    Schema.encodeKeys({ requestId: "request_id" })
  ),
  failure: GrokBotOutboxError
})
  .annotate(Tool.Readonly, false)
  .annotate(Tool.Destructive, false)
  .annotate(Tool.Idempotent, true)
  .annotate(Tool.OpenWorld, false)

const NotifyFernando = Tool.make("notify_fernando", {
  description: "Speaks a short, fixed notice for the given event to the user, on the Mac or the Echo. " +
    "At most 3 per minute and 20 per day; never retry after an error.",
  parameters: Schema.Struct({
    event: NotifyEvent.annotate({ description: "What happened: task_done, task_failed or needs_attention." })
  }),
  success: Schema.Struct({ event: NotifyEvent, status: Schema.Literal("accepted") }),
  failure: Schema.Union([SpeakError, NotificationError])
})
  .annotate(Tool.Readonly, false)
  .annotate(Tool.Destructive, false)
  .annotate(Tool.Idempotent, false)
  .annotate(Tool.OpenWorld, true)

/**
 * The only tools the public callback endpoint serves.
 */
export const CallbackToolkit = Toolkit.make(GrokBotDeliver, NotifyFernando)

const Quota = Schema.Struct({ acceptedAt: Schema.Array(Schema.Int) })
const QuotaJson = Schema.fromJsonString(Quota)

const quotaUnavailable = () =>
  new NotificationError({ message: "The notification quota could not be read or saved, so nothing was spoken." })

// Persisted so a restart cannot reset the limits.
const makeQuota = Effect.fnUntraced(function*(stateDir: string) {
  const fs = yield* FileSystem.FileSystem
  const path = yield* Path.Path
  const file = path.join(stateDir, QUOTA_FILE)
  const lock = yield* Semaphore.make(1)
  const longest = Math.max(...NOTIFICATION_LIMITS.map(({ window }) => Duration.toMillis(window)))

  const load = fs.readFileString(file).pipe(
    Effect.flatMap(Schema.decodeUnknownEffect(QuotaJson)),
    Effect.map(({ acceptedAt }) => acceptedAt),
    Effect.catchIf(
      (error) => error._tag === "PlatformError" && error.reason._tag === "NotFound",
      () => Effect.succeed([])
    )
  )

  const save = Effect.fnUntraced(function*(acceptedAt: ReadonlyArray<number>) {
    yield* fs.makeDirectory(stateDir, { recursive: true, mode: 0o700 })
    const temporary = `${file}.${yield* Random.nextInt}.tmp`
    const contents = yield* Schema.encodeEffect(QuotaJson)({ acceptedAt })
    yield* Effect.gen(function*() {
      yield* fs.writeFileString(temporary, contents, { mode: 0o600 })
      yield* fs.chmod(temporary, 0o600)
      yield* fs.rename(temporary, file)
    }).pipe(Effect.onError(() => Effect.ignore(fs.remove(temporary))))
  })

  // Counted when accepted, before speaking: a failed or interrupted speech still spends its slot.
  const take = Effect.gen(function*() {
    const now = yield* Clock.currentTimeMillis
    const recent = (yield* load.pipe(Effect.mapError(quotaUnavailable))).filter((at) => now - at < longest)
    for (const { max, window } of NOTIFICATION_LIMITS) {
      if (recent.filter((at) => now - at < Duration.toMillis(window)).length >= max) {
        return yield* new NotificationError({
          message: `Notification limit reached: at most ${max} per ${Duration.format(window)}. Nothing was spoken.`
        })
      }
    }
    yield* save([...recent, now]).pipe(Effect.mapError(quotaUnavailable))
  }).pipe(Semaphore.withPermits(lock, 1))

  return { take }
})

/**
 * Binds the callback tools to the outbox, and to the shared `Speaker` for
 * notices, which play on the output chosen by `AI_VOICE_OUTPUT`.
 */
export const CallbackToolkitHandlers = CallbackToolkit.toLayer(Effect.gen(function*() {
  const outbox = yield* GrokBotOutbox
  const speaker = yield* Speaker
  const { stateDir } = yield* AlexaConfig
  const quota = yield* makeQuota(stateDir)
  return CallbackToolkit.of({
    grokbot_deliver: (input) =>
      Effect.map(outbox.deliver(input), (outcome) => ({ requestId: input.requestId, outcome })),
    // A single attempt: a lost answer does not mean nothing was spoken.
    notify_fernando: ({ event }) =>
      quota.take.pipe(
        Effect.andThen(speaker.speak({ text: notificationTemplates[event] })),
        Effect.as({ event, status: "accepted" as const })
      )
  })
}))

/**
 * Registers the callback tools on whichever `McpServer` transport is provided.
 */
export const layer = McpServer.toolkit(CallbackToolkit).pipe(Layer.provide(CallbackToolkitHandlers))
