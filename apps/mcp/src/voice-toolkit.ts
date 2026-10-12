import { Alexa, AlexaError, Echo, MAX_TEXT_LENGTH, SpeechAccepted } from "@jarvis/alexa"
import { Effect, Schema } from "effect"
import { Tool, Toolkit } from "effect/ai"

// The wire format stays snake_case so existing agents and prompts keep working.
export const EchoDevice = Echo.pipe(Schema.encodeKeys({ isDefault: "is_default" }))

const ListDevices = Tool.make("list_devices", {
  description: "Lists the physical Echo devices on the Amazon account without playing audio.",
  // Stateful protocol adapters omit structured output for non-object results, so agents get the JSON array as text.
  success: Schema.Array(EchoDevice),
  failure: AlexaError
})
  .annotate(Tool.Readonly, true)
  .annotate(Tool.Destructive, false)
  .annotate(Tool.OpenWorld, true)

const SpeakParameters = Schema.Struct({
  deviceSerial: Schema.optionalKey(
    Schema.NonEmptyString.annotate({
      description: "Serial returned by list_devices. Omit to use AI_VOICE_DEFAULT_DEVICE."
    })
  ),
  text: Schema.String.check(Schema.isMinLength(1), Schema.isMaxLength(MAX_TEXT_LENGTH)).annotate({
    description: `Plain text, up to ${MAX_TEXT_LENGTH} UTF-16 code units; no SSML.`
  })
}).pipe(Schema.encodeKeys({ deviceSerial: "device_serial" }))

const makeSpeak = (locale: string) =>
  Tool.make("speak", {
    description: `Speaks text in the Alexa voice on an Echo device, using locale ${locale}. ` +
      "Uses the default Echo when device_serial is omitted. Long text is split into an ordered sequence. " +
      "Use when the user asks for speech; do not retry automatically after an error or timeout.",
    parameters: SpeakParameters,
    success: SpeechAccepted.mapFields((fields) => ({ ...fields, chunks: Schema.Int })).pipe(
      Schema.encodeKeys({ playbackConfirmed: "playback_confirmed" })
    ),
    failure: AlexaError
  })
    .annotate(Tool.Readonly, false)
    .annotate(Tool.Destructive, false)
    .annotate(Tool.Idempotent, false)
    .annotate(Tool.OpenWorld, true)

/**
 * The voice tools, with descriptions that name the configured speech locale.
 */
export const makeVoiceToolkit = (locale: string) => Toolkit.make(ListDevices, makeSpeak(locale))

export type VoiceToolkit = ReturnType<typeof makeVoiceToolkit>

/**
 * Binds the voice tools to the `Alexa` service.
 */
export const voiceHandlers = (toolkit: VoiceToolkit) =>
  toolkit.toLayer(Effect.gen(function*() {
    const alexa = yield* Alexa
    return toolkit.of({
      list_devices: () => alexa.devices,
      speak: (input) => alexa.speak(input)
    })
  }))
