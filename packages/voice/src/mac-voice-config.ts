import { Config, Context, Effect, Layer, Option, Path, String } from "effect"

const optional = (name: string) =>
  Config.String(name).pipe(Config.option, Config.map(Option.filter(String.isNonEmpty)))

export const DEFAULT_MAC_VOICE = "Luciana"
/** Relative to HOME. Shared with `~/.local/bin/jarvis-fala`, so both speak one at a time. */
export const DEFAULT_MAC_LOCK = ".local/share/jarvis/fala.lock"

export const DEFAULT_TTS_URL = "http://127.0.0.1:7349"
export const DEFAULT_TTS_VOICE = "pf_dora"

export class MacVoiceConfig extends Context.Service<MacVoiceConfig, {
  /** A voice name accepted by `say -v` (see `say -v '?'`). */
  readonly voice: string
  /** flock(2) file shared with every program that speaks on this Mac through `lockf`. */
  readonly lock: string
  /** Base URL of the local TTS server (Kokoro), tried before `say`. */
  readonly ttsUrl: string
  /** Voice the TTS server synthesizes with. */
  readonly ttsVoice: string
}>()("@jarvis/voice/MacVoiceConfig") {
  /**
   * Reads `AI_VOICE_MAC_VOICE` (default Luciana), `AI_VOICE_MAC_LOCK` (default ~/.local/share/jarvis/fala.lock),
   * `AI_VOICE_TTS_URL` (default http://127.0.0.1:7349) and `AI_VOICE_TTS_VOICE` (default pf_dora).
   */
  static readonly layer = Layer.effect(
    MacVoiceConfig,
    Effect.gen(function*() {
      const path = yield* Path.Path
      const lock = yield* Option.match(yield* optional("AI_VOICE_MAC_LOCK"), {
        onNone: () => Config.String("HOME").pipe(Config.map((home) => path.join(home, DEFAULT_MAC_LOCK))),
        onSome: Config.succeed
      })
      return MacVoiceConfig.of({
        voice: Option.getOrElse(yield* optional("AI_VOICE_MAC_VOICE"), () => DEFAULT_MAC_VOICE),
        lock: path.resolve(lock),
        ttsUrl: Option.getOrElse(yield* optional("AI_VOICE_TTS_URL"), () => DEFAULT_TTS_URL).replace(/\/+$/u, ""),
        ttsVoice: Option.getOrElse(yield* optional("AI_VOICE_TTS_VOICE"), () => DEFAULT_TTS_VOICE)
      })
    })
  )
}
