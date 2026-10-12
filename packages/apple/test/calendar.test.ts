import { assert, describe, it } from "@effect/vitest"
import { Effect, Layer, Ref } from "effect"
import { TestClock } from "effect/testing"
import { AutomationError } from "../src/apple-error.ts"
import { Calendar } from "../src/calendar.ts"
import { FIELD_SEPARATOR as US, Osascript, RECORD_SEPARATOR as RS } from "../src/osascript.ts"

// Pin a zone with DST so recurrence expansion is checked across offset changes.
// Every other assertion is computed from local `Date`s, so it follows this zone.
process.env.TZ = "America/New_York"

interface Call {
  readonly script: string
  readonly args: ReadonlyArray<string>
}

const makeFake = (respond: (call: Call) => string | AutomationError) =>
  Effect.gen(function*() {
    const calls = yield* Ref.make<ReadonlyArray<Call>>([])
    const osascript = Osascript.of({
      run: (script, args = []) => {
        const call = { script, args }
        return Ref.update(calls, (all) => [...all, call]).pipe(
          Effect.andThen(() => {
            const result = respond(call)
            return typeof result === "string" ? Effect.succeed(result) : Effect.fail(result)
          })
        )
      },
      open: () => Effect.void
    })
    return { calls, layer: Calendar.layer.pipe(Layer.provide(Layer.succeed(Osascript, osascript))) }
  })

const row = (
  fields: {
    readonly id: string
    readonly title: string
    readonly calendar?: string
    readonly start: readonly [number, number, number, number, number, number]
    readonly end: readonly [number, number, number, number, number, number]
    readonly allDay?: boolean
    readonly location?: string
    readonly recurrence?: string
    readonly excluded?: string
  }
) =>
  [
    fields.id,
    fields.title,
    fields.calendar ?? "Work",
    ...fields.start,
    ...fields.end,
    String(fields.allDay ?? false),
    fields.location ?? "",
    fields.recurrence ?? "",
    fields.excluded ?? ""
  ].join(US) + RS

const localParts = (date: Date) => [
  date.getFullYear(),
  date.getMonth() + 1,
  date.getDate(),
  date.getHours(),
  date.getMinutes(),
  date.getSeconds()
].map(String)

const FROM = "2026-10-11T00:00:00.000Z"
const TO = "2026-10-12T00:00:00.000Z"

describe("Calendar", () => {
  it.effect("lists calendar names and uses a constant script", () =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() => `Work${RS}Home${RS}`)
      const names = yield* Calendar.use((calendar) => calendar.calendars).pipe(Effect.provide(fake.layer))
      assert.deepStrictEqual(names, ["Work", "Home"])
      const [call] = yield* Ref.get(fake.calls)
      assert.deepStrictEqual(call?.args, [])
      assert.include(call?.script, "on run argv")
    }))

  it.effect("passes local date parts and the calendar name only through argv", () =>
    Effect.gen(function*() {
      const hostile = `Work" & (do shell script "rm -rf ~") & "`
      const fake = yield* makeFake(() => "")
      const events = yield* Calendar.use((calendar) => calendar.events({ from: FROM, to: TO, calendar: hostile })).pipe(
        Effect.provide(fake.layer)
      )
      assert.deepStrictEqual(events, [])
      const [call] = yield* Ref.get(fake.calls)
      assert.notInclude(call?.script, "rm -rf")
      assert.notInclude(call?.script, hostile)
      assert.include(call?.script, "on run argv")
      assert.deepStrictEqual(call?.args, [
        hostile,
        ...localParts(new Date(FROM)),
        ...localParts(new Date(TO))
      ])
    }))

  it.effect("uses the same script for every request and an empty calendar filter by default", () =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() => "")
      yield* Calendar.use((calendar) =>
        Effect.all([
          calendar.events({ from: FROM, to: TO }),
          calendar.events({ from: FROM, to: TO, calendar: "Home" })
        ])
      ).pipe(Effect.provide(fake.layer))
      const [first, second] = yield* Ref.get(fake.calls)
      assert.strictEqual(first?.script, second?.script)
      assert.strictEqual(first?.args[0], "")
      assert.strictEqual(second?.args[0], "Home")
    }))

  it.effect("parses records into ISO events sorted by start", () =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() =>
        row({ id: "b", title: "Lunch", start: [2026, 10, 11, 12, 0, 0], end: [2026, 10, 11, 13, 0, 0], location: "Cafe" })
        + row({ id: "c", title: "Holiday", start: [2026, 10, 11, 0, 0, 0], end: [2026, 10, 12, 0, 0, 0], allDay: true })
        + row({ id: "a", title: "Standup", start: [2026, 10, 11, 9, 30, 0], end: [2026, 10, 11, 9, 45, 0] })
      )
      const events = yield* Calendar.use((calendar) => calendar.events({ from: FROM, to: TO })).pipe(
        Effect.provide(fake.layer)
      )
      assert.deepStrictEqual(events.map((event) => event.id), ["c", "a", "b"])
      assert.strictEqual(events[0]?.allDay, true)
      assert.isUndefined(events[1]?.location)
      assert.strictEqual(events[2]?.location, "Cafe")
      const offsetIso = /^2026-10-11T09:30:00\.000(Z|[+-]\d{2}:\d{2})$/
      assert.match(events[1]?.start ?? "", offsetIso)
    }))

  it.effect("keeps titles that contain punctuation intact", () =>
    Effect.gen(function*() {
      const title = `Dentist "Dr. O'Neil" \\ ; $(whoami)`
      const fake = yield* makeFake(() => row({ id: "x", title, start: [2026, 10, 11, 8, 0, 0], end: [2026, 10, 11, 9, 0, 0] }))
      const events = yield* Calendar.use((calendar) => calendar.events({ from: FROM, to: TO })).pipe(
        Effect.provide(fake.layer)
      )
      assert.strictEqual(events[0]?.title, title)
    }))

  it.effect.each([
    { name: "reversed range", from: TO, to: FROM },
    { name: "empty range", from: FROM, to: FROM },
    { name: "span over 31 days", from: FROM, to: "2026-11-12T00:00:00.000Z" },
    { name: "invalid from", from: "not a date", to: TO },
    { name: "invalid to", from: FROM, to: "tomorrow-ish" }
  ])("rejects $name without running a script", ({ from, to }) =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() => "")
      const error = yield* Calendar.use((calendar) => calendar.events({ from, to })).pipe(
        Effect.provide(fake.layer),
        Effect.flip
      )
      assert.instanceOf(error, AutomationError)
      assert.lengthOf(yield* Ref.get(fake.calls), 0)
    }))

  it.effect("accepts exactly 31 days", () =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() => "")
      yield* Calendar.use((calendar) => calendar.events({ from: FROM, to: "2026-11-11T00:00:00.000Z" })).pipe(
        Effect.provide(fake.layer)
      )
      assert.lengthOf(yield* Ref.get(fake.calls), 1)
    }))

  it.effect("rejects an empty calendar name", () =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() => "")
      const error = yield* Calendar.use((calendar) => calendar.events({ from: FROM, to: TO, calendar: "  " })).pipe(
        Effect.provide(fake.layer),
        Effect.flip
      )
      assert.instanceOf(error, AutomationError)
      assert.lengthOf(yield* Ref.get(fake.calls), 0)
    }))

  it.effect("fails with AutomationError on malformed output", () =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() => `only${US}three${US}fields${RS}`)
      const error = yield* Calendar.use((calendar) => calendar.events({ from: FROM, to: TO })).pipe(
        Effect.provide(fake.layer),
        Effect.flip
      )
      assert.instanceOf(error, AutomationError)
    }))

  it.effect("propagates osascript failures", () =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() => new AutomationError({ message: "denied" }))
      const error = yield* Calendar.use((calendar) => calendar.events({ from: FROM, to: TO })).pipe(
        Effect.provide(fake.layer),
        Effect.flip
      )
      assert.strictEqual(error.message, "denied")
    }))

  it.effect("today queries the local day of the test clock", () =>
    Effect.gen(function*() {
      const now = new Date(2026, 9, 11, 15, 20, 0)
      yield* TestClock.setTime(now.getTime())
      const fake = yield* makeFake(() => "")
      yield* Calendar.use((calendar) => calendar.today).pipe(Effect.provide(fake.layer))
      const [call] = yield* Ref.get(fake.calls)
      assert.deepStrictEqual(call?.args, [
        "",
        ...localParts(new Date(2026, 9, 11, 0, 0, 0)),
        ...localParts(new Date(2026, 9, 12, 0, 0, 0))
      ])
    }))

  it.effect("today moves to the next day when the clock crosses local midnight", () =>
    Effect.gen(function*() {
      yield* TestClock.setTime(new Date(2026, 9, 11, 23, 59, 59).getTime())
      const fake = yield* makeFake(() => "")
      const run = Calendar.use((calendar) => calendar.today).pipe(Effect.provide(fake.layer))
      yield* run
      yield* TestClock.adjust(2_000)
      yield* run
      const calls = yield* Ref.get(fake.calls)
      assert.strictEqual(calls[0]?.args[3], "11")
      assert.strictEqual(calls[1]?.args[3], "12")
    }))

  describe("recurring events", () => {
    const run = (output: string, from: string, to: string) =>
      Effect.gen(function*() {
        const fake = yield* makeFake(() => output)
        return yield* Calendar.use((calendar) => calendar.events({ from, to })).pipe(Effect.provide(fake.layer))
      })

    // Monday 2026-01-05 10:00-11:00 EST, weekly on Mondays.
    const weekly = (extra: { readonly recurrence?: string; readonly excluded?: string } = {}) =>
      row({
        id: "standup",
        title: "Weekly",
        start: [2026, 1, 5, 10, 0, 0],
        end: [2026, 1, 5, 11, 0, 0],
        recurrence: "FREQ=WEEKLY;INTERVAL=1;BYDAY=MO",
        ...extra
      })
    const JULY_6 = "2026-07-06T00:00:00-04:00"
    const JULY_7 = "2026-07-07T00:00:00-04:00"

    it.effect("expands a master created months earlier into the queried day, keeping wall-clock time across DST", () =>
      Effect.gen(function*() {
        const events = yield* run(weekly(), JULY_6, JULY_7)
        assert.deepStrictEqual(events.map(({ end, id, start }) => ({ id, start, end })), [
          { id: "standup", start: "2026-07-06T10:00:00.000-04:00", end: "2026-07-06T11:00:00.000-04:00" }
        ])
      }))

    it.effect("drops excluded dates", () =>
      Effect.gen(function*() {
        assert.lengthOf(yield* run(weekly({ excluded: "2026-6-29,2026-7-6" }), JULY_6, JULY_7), 0)
      }))

    it.effect.each([
      { name: "COUNT already used up", recurrence: "FREQ=WEEKLY;BYDAY=MO;COUNT=3", expected: 0 },
      { name: "UNTIL in the past", recurrence: "FREQ=WEEKLY;BYDAY=MO;UNTIL=20260301T045959Z", expected: 0 },
      { name: "date-only UNTIL on the day (inclusive)", recurrence: "FREQ=WEEKLY;BYDAY=MO;UNTIL=20260706", expected: 1 },
    ])("honours $name", ({ expected, recurrence }) =>
      Effect.gen(function*() {
        const output = weekly({ recurrence })
        assert.lengthOf(yield* run(output, JULY_6, JULY_7), expected)
      }))

    it.effect("INTERVAL=2 includes every other week", () =>
      Effect.gen(function*() {
        const output = row({
          id: "x",
          title: "Biweekly",
          start: [2026, 1, 5, 10, 0, 0],
          end: [2026, 1, 5, 11, 0, 0],
          recurrence: "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO"
        })
        const events = yield* run(output, "2026-07-01T00:00:00-04:00", "2026-07-31T00:00:00-04:00")
        assert.deepStrictEqual(events.map((event) => event.start.slice(0, 10)), ["2026-07-06", "2026-07-20"])
      }))

    it.effect("expands monthly last-Friday and last-weekday rules", () =>
      Effect.gen(function*() {
        const output = row({
          id: "a",
          title: "Last Friday",
          start: [2026, 1, 30, 9, 0, 0],
          end: [2026, 1, 30, 9, 30, 0],
          recurrence: "FREQ=MONTHLY;INTERVAL=1;BYDAY=-1FR"
        }) + row({
          id: "b",
          title: "Last weekday",
          start: [2026, 1, 30, 17, 0, 0],
          end: [2026, 1, 30, 17, 30, 0],
          recurrence: "FREQ=MONTHLY;BYSETPOS=-1;BYDAY=MO,TU,WE,TH,FR"
        }) + row({
          id: "c",
          title: "On the 31st",
          start: [2026, 1, 31, 8, 0, 0],
          end: [2026, 1, 31, 8, 30, 0],
          recurrence: "FREQ=MONTHLY"
        })
        const september = yield* run(output, "2026-09-01T00:00:00-04:00", "2026-10-01T00:00:00-04:00")
        assert.deepStrictEqual(september.map(({ id, start }) => `${id} ${start}`), [
          "a 2026-09-25T09:00:00.000-04:00",
          "b 2026-09-30T17:00:00.000-04:00"
        ])
        const october = yield* run(output, "2026-10-01T00:00:00-04:00", "2026-11-01T00:00:00-04:00")
        assert.deepStrictEqual(october.map(({ id, start }) => `${id} ${start}`), [
          "a 2026-10-30T09:00:00.000-04:00",
          "b 2026-10-30T17:00:00.000-04:00",
          "c 2026-10-31T08:00:00.000-04:00"
        ])
      }))

    it.effect("expands a yearly all-day birthday whose master is centuries old", () =>
      Effect.gen(function*() {
        const output = row({
          id: "bday",
          title: "Birthday",
          calendar: "Birthdays",
          start: [1604, 10, 11, 0, 0, 0],
          end: [1604, 10, 12, 0, 0, 0],
          allDay: true,
          recurrence: "FREQ=YEARLY;INTERVAL=1"
        })
        const events = yield* run(output, "2026-10-11T00:00:00-04:00", "2026-10-12T00:00:00-04:00")
        assert.deepStrictEqual(events.map(({ allDay, end, start }) => ({ allDay, start, end })), [
          { allDay: true, start: "2026-10-11T00:00:00.000-04:00", end: "2026-10-12T00:00:00.000-04:00" }
        ])
      }))

    it.effect("keeps 09:00 local on both sides of the DST change", () =>
      Effect.gen(function*() {
        const output = row({
          id: "d",
          title: "Daily",
          start: [2026, 10, 30, 9, 0, 0],
          end: [2026, 10, 30, 9, 15, 0],
          recurrence: "FREQ=DAILY"
        })
        const events = yield* run(output, "2026-10-31T00:00:00-04:00", "2026-11-03T00:00:00-05:00")
        assert.deepStrictEqual(events.map((event) => event.start), [
          "2026-10-31T09:00:00.000-04:00",
          "2026-11-01T09:00:00.000-05:00",
          "2026-11-02T09:00:00.000-05:00"
        ])
      }))

    it.effect("an edited (detached) occurrence replaces the generated one on its day", () =>
      Effect.gen(function*() {
        const output = weekly() + row({
          id: "standup",
          title: "Weekly (moved)",
          start: [2026, 7, 6, 14, 0, 0],
          end: [2026, 7, 6, 15, 0, 0]
        })
        const events = yield* run(output, JULY_6, JULY_7)
        assert.deepStrictEqual(events.map(({ start, title }) => `${title} ${start}`), [
          "Weekly (moved) 2026-07-06T14:00:00.000-04:00"
        ])
      }))

    it.effect("the master's own first occurrence is reported once", () =>
      Effect.gen(function*() {
        const events = yield* run(weekly(), "2026-01-05T00:00:00-05:00", "2026-01-13T00:00:00-05:00")
        assert.deepStrictEqual(events.map((event) => event.start), [
          "2026-01-05T10:00:00.000-05:00",
          "2026-01-12T10:00:00.000-05:00"
        ])
      }))

    it.effect("an unsupported rule falls back to the master and drops it outside the range", () =>
      Effect.gen(function*() {
        const hourlyOld = row({
          id: "h",
          title: "Hourly",
          start: [2026, 1, 5, 10, 0, 0],
          end: [2026, 1, 5, 10, 15, 0],
          recurrence: "FREQ=HOURLY"
        })
        const hourlyToday = row({
          id: "h2",
          title: "Hourly today",
          start: [2026, 7, 6, 10, 0, 0],
          end: [2026, 7, 6, 10, 15, 0],
          recurrence: "FREQ=WEEKLY;BYHOUR=10,12"
        })
        const events = yield* run(hourlyOld + hourlyToday, JULY_6, JULY_7)
        assert.deepStrictEqual(events.map((event) => event.id), ["h2"])
      }))
  })

  it.effect("both scripts strip the record/field separators from Calendar text before emitting it", () =>
    Effect.gen(function*() {
      const fake = yield* makeFake(() => "")
      yield* Calendar.use((calendar) => Effect.all([calendar.calendars, calendar.events({ from: FROM, to: TO })])).pipe(
        Effect.provide(fake.layer)
      )
      const [names, events] = yield* Ref.get(fake.calls)
      assert.include(names?.script, "AppleScript's text item delimiters to {RS, US}")
      assert.include(names?.script, "my clean(contents of n)")
      for (const field of ["uid of e", "summary of e", "calName", "location of e", "r"]) {
        assert.include(events?.script, `my clean(${field})`)
      }
    }))
})
