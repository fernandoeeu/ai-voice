// Read-only Calendar helper for @jarvis/apple.
//
//   calendar-events --list-calendars
//   calendar-events --from ISO --to ISO [--calendar NAME]
//
// Prints one JSON object on stdout. It never creates, edits or deletes
// anything: it only calls EventKit's query APIs. EventKit expands recurring
// series itself (`predicateForEvents` returns concrete occurrences), so the
// query is bounded by the range instead of scanning each calendar's history.
//
// Exit codes: 0 ok, 2 bad arguments, 3 Calendar access denied, 4 calendar
// not found, 5 the access request did not complete. Requires macOS 14+
// (`requestFullAccessToEvents`).

import EventKit
import Foundation

let EXIT_USAGE: Int32 = 2
let EXIT_DENIED: Int32 = 3
let EXIT_NOT_FOUND: Int32 = 4
let EXIT_NO_ANSWER: Int32 = 5

func fail(_ code: Int32, _ message: String) -> Never {
  FileHandle.standardError.write(Data((message + "\n").utf8))
  exit(code)
}

func emit(_ object: Any) -> Never {
  guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]) else {
    fail(1, "could not encode the result")
  }
  FileHandle.standardOutput.write(data)
  FileHandle.standardOutput.write(Data("\n".utf8))
  exit(0)
}

func parseDate(_ text: String) -> Date? {
  let withFraction = ISO8601DateFormatter()
  withFraction.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  if let date = withFraction.date(from: text) { return date }
  let plain = ISO8601DateFormatter()
  plain.formatOptions = [.withInternetDateTime]
  return plain.date(from: text)
}

func millis(_ date: Date) -> Int64 { Int64((date.timeIntervalSince1970 * 1000).rounded()) }

struct Options {
  var listCalendars = false
  var from: Date?
  var to: Date?
  var calendar: String?
}

func parseArguments(_ argv: [String]) -> Options {
  var options = Options()
  var index = 0
  func value(_ flag: String) -> String {
    index += 1
    guard index < argv.count else { fail(EXIT_USAGE, "missing value for \(flag)") }
    return argv[index]
  }
  while index < argv.count {
    let flag = argv[index]
    switch flag {
    case "--list-calendars": options.listCalendars = true
    case "--from":
      guard let date = parseDate(value(flag)) else { fail(EXIT_USAGE, "--from is not an ISO 8601 date") }
      options.from = date
    case "--to":
      guard let date = parseDate(value(flag)) else { fail(EXIT_USAGE, "--to is not an ISO 8601 date") }
      options.to = date
    case "--calendar": options.calendar = value(flag)
    default: fail(EXIT_USAGE, "unknown argument: \(flag)")
    }
    index += 1
  }
  return options
}

// Asks for full access when macOS has not decided yet, or when only
// write-only ("Add Events Only") access was granted: the system shows the
// upgrade prompt once. A recorded denial is reported right away instead of
// waiting for a prompt that will never show. TCC attributes the request to the
// responsible process (the terminal app or launchd job that started the
// server), so the grant is the one listed under that app in System Settings.
func ensureAccess(_ store: EKEventStore) {
  let status = EKEventStore.authorizationStatus(for: .event)
  if status == .fullAccess { return }
  guard status == .notDetermined || status == .writeOnly else {
    fail(EXIT_DENIED, "calendar access denied")
  }
  let done = DispatchSemaphore(value: 0)
  var granted = false
  store.requestFullAccessToEvents { ok, _ in
    granted = ok
    done.signal()
  }
  // The caller enforces its own deadline; this only avoids hanging forever.
  if done.wait(timeout: .now() + 60) == .timedOut { fail(EXIT_NO_ANSWER, "calendar access request timed out") }
  if !granted {
    fail(EXIT_DENIED, status == .writeOnly ? "calendar access is write-only" : "calendar access denied")
  }
}

let options = parseArguments(Array(CommandLine.arguments.dropFirst()))
let store = EKEventStore()
ensureAccess(store)
let calendars = store.calendars(for: .event)

if options.listCalendars {
  emit(["calendars": calendars.map { $0.title }])
}

guard let from = options.from, let to = options.to else { fail(EXIT_USAGE, "--from and --to are required") }
guard to > from else { fail(EXIT_USAGE, "--to must be after --from") }

var selected: [EKCalendar]? = nil
if let name = options.calendar {
  let matching = calendars.filter { $0.title == name }
  if matching.isEmpty { fail(EXIT_NOT_FOUND, "calendar not found") }
  selected = matching
}

let predicate = store.predicateForEvents(withStart: from, end: to, calendars: selected)
let local = Calendar.current
let events: [[String: Any]] = store.events(matching: predicate).compactMap { event in
  guard let start = event.startDate, var end = event.endDate else { return nil }
  // EventKit ends an all-day event at 23:59:59 of its last day; report the
  // following local midnight, the exclusive end the callers expect.
  if event.isAllDay {
    let lastDay = local.startOfDay(for: end)
    if end != lastDay, let next = local.date(byAdding: .day, value: 1, to: lastDay) { end = next }
  }
  if end <= from || start >= to { return nil }
  var item: [String: Any] = [
    "id": event.calendarItemExternalIdentifier ?? event.calendarItemIdentifier,
    "title": event.title ?? "",
    "calendar": event.calendar?.title ?? "",
    "start": millis(start),
    "end": millis(end),
    "allDay": event.isAllDay
  ]
  if let location = event.location, !location.isEmpty { item["location"] = location }
  return item
}
emit(["events": events])
