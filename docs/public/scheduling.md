# Scheduling model

`rust-desktop-ui-data` provides an owned `SchedulingModel` for appointments,
resources, recurrence, exceptions and UTC range queries. It has no dependency
on a window, renderer, transport or database. The application supplies resource
and series identities and validated timezone transition tables.

```rust
use rust_desktop_ui_data::{ScheduleQueryLimits, SchedulingModel};

let schedule = SchedulingModel::new();
assert_eq!(schedule.revision(), 0);
let limits = ScheduleQueryLimits::default();
assert_eq!(limits.max_occurrences, 10_000);
```

The executable example exercises a synthetic seasonal transition, a moved
occurrence, resource filtering and a rejected resource removal:

```text
cargo run -p rust-desktop-ui-data --example scheduling_queries
```

## Time and timezone coverage

`ScheduleInstant` is a checked Unix timestamp in whole UTC seconds. Leap
seconds are not represented. `ScheduleLocal` combines the existing `DateValue`
and `TimeValue`, rejects fractional seconds, and parses exactly
`YYYY-MM-DDTHH:MM:SS`. Civil dates range from year 0001 through 9999.

`ScheduleLocalInterval` and `ScheduleUtcInterval` are half-open `[start, end)`
intervals. Their end must follow their start. Resolving a local interval checks
both endpoints and requires `end_utc > start_utc` after applying timezone policy.
Recurring events preserve their local wall-clock duration: a day-long interval
can therefore span 23 or 25 UTC hours across an offset change.

`ScheduleTimeZone::new` accepts an identifier, finite UTC coverage, an initial
offset in seconds and strictly increasing `ScheduleTransition` values. Each
transition must lie inside coverage and change the offset. Offsets are bounded
to ±86,399 seconds; at most 4,096 transitions and at most two mappings of any
local time are allowed. Arithmetic is checked before publication. These tables
are application data; the library does not load an operating-system timezone,
an IANA database or current timezone legislation. Example transitions are
synthetic fixtures.

Coverage is `[start, end)`. A start instant at its exclusive end is invalid.
An interval may end exactly at coverage end when its local end matches the
last offset period's end. `to_local` still rejects that exclusive instant.
Queries must fit wholly within the coverage of every relevant series' zone.
Every included second must map to a supported civil date; an exclusive query
endpoint immediately after the last second of year 9999 is allowed.
An intersecting appointment whose complete interval exceeds coverage causes an
error, even when only its covered portion intersects the query.

`ScheduleResolvePolicy` makes ambiguous behavior explicit:

| Condition | Policies |
| --- | --- |
| Nonexistent local time after an offset increase | `Reject` or `Skip` the occurrence |
| Ambiguous local time after an offset decrease | `Reject`, `Earlier` UTC mapping or `Later` UTC mapping |

Both policies default to `Reject`. The library never silently shifts a local
time out of a gap. `Skip` is distinct from coverage, arithmetic and capacity
errors. Both endpoints are checked even when the first is skipped. A wholly
nonexistent interval is relevant to a range query at the instant of its forward
transition; a query wholly before that instant is unaffected by the gap.

## Recurrence and occurrence identity

`ScheduleRecurrence` supports these frequencies:

| Frequency | Rule |
| --- | --- |
| `Once` | Anchor interval only; interval must be 1 and termination `Never` |
| `Daily` | Every N days at the anchor local time |
| `Weekly` | Every N Monday-based weeks on sorted, distinct selected weekdays |
| `Monthly` | Every N months on the anchor day of month |
| `Yearly` | Every N years on the anchor month and day |

N is 1–366. There is no textual RRULE parser, ordinal weekday expression,
calendar holiday service or timezone database in this profile.

Termination is `Never`, `Count(1..=1_000_000)` or an inclusive local-start
`Until`. Count consumes **valid calendar candidates**, including candidates
subsequently skipped by DST policy, cancelled or moved by an exception.
Invalid calendar dates use `ScheduleInvalidDate::{Reject, Skip, LastDay}`.
An invalid date skipped by this calendar policy does not consume Count;
`LastDay` produces the final valid date of that month. Weekly dates before the
anchor in its first week do not consume Count.

`ScheduleOccurrenceId` contains the stable series identity and its **original
local candidate start**. UTC conversion, title/resource updates, changes of
duration and moving an occurrence do not replace that identity. Changing a
recurrence anchor or rule can change the candidate set; an update that would
strand an existing exception is rejected unless that exception is removed in
the same transaction.

`ScheduleException::Cancel` suppresses a candidate. `Replace` supplies a new
local interval and optional title/resource replacements in the same series
timezone. Replacements are queried independently of their original date, so
moving an old event into the requested range remains visible. An explicit
replacement that resolves to a DST skip is rejected on write. A cancelled
candidate remains a valid identity for later removal of its exception.

## Atomic changes and bounded queries

`apply(expected_revision, changes)` stages owned zone, resource, series and
exception descriptors, validates the complete candidate, and publishes one
revision and one `ScheduleChangeBatch`. References are validated against the
final candidate, making operation order immaterial. Removing a referenced
resource, zone or series requires corresponding dependent changes in the same
batch. Duplicate changes to the same identity, unknown removals, stale
revisions, invalid values, stranded exceptions and revision overflow leave
the entire model and journal unchanged. No callbacks or shared mutable
descriptors participate in publication. Identical final state is a no-op.

`changes_since` retains the most recent 64 batches. An older or future cursor
returns `reset_required`; consumers must reload from the model's immutable
views instead of treating the missing history as an empty change set.

`query(range, resources, limits)` returns overlapping occurrences sorted by
UTC start, UTC end and identity. A resource filter matches any listed identity;
an empty filter includes every event, including unassigned events. Filters
require at most 32 distinct known resources. Returned occurrences include
their identity, actual local and UTC intervals, title, resources and whether
they are replacements.

Expansion walks calendar candidates from the anchor. Late queries on a long
series can exceed the work budget; this implementation does not claim an
indexed recurrence engine. Invalid-calendar `Reject` stops traversal when the
invalid attempt precedes the query's upper bound. Candidates clearly outside
the range are culled before timezone resolution or duration expansion, so a
later gap or coverage error does not invalidate an earlier query.

| Bound | Default or maximum |
| --- | --- |
| Model zones / resources / series / exceptions | 64 / 4,096 / 4,096 / 65,536 |
| Changes per transaction | 256 |
| Resources per appointment | 32 |
| Logical model payload | 16 MiB |
| Candidate work per query | Default 100,000; maximum 1,000,000 |
| Returned occurrences per query | Default 10,000; maximum 100,000 |
| Timezone scan work per query | Default 1,000,000; maximum 10,000,000 |
| Transaction validation work | 1,000,000 candidates; 10,000,000 timezone checks |

All query limits must be nonzero. Exhausting any limit returns an error for the
whole query, never a silently truncated list. Timezone work includes conservative
scan charges and coverage validation. Logical payload accounting uses declared
string lengths and descriptor estimates; it is not an allocator capacity or
process-memory limit. The application owns persistence, shared-thread access,
UI projection, external timezone-table updates and synchronization.

## Verification

Unit tests cover gap/fold policies, 23/25-hour durations, month ends and leap
years, recurrence counts, original identities, moved/cancelled exceptions,
resource transactions, revision exhaustion, finite coverage, year-9999 query
boundaries and fail-whole-query budgets. The CLI example exercises the public
API without a UI host. Tests use explicit synthetic transition tables and do
not certify a geographic timezone database.
