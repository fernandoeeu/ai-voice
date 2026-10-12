import { Array as Arr, Context, DateTime, Effect, Layer, Option, Order, Schema } from "effect"
import { AutomationError } from "./apple-error.ts"
import { Osascript, parseRecords } from "./osascript.ts"

export const CalendarEvent = Schema.Struct({
  id: Schema.String,
  title: Schema.String,
  calendar: Schema.String,
  start: Schema.String,
  end: Schema.String,
  allDay: Schema.Boolean,
  location: Schema.optional(Schema.String)
})
export type CalendarEvent = typeof CalendarEvent.Type

const MAX_SPAN_DAYS = 31
const MAX_SPAN_MILLIS = MAX_SPAN_DAYS * 24 * 60 * 60 * 1000

const CalendarName = Schema.Trim.check(Schema.isNonEmpty(), Schema.isMaxLength(200))
const EventsInput = Schema.Struct({
  from: Schema.String,
  to: Schema.String,
  calendar: Schema.optional(CalendarName)
})

// id, title, calendar, start y/m/d/h/mi/s, end y/m/d/h/mi/s, allDay, location,
// recurrence (RRULE text or ""), excluded dates ("y-m-d,y-m-d" or "")
const Part = Schema.NumberFromString.check(Schema.isInt())
const Row = Schema.Tuple([
  Schema.String,
  Schema.String,
  Schema.String,
  Part, Part, Part, Part, Part, Part,
  Part, Part, Part, Part, Part, Part,
  Schema.Literals(["true", "false"]),
  Schema.String,
  Schema.String,
  Schema.String
])

const decodeInput = Schema.decodeUnknownEffect(EventsInput)
const decodeRow = Schema.decodeUnknownEffect(Row)

const invalid = (what: string) => (error: { readonly message: string }) =>
  new AutomationError({ message: `${what}: ${error.message}` })

// Text coming from Calendar (names, titles, locations, invitations from other
// people) may contain the record/field separators; they are replaced before
// being emitted so a hostile title cannot break or inject records.
const CLEAN_HANDLER = `
on clean(t)
  if t is missing value then return ""
  set t to t as text
  set saved to AppleScript's text item delimiters
  set AppleScript's text item delimiters to {RS, US}
  set pieces to text items of t
  set AppleScript's text item delimiters to " "
  set t to pieces as text
  set AppleScript's text item delimiters to saved
  return t
end clean
`

const CALENDARS_SCRIPT = `
property RS : ASCII character 30
property US : ASCII character 31
${CLEAN_HANDLER}
on run argv
  set out to ""
  tell application "Calendar"
    set calNames to name of every calendar
  end tell
  repeat with n in calNames
    set out to out & my clean(contents of n) & RS
  end repeat
  return out
end run
`

// Dates cross the boundary as numeric parts only. `day` is pinned to 1 before
// `month` is set, otherwise "31" + "February" overflows into March.
//
// Calendar's scripting model exposes a recurring series only as its master
// event (with the first occurrence's dates and an RRULE in `recurrence`), so a
// weekly meeting created months ago never matches a date filter on today. The
// script therefore also returns every recurring master that started before the
// range ends, and the occurrences are expanded on the TypeScript side.
// Properties are fetched in bulk (one Apple event per property per calendar)
// instead of per event, then only the needed events are read one by one.
const EVENTS_SCRIPT = `
property RS : ASCII character 30
property US : ASCII character 31
${CLEAN_HANDLER}
on makeDate(y, mo, d, h, mi, s)
  set theDate to current date
  set day of theDate to 1
  set year of theDate to (y as integer)
  set month of theDate to (mo as integer)
  set day of theDate to (d as integer)
  set time of theDate to ((h as integer) * 3600 + (mi as integer) * 60 + (s as integer))
  return theDate
end makeDate

on formatParts(d)
  return ((year of d) as text) & US & ((month of d as integer) as text) & US & ((day of d) as text) & US & ((hours of d) as text) & US & ((minutes of d) as text) & US & ((seconds of d) as text) & US
end formatParts

on joinDays(ds)
  if ds is missing value then return ""
  set out to ""
  repeat with d in ds
    set d to contents of d
    if class of d is date then
      if out is not "" then set out to out & ","
      set out to out & ((year of d) as text) & "-" & ((month of d as integer) as text) & "-" & ((day of d) as text)
    end if
  end repeat
  return out
end joinDays

on run argv
  set wanted to item 1 of argv
  set fromDate to my makeDate(item 2 of argv, item 3 of argv, item 4 of argv, item 5 of argv, item 6 of argv, item 7 of argv)
  set toDate to my makeDate(item 8 of argv, item 9 of argv, item 10 of argv, item 11 of argv, item 12 of argv, item 13 of argv)
  set out to ""
  set found to (wanted is "")
  tell application "Calendar"
    set cals to every calendar
    set calNames to name of every calendar
    if (count of cals) is not (count of calNames) then error "Calendar changed during the query; try again."
    repeat with k from 1 to count of cals
      set calName to item k of calNames
      if wanted is "" or calName is wanted then
        set found to true
        set c to item k of cals
        set evs to every event of c whose start date < toDate
        set recs to recurrence of every event of c whose start date < toDate
        set ends to end date of every event of c whose start date < toDate
        if (count of evs) is not (count of recs) or (count of evs) is not (count of ends) then error "Calendar changed during the query; try again."
        repeat with i from 1 to count of evs
          set r to item i of recs
          if r is missing value then set r to ""
          if (item i of ends) > fromDate or r is not "" then
            set e to item i of evs
            set exdates to ""
            if r is not "" then set exdates to my joinDays(excluded dates of e)
            set out to out & my clean(uid of e) & US & my clean(summary of e) & US & my clean(calName) & US & my formatParts(start date of e) & my formatParts(item i of ends) & ((allday event of e) as text) & US & my clean(location of e) & US & my clean(r) & US & exdates & RS
          end if
        end repeat
      end if
    end repeat
  end tell
  if not found then error "Calendar not found: " & wanted
  return out
end run
`

// --- civil-date arithmetic (day numbers since the epoch, no time zone) -------

const DAY_MILLIS = 24 * 60 * 60 * 1000
const dayOf = (year: number, month: number, day: number) => Math.floor(Date.UTC(year, month - 1, day) / DAY_MILLIS)
const civilOf = (dayNumber: number) => {
  const date = new Date(dayNumber * DAY_MILLIS)
  return { year: date.getUTCFullYear(), month: date.getUTCMonth() + 1, day: date.getUTCDate() }
}
const weekdayOf = (dayNumber: number) => new Date(dayNumber * DAY_MILLIS).getUTCDay()
const daysInMonth = (year: number, month: number) => new Date(Date.UTC(year, month, 0)).getUTCDate()

type Parts = readonly [number, number, number, number, number, number]

const localMillis = (zone: DateTime.TimeZone, [year, month, day, hour, minute, second]: Parts) =>
  Option.map(
    DateTime.makeZoned({ year, month, day, hour, minute, second }, { timeZone: zone, adjustForTimeZone: true }),
    DateTime.toEpochMillis
  )

const atDay = (dayNumber: number, [, , , hour, minute, second]: Parts): Parts => {
  const { year, month, day } = civilOf(dayNumber)
  return [year, month, day, hour, minute, second]
}

// --- RRULE (RFC 5545) subset ---------------------------------------------------

type Freq = "DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY"

interface WeekdaySpec {
  readonly ordinal: number | undefined
  readonly weekday: number
}

type Until =
  | { readonly _tag: "Instant"; readonly millis: number }
  | { readonly _tag: "Local"; readonly parts: Parts }
  | { readonly _tag: "Date"; readonly day: number }

interface Rule {
  readonly freq: Freq
  readonly interval: number
  readonly count: number | undefined
  readonly until: Until | undefined
  readonly byDay: ReadonlyArray<WeekdaySpec>
  readonly byMonthDay: ReadonlyArray<number>
  readonly byMonth: ReadonlyArray<number>
  readonly bySetPos: ReadonlyArray<number>
  readonly weekStart: number
}

const WEEKDAYS: Record<string, number> = { SU: 0, MO: 1, TU: 2, WE: 3, TH: 4, FR: 5, SA: 6 }
const RULE_KEYS = new Set(["FREQ", "INTERVAL", "COUNT", "UNTIL", "BYDAY", "BYMONTHDAY", "BYMONTH", "BYSETPOS", "WKST"])
const FREQS = new Set<string>(["DAILY", "WEEKLY", "MONTHLY", "YEARLY"])

const parseInteger = (text: string, min: number, max: number, allowNegative: boolean): number | undefined => {
  if (!/^[+-]?\d{1,4}$/.test(text)) return undefined
  const value = Number(text)
  const magnitude = Math.abs(value)
  if (magnitude < min || magnitude > max || (value < 0 && !allowNegative)) return undefined
  return value
}

const parseList = (text: string | undefined, min: number, max: number, allowNegative: boolean) => {
  if (text === undefined) return []
  const values = text.split(",").map((item) => parseInteger(item, min, max, allowNegative))
  return values.every((value) => value !== undefined) ? values as Array<number> : undefined
}

const parseUntil = (text: string): Until | undefined => {
  const match = /^(\d{4})(\d{2})(\d{2})(?:T(\d{2})(\d{2})(\d{2})(Z)?)?$/.exec(text)
  if (match === null) return undefined
  const [year, month, day, hour, minute, second] = match.slice(1, 7).map((part) => Number(part ?? "0")) as unknown as Parts
  if (month < 1 || month > 12 || day < 1 || day > daysInMonth(year, month)) return undefined
  if (match[4] === undefined) return { _tag: "Date", day: dayOf(year, month, day) }
  if (match[7] === "Z") return { _tag: "Instant", millis: Date.UTC(year, month - 1, day, hour, minute, second) }
  return { _tag: "Local", parts: [year, month, day, hour, minute, second] }
}

/** Parses the RRULE subset Calendar produces; anything else yields `undefined`. */
const parseRule = (text: string): Rule | undefined => {
  const body = text.trim().replace(/^RRULE:/i, "")
  if (body.length === 0 || /[\r\n]/.test(body)) return undefined
  const fields = new Map<string, string>()
  for (const piece of body.split(";")) {
    if (piece.trim().length === 0) continue
    const eq = piece.indexOf("=")
    if (eq <= 0) return undefined
    const key = piece.slice(0, eq).trim().toUpperCase()
    if (!RULE_KEYS.has(key) || fields.has(key)) return undefined
    fields.set(key, piece.slice(eq + 1).trim().toUpperCase())
  }
  const freq = fields.get("FREQ")
  if (freq === undefined || !FREQS.has(freq)) return undefined
  const interval = fields.has("INTERVAL") ? parseInteger(fields.get("INTERVAL")!, 1, 1000, false) : 1
  const count = fields.has("COUNT") ? parseInteger(fields.get("COUNT")!, 1, 9999, false) : undefined
  const until = fields.has("UNTIL") ? parseUntil(fields.get("UNTIL")!) : undefined
  const byMonthDay = parseList(fields.get("BYMONTHDAY"), 1, 31, true)
  const byMonth = parseList(fields.get("BYMONTH"), 1, 12, false)
  const bySetPos = parseList(fields.get("BYSETPOS"), 1, 366, true)
  const weekStart = WEEKDAYS[fields.get("WKST") ?? "MO"]
  const byDay: Array<WeekdaySpec> = []
  for (const item of fields.get("BYDAY")?.split(",") ?? []) {
    const match = /^([+-]?\d{1,2})?(SU|MO|TU|WE|TH|FR|SA)$/.exec(item)
    if (match === null) return undefined
    const ordinal = match[1] === undefined ? undefined : parseInteger(match[1], 1, 53, true)
    if (match[1] !== undefined && ordinal === undefined) return undefined
    byDay.push({ ordinal, weekday: WEEKDAYS[match[2]!]! })
  }
  if (
    interval === undefined || (fields.has("COUNT") && count === undefined) ||
    (fields.has("UNTIL") && until === undefined) || (fields.has("COUNT") && fields.has("UNTIL")) ||
    byMonthDay === undefined || byMonth === undefined || bySetPos === undefined || weekStart === undefined
  ) {
    return undefined
  }
  // Ordinal weekdays only make sense inside a month or a year.
  if ((freq === "DAILY" || freq === "WEEKLY") && byDay.some((spec) => spec.ordinal !== undefined)) return undefined
  return { freq: freq as Freq, interval, count, until, byDay, byMonthDay, byMonth, bySetPos, weekStart }
}

const matchingWeekdays = (first: number, last: number, spec: WeekdaySpec): Array<number> => {
  const all: Array<number> = []
  for (let day = first; day <= last; day++) {
    if (weekdayOf(day) === spec.weekday) all.push(day)
  }
  if (spec.ordinal === undefined) return all
  const pick = spec.ordinal > 0 ? all[spec.ordinal - 1] : all[all.length + spec.ordinal]
  return pick === undefined ? [] : [pick]
}

const monthDays = (rule: Rule, year: number, month: number, startDayOfMonth: number): Array<number> => {
  const length = daysInMonth(year, month)
  if (rule.byMonthDay.length > 0) {
    return rule.byMonthDay
      .map((value) => value > 0 ? value : length + 1 + value)
      .filter((value) => value >= 1 && value <= length)
      .map((value) => dayOf(year, month, value))
      .filter((day) => rule.byDay.length === 0 || rule.byDay.some((spec) => spec.weekday === weekdayOf(day)))
  }
  if (rule.byDay.length > 0) {
    return rule.byDay.flatMap((spec) => matchingWeekdays(dayOf(year, month, 1), dayOf(year, month, length), spec))
  }
  return startDayOfMonth <= length ? [dayOf(year, month, startDayOfMonth)] : []
}

const applySetPos = (rule: Rule, days: ReadonlyArray<number>): Array<number> => {
  const sorted = Array.from(new Set(days)).sort((a, b) => a - b)
  if (rule.bySetPos.length === 0) return sorted
  const picked = rule.bySetPos
    .map((position) => position > 0 ? sorted[position - 1] : sorted[sorted.length + position])
    .filter((day): day is number => day !== undefined)
  return Array.from(new Set(picked)).sort((a, b) => a - b)
}

interface Period {
  readonly first: number
  readonly last: number
  readonly days: () => Array<number>
}

const periodAt = (rule: Rule, startDay: number, index: number): Period => {
  const start = civilOf(startDay)
  const step = index * rule.interval
  switch (rule.freq) {
    case "DAILY": {
      const day = startDay + step
      const limit = () => {
        const { month, year } = civilOf(day)
        const length = daysInMonth(year, month)
        const dayOfMonth = civilOf(day).day
        const keep = (rule.byMonth.length === 0 || rule.byMonth.includes(month)) &&
          (rule.byMonthDay.length === 0 ||
            rule.byMonthDay.some((value) => (value > 0 ? value : length + 1 + value) === dayOfMonth)) &&
          (rule.byDay.length === 0 || rule.byDay.some((spec) => spec.weekday === weekdayOf(day)))
        return keep ? [day] : []
      }
      return { first: day, last: day, days: limit }
    }
    case "WEEKLY": {
      const weekFirst = startDay - ((weekdayOf(startDay) - rule.weekStart + 7) % 7) + 7 * step
      const weekdays = rule.byDay.length > 0 ? rule.byDay.map((spec) => spec.weekday) : [weekdayOf(startDay)]
      return {
        first: weekFirst,
        last: weekFirst + 6,
        days: () =>
          applySetPos(
            rule,
            weekdays
              .map((weekday) => weekFirst + ((weekday - rule.weekStart + 7) % 7))
              .filter((day) => rule.byMonth.length === 0 || rule.byMonth.includes(civilOf(day).month))
          )
      }
    }
    case "MONTHLY": {
      const total = start.year * 12 + (start.month - 1) + step
      const year = Math.floor(total / 12)
      const month = (total % 12) + 1
      return {
        first: dayOf(year, month, 1),
        last: dayOf(year, month, daysInMonth(year, month)),
        days: () =>
          rule.byMonth.length > 0 && !rule.byMonth.includes(month)
            ? []
            : applySetPos(rule, monthDays(rule, year, month, start.day))
      }
    }
    case "YEARLY": {
      const year = start.year + step
      const days = () => {
        if (rule.byDay.length > 0 && rule.byMonth.length === 0 && rule.byMonthDay.length === 0) {
          const first = dayOf(year, 1, 1)
          const last = dayOf(year, 12, 31)
          return applySetPos(rule, rule.byDay.flatMap((spec) => matchingWeekdays(first, last, spec)))
        }
        const months = rule.byMonth.length > 0
          ? rule.byMonth
          : rule.byMonthDay.length > 0
          ? [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
          : [start.month]
        return applySetPos(rule, months.flatMap((month) => monthDays(rule, year, month, start.day)))
      }
      return { first: dayOf(year, 1, 1), last: dayOf(year, 12, 31), days }
    }
  }
}

// A daily rule that began in 1970 still only needs ~20k cheap iterations.
const MAX_PERIODS = 100_000

interface Interval {
  readonly start: number
  readonly end: number
}

interface RawEvent {
  readonly id: string
  readonly title: string
  readonly calendar: string
  readonly start: Parts
  readonly end: Parts
  readonly allDay: boolean
  readonly location: string
  readonly recurrence: string
  readonly excludedDays: ReadonlySet<number>
}

/**
 * Expands a recurring master into the occurrences overlapping `[from, to)`.
 * Occurrences keep the master's local wall-clock time (DST-safe), all-day
 * events keep their length in days, `COUNT` includes excluded dates (RFC 5545).
 */
const expand = (
  zone: DateTime.TimeZone,
  event: RawEvent,
  rule: Rule,
  masterStart: number,
  masterEnd: number,
  from: number,
  to: number,
  skipDays: ReadonlySet<number>
): Array<Interval> => {
  const startDay = dayOf(event.start[0], event.start[1], event.start[2])
  const spanDays = Math.max(0, dayOf(event.end[0], event.end[1], event.end[2]) - startDay)
  // Day numbers are UTC-based; one day of slack covers any local offset.
  const fromDay = Math.floor(from / DAY_MILLIS) - 1
  const toDay = Math.floor(to / DAY_MILLIS) + 1
  // `until` is inclusive; a DATE-only UNTIL covers its whole local day, so it
  // becomes an exclusive bound at the next local midnight.
  const untilInclusive = rule.until?._tag !== "Date"
  const until = rule.until === undefined
    ? undefined
    : rule.until._tag === "Instant"
    ? rule.until.millis
    : rule.until._tag === "Local"
    ? Option.getOrUndefined(localMillis(zone, rule.until.parts))
    : Option.getOrUndefined(localMillis(zone, atDay(rule.until.day + 1, [0, 0, 0, 0, 0, 0])))
  const untilDay = until === undefined ? undefined : Math.floor(until / DAY_MILLIS) + 1

  const occurrences: Array<Interval> = []
  let emitted = 0
  for (let index = 0; index < MAX_PERIODS; index++) {
    const period = periodAt(rule, startDay, index)
    if (period.first > toDay || (untilDay !== undefined && period.first > untilDay)) break
    if (rule.count === undefined && period.last + spanDays < fromDay) continue
    const days = period.days()
    if (index === 0 && !days.includes(startDay)) days.unshift(startDay)
    for (const day of days) {
      if (day < startDay) continue
      const start = day === startDay
        ? masterStart
        : Option.getOrUndefined(localMillis(zone, atDay(day, event.start)))
      if (start === undefined) continue
      if (until !== undefined && (untilInclusive ? start > until : start >= until)) return occurrences
      emitted++
      if (rule.count !== undefined && emitted > rule.count) return occurrences
      if (start >= to) return occurrences
      if (event.excludedDays.has(day) || skipDays.has(day)) continue
      const end = day === startDay
        ? masterEnd
        : event.allDay
        ? Option.getOrUndefined(localMillis(zone, atDay(day + spanDays, event.end)))
        : start + (masterEnd - masterStart)
      if (end !== undefined && end > from) occurrences.push({ start, end })
    }
  }
  return occurrences
}

const parseExcludedDays = (text: string): ReadonlySet<number> =>
  new Set(
    text.split(",").flatMap((item) => {
      const match = /^(\d{1,4})-(\d{1,2})-(\d{1,2})$/.exec(item)
      return match === null ? [] : [dayOf(Number(match[1]), Number(match[2]), Number(match[3]))]
    })
  )

interface Placed {
  readonly event: RawEvent
  readonly start: number
  readonly end: number
}

const place = (zone: DateTime.TimeZone, events: ReadonlyArray<RawEvent>, from: number, to: number) =>
  Effect.gen(function*() {
    const timed: Array<{ readonly event: RawEvent; readonly start: number; readonly end: number }> = []
    for (const event of events) {
      const start = localMillis(zone, event.start)
      const end = localMillis(zone, event.end)
      if (Option.isNone(start) || Option.isNone(end)) {
        return yield* new AutomationError({ message: "Calendar returned an invalid date." })
      }
      timed.push({ event, start: start.value, end: end.value })
    }

    // Group by calendar + uid: the earliest recurring record is the series
    // master; any other record with the same uid is a detached (edited)
    // occurrence, which replaces the generated occurrence on its day.
    const groups = new Map<string, Array<(typeof timed)[number]>>()
    for (const item of timed) {
      const key = `${item.event.calendar}\u0000${item.event.id}`
      groups.set(key, [...(groups.get(key) ?? []), item])
    }

    const placed: Array<Placed> = []
    for (const group of groups.values()) {
      const master = group
        .filter((item) => item.event.recurrence.length > 0)
        .reduce<(typeof timed)[number] | undefined>(
          (best, item) => best === undefined || item.start < best.start ? item : best,
          undefined
        )
      const rule = master === undefined ? undefined : parseRule(master.event.recurrence)
      const others = group.filter((item) => item !== master || rule === undefined)
      for (const item of others) {
        if (item.start < to && item.end > from) placed.push(item)
      }
      if (master !== undefined && rule !== undefined) {
        const skipDays = new Set(
          others.map((item) => dayOf(item.event.start[0], item.event.start[1], item.event.start[2]))
        )
        for (const occurrence of expand(zone, master.event, rule, master.start, master.end, from, to, skipDays)) {
          placed.push({ event: master.event, ...occurrence })
        }
      }
    }
    return placed
  })

const byStartThenTitle = Order.combine(
  Order.mapInput(Order.Number, (item: Placed) => item.start),
  Order.mapInput(Order.String, (item: Placed) => item.event.title)
)

const parseInstant = (label: string, value: string) =>
  Option.match(DateTime.make(value), {
    onNone: () => Effect.fail(new AutomationError({ message: `${label} is not a valid ISO 8601 date: ${value}` })),
    onSome: (dateTime) => Effect.succeed(DateTime.toUtc(dateTime))
  })

const localParts = (zone: DateTime.TimeZone, instant: DateTime.Utc): ReadonlyArray<string> => {
  const parts = DateTime.toParts(DateTime.setZone(instant, zone))
  return [parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second].map(String)
}

export class Calendar extends Context.Service<Calendar, {
  readonly calendars: Effect.Effect<ReadonlyArray<string>, AutomationError>
  readonly events: (input: {
    readonly from: string
    readonly to: string
    readonly calendar?: string
  }) => Effect.Effect<ReadonlyArray<CalendarEvent>, AutomationError>
  readonly today: Effect.Effect<ReadonlyArray<CalendarEvent>, AutomationError>
}>()("@jarvis/apple/Calendar") {
  static readonly layer = Layer.effect(
    Calendar,
    Effect.gen(function*() {
      const osascript = yield* Osascript

      const calendars = osascript.run(CALENDARS_SCRIPT).pipe(
        Effect.map((output) => parseRecords(output).flatMap((record) => record.slice(0, 1))),
        Effect.withSpan("Calendar.calendars")
      )

      const query = Effect.fn("Calendar.query")(function*(
        from: DateTime.Utc,
        to: DateTime.Utc,
        calendar: string | undefined
      ) {
        const fromMillis = DateTime.toEpochMillis(from)
        const toMillis = DateTime.toEpochMillis(to)
        const span = toMillis - fromMillis
        if (span <= 0) {
          return yield* new AutomationError({ message: "`to` must be after `from`." })
        }
        if (span > MAX_SPAN_MILLIS) {
          return yield* new AutomationError({ message: `The range must not exceed ${MAX_SPAN_DAYS} days.` })
        }
        const zone = DateTime.zoneMakeLocal()
        const output = yield* osascript.run(EVENTS_SCRIPT, [
          calendar ?? "",
          ...localParts(zone, from),
          ...localParts(zone, to)
        ])
        const raw = yield* Effect.forEach(parseRecords(output), (record) =>
          decodeRow(record).pipe(
            Effect.mapError(invalid("Unexpected Calendar output")),
            Effect.map(([id, title, name, ...rest]): RawEvent => ({
              id,
              title,
              calendar: name,
              start: rest.slice(0, 6) as unknown as Parts,
              end: rest.slice(6, 12) as unknown as Parts,
              allDay: rest[12] === "true",
              location: rest[13] as string,
              recurrence: rest[14] as string,
              excludedDays: parseExcludedDays(rest[15] as string)
            }))
          ))
        const placed = yield* place(zone, raw, fromMillis, toMillis)
        const iso = (millis: number) => DateTime.formatIsoOffset(DateTime.makeZonedUnsafe(millis, { timeZone: zone }))
        return Arr.sort(placed, byStartThenTitle).map(({ end, event, start }): CalendarEvent => ({
          id: event.id,
          title: event.title,
          calendar: event.calendar,
          start: iso(start),
          end: iso(end),
          allDay: event.allDay,
          ...(event.location.length > 0 ? { location: event.location } : {})
        }))
      })

      const events = Effect.fn("Calendar.events")(function*(input: {
        readonly from: string
        readonly to: string
        readonly calendar?: string
      }) {
        const { from, to, calendar } = yield* decodeInput(input).pipe(Effect.mapError(invalid("Invalid input")))
        return yield* query(yield* parseInstant("from", from), yield* parseInstant("to", to), calendar)
      })

      const today = Effect.gen(function*() {
        const now = DateTime.setZone(yield* DateTime.now, DateTime.zoneMakeLocal())
        const start = DateTime.startOf(now, "day")
        const end = DateTime.add(start, { days: 1 })
        return yield* query(DateTime.toUtc(start), DateTime.toUtc(end), undefined)
      }).pipe(Effect.withSpan("Calendar.today"))

      return Calendar.of({ calendars, events, today })
    })
  )
}
