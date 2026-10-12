import { Schema } from "effect"

export class SessionError extends Schema.TaggedError<SessionError>()("SessionError", {
  message: Schema.String
}) {}

export class DeviceError extends Schema.TaggedError<DeviceError>()("DeviceError", {
  message: Schema.String
}) {}

export class SpeechError extends Schema.TaggedError<SpeechError>()("SpeechError", {
  message: Schema.String
}) {}

export const AlexaError = Schema.Union([SessionError, DeviceError, SpeechError])
export type AlexaError = typeof AlexaError.Type
