# DateTime

`DateTime` represents an instant in time, rendered either in UTC or a fixed local offset. It is
auto-imported into every program — no `import` needed, just like `Math`, `File`, and `Time`.

Only two operations genuinely need the host: reading the wall clock and resolving the local system
timezone's UTC offset (including DST). Everything else — calendar math, arithmetic, comparison,
and ISO-8601 formatting/parsing — is implemented in pure Dream, so it behaves identically on the
native wasmtime CLI, Node.js, and the browser.

```dream
fun main(): void {
    let now = DateTime.now();                 // local time
    System.println(now.to_iso8601());

    let launch = DateTime.of(2026, 7, 2, 9, 30, 0, 0);   // UTC
    let deadline = launch.add_days(7);
    System.println(deadline.to_iso8601());
    System.println(launch.is_before(deadline));
}
```

## Construction

| Member | Description |
| --- | --- |
| `DateTime.utc_now(): DateTime` | the current instant, in UTC |
| `DateTime.now(): DateTime` | the current instant, rendered in the local system timezone |
| `DateTime.from_epoch_millis(millis): DateTime` | wraps a raw UTC epoch millisecond instant |
| `DateTime.of(year, month, day, hour, minute, second, millisecond): DateTime` | builds a UTC instant from calendar fields |
| `DateTime.of_local(year, month, day, hour, minute, second, millisecond): DateTime` | builds an instant from calendar fields interpreted as local wall-clock time |

`DateTime` also has a public constructor, `DateTime(epoch_millis, offset_minutes)`, for the rare
case where you already have both parts.

## Fields and calendar accessors

Every `DateTime` exposes its raw instant and rendering offset directly:

```dream
let dt = DateTime.now();
System.println(dt.epoch_millis);      // long: UTC epoch milliseconds
System.println(dt.offset_minutes);    // int: minutes east of UTC (0 for UTC)
```

Calendar fields are derived from `epoch_millis + offset_minutes` on demand:

| Member | Description |
| --- | --- |
| `year(): int` / `month(): int` / `day(): int` | calendar date, in the rendered offset |
| `hour(): int` / `minute(): int` / `second(): int` / `millisecond(): int` | time of day, in the rendered offset |
| `day_of_week(): int` | `0` = Sunday ... `6` = Saturday (matches JS `Date.getDay()`) |
| `day_of_year(): int` | 1-based day of the year (Jan 1st is `1`) |

## Conversion

| Member | Description |
| --- | --- |
| `to_utc(): DateTime` | the same instant, rendered in UTC |
| `to_local(): DateTime` | the same instant, re-resolved to the local system timezone's offset for that exact instant (correct across DST transitions) |

## Arithmetic

`add_millis`/`add_seconds`/`add_minutes`/`add_hours`/`add_days` all take a `long` and return a new
`DateTime` with the same `offset_minutes`:

```dream
let dt = DateTime.of(2026, 7, 2, 10, 0, 0, 0);
let tomorrow = dt.add_days(1);
let an_hour_ago = dt.add_hours(0L - 1L);
```

Because arithmetic preserves `offset_minutes` rather than re-resolving it, a local `DateTime` that
crosses a DST boundary keeps its original offset; call `.to_local()` afterwards if you need the
DST-correct offset for the result.

## Comparison

`compare_to`, `is_before`, `is_after`, and `equals` all compare the absolute instant
(`epoch_millis`), regardless of `offset_minutes`:

```dream
let a = DateTime.of(2026, 1, 1, 0, 0, 0, 0);
let b = DateTime.of(2026, 6, 1, 0, 0, 0, 0);
System.println(a.is_before(b));     // true
System.println(a.compare_to(b));    // -1
```

## Formatting and parsing

`to_iso8601()` renders `"YYYY-MM-DDTHH:mm:ss.fffZ"` for UTC or with a `"+HH:MM"`/`"-HH:MM"` suffix
for a non-zero offset. `to_string()` is a more human-readable variant (space-separated, no
fractional seconds, `"UTC"` or `"+HH:MM"` suffix).

`DateTime.parse_iso8601(text)` parses `"YYYY-MM-DDTHH:mm:ss[.fff](Z|+HH:MM|-HH:MM)?"` and returns a
`Result<DateTime, string>`. A missing fractional part defaults to `0`; more than three fractional
digits are truncated to milliseconds. A missing offset (and no trailing `Z`) is treated as UTC:

```dream
let parsed = DateTime.parse_iso8601("2026-07-02T10:35:00.250Z");
System.println(parsed.unwrap_or(DateTime.from_epoch_millis(0L)).to_iso8601());
```

## Runtime support

| Runtime | Wall clock | Local timezone offset |
| --- | --- | --- |
| Wasmtime (native CLI) | `std::time::SystemTime` | the `chrono` crate (OS timezone database, DST-aware) |
| Node.js / browser | `Date.now()` | `Date.getTimezoneOffset()` |
