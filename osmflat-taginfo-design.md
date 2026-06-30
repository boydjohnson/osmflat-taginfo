# Design: `osmflat-taginfo` — a taginfo.openstreetmap.org-compatible CLI over osmflat

Status: draft spec / plan.

A command-line reimplementation of the [taginfo.openstreetmap.org] query surface
that reads an [osmflat] archive plus its [`osmflat-ext`] `Taginfo` sidecar and
prints **JSON that is byte-for-byte shape-compatible with the taginfo API v4**,
behind an ergonomic noun/verb CLI.

This crate is the third in the family:

```
osmflat-rs   (../osmflat-rs)    the base format: Osm archive + osmflatc compiler
osmflat-ext  (../osmflat-ext)   the Taginfo sidecar + query lib (this CLI's data source)
osmflat-taginfo  (this repo)    the taginfo-compatible CLI / formatter   ← new
```

It owns **no index building and no query algorithms** — those live in
`osmflat-ext`. This crate is a thin, well-factored layer: parse args → call the
`osmflat_ext::taginfo` query API → assemble the taginfo JSON envelope → print.

---

## 1. Goal & non-goals

**Goal.** Reproduce the data-bearing taginfo HTTP endpoints as local subcommands,
emitting the exact taginfo API v4 JSON (envelope + item field names), so existing
taginfo client code and scripts can be repointed at a local archive by swapping a
URL for a shell command. Decisions locked with the user:

- **JSON fidelity:** exact taginfo v4 envelope and per-item field names. Fields
  osmflat cannot supply are emitted as documented stubs, never silently dropped
  (§4).
- **Input:** the CLI is a pure **reader**. It requires a parent `Osm` archive and
  a prebuilt `Ext` sidecar (`osmflat-extc --taginfo [--combinations]`). It does
  **not** build or refresh sidecars — that is `osmflat-extc`'s job, and mixing
  the two would pull the compiler into this binary.

**Non-goals.**

- No HTTP server. (A `serve` mode that wraps the same formatters behind the
  taginfo URL routes is a clean future extension — see §11 — but v1 is CLI only.)
- No index construction, no PBF reading, no spatial algorithms. All querying is
  delegated to `osmflat_ext`.
- No reproduction of taginfo fields that have no source in osmflat data
  (wiki/JOSM/project metadata, per-user stats). These are stubbed (§4.4).
- Not a substitute for taginfo's full endpoint catalog. v1 covers the six
  data endpoints in §3; the rest are out of scope or phase 2.

---

## 2. Relationship to `osmflat-ext` (the data source)

Every number this CLI prints comes from one of these `osmflat_ext` calls (all
already implemented per the osmflat-ext README):

| CLI needs | `osmflat_ext` call | cost |
|---|---|---|
| open + verify sidecar | `ExtArchive::open(parent, ext)` (checks fingerprint) | O(1) |
| taginfo entry point | `archive.taginfo()? -> TaginfoQuery` | O(1) |
| all keys | `tq.keys() -> impl Iterator<KeyView>` | O(K) |
| keys by prefix (search) | `tq.keys_with_prefix(prefix)` | O(log K + m) |
| one key | `tq.key(b"highway") -> Option<KeyView>` | O(log K) |
| key per-type counts | `KeyView::counts() -> TypeCounts {nodes,ways,relations}` | O(1) |
| key distinct-value count | `KeyView::distinct_values() -> u64` | O(1) |
| key's values | `KeyView::values() -> impl Iterator<ValueView>` | O(values) |
| one value | `KeyView::value(b"primary")` / `tq.kv(k, v)` | O(log V) |
| value per-type counts | `ValueView::counts() -> TypeCounts` | O(1) |
| key combinations | `KeyView::combinations() -> CombinationView{key, together_count}` | O(combos) |
| tag combinations | `ValueView::combinations() -> TagCombinationView{key, value, together_count}` | O(combos) |
| example object ids | postings `ValueView::{nodes,ways,relations}()` + `osmflat::{node_id,way_id,relation_id}` | O(limit) |

The denominators taginfo needs for `*_fraction` fields come from the **parent**
archive vector lengths, read once at startup:

```
total_nodes      = parent.nodes().len()        // minus the flatdata sentinel
total_ways       = parent.ways().len()
total_relations  = parent.relations().len()
total_objects    = total_nodes + total_ways + total_relations
```

(Verify the exact sentinel handling against osmflat's accessors during
implementation; the same `- 1` convention osmflat-ext uses for `keys()` applies.)

All strings from `osmflat_ext` are raw `&[u8]` (parent stringtable slices). The
CLI converts with `String::from_utf8_lossy` at the JSON boundary — OSM strings
are conventionally UTF-8, but the format does not guarantee it, so lossy is the
safe default (note this in `--help`).

---

## 3. Command surface

Binary name: **`osmflat-taginfo`**. Built with `clap` derive. Noun/verb layout
mirroring taginfo's own information architecture (keys → key → values/tag), each
subcommand mapping to one taginfo API route.

### 3.1 Global args (apply to every subcommand)

```
-a, --archive <DIR>     Parent osmflat archive directory.        [required]
-x, --ext <DIR>         Sibling Ext sidecar (built --taginfo).   [required]
    --format <FMT>      json | pretty | table        [default: pretty]
    --page <N>          1-based page (taginfo pagination).        [default: 1]
    --rp <N>            Rows per page; 0 = all.                   [default: 0]
    --sortname <FIELD>  Sort field (per-endpoint allowed set).
    --sortorder <DIR>   asc | desc                               [default: desc]
    --no-envelope       Emit the bare data array, drop the taginfo envelope.
```

- `--archive`/`--ext` may also be given as the first two positionals for terse
  invocation; flags win if both are present.
- `--format pretty` is `serde_json::to_string_pretty`; `json` is compact
  (one line, the wire-faithful form); `table` is a human aligned table (what the
  osmflat-ext `taginfo` example prints today) and is explicitly **not** stable
  output — scripts use `json`/`pretty`.
- Pagination/sorting are applied by this CLI over the full result the query
  returns (the underlying postings are already sorted by string; count-sorts are
  done here), so behavior matches taginfo's server-side paging.

### 3.2 Subcommands → taginfo routes

| Subcommand | taginfo route | Purpose |
|---|---|---|
| `keys` | `/api/4/keys/all` | The keys table: every key with per-type counts + fractions. |
| `key <KEY> stats` | `/api/4/key/stats` | One key's all/nodes/ways/relations counts + distinct values. |
| `key <KEY> values` | `/api/4/key/values` | Distinct values of a key with counts/fractions. |
| `key <KEY> combinations` | `/api/4/key/combinations` | Other keys co-occurring with this key. |
| `tag <KEY> <VALUE> stats` | `/api/4/tag/stats` | One `key=value`'s per-type counts. |
| `tag <KEY> <VALUE> combinations` | `/api/4/tag/combinations` | Other tags co-occurring with this tag. |

Ergonomic conveniences (sugar over the table above; same JSON out):

- `osmflat-taginfo key <KEY>` with no verb ⇒ `key <KEY> stats`.
- `osmflat-taginfo tag <KEY> <VALUE>` with no verb ⇒ `tag <KEY> <VALUE> stats`.
- `osmflat-taginfo tag <KEY=VALUE>` accepted as a single `key=value` token in
  addition to the two-arg form.
- `--search <PREFIX>` on `keys` filters via `keys_with_prefix` (taginfo's key
  search box), still emitting the `keys/all` item shape.

Examples:

```text
# the keys table, top 20 by object count, human-readable
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext keys --rp 20 --sortname count_all

# wire-faithful JSON for one key's values, paged like the website
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext \
    key highway values --page 1 --rp 50 --format json

# co-occurring tags for highway=primary (needs a --combinations sidecar)
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext tag highway primary combinations

# terse positional form, single key=value token
osmflat-taginfo planet.osm.flatdata planet.osm.ext tag highway=primary
```

---

## 4. JSON output — exact taginfo v4 shapes

### 4.1 The envelope (every response unless `--no-envelope`)

Confirmed against the live API for `keys/all`, `key/values`, `key/stats`,
`key/combinations`:

```json
{
  "url": "osmflat-taginfo key highway values",
  "data_until": "2026-06-29T00:00:00Z",
  "page": 1,
  "rp": 50,
  "total": 312,
  "data": [ /* items, schema per endpoint below */ ]
}
```

- `url` — taginfo puts the request URL here. We put the **canonical CLI
  invocation** that produced the output (useful, and there is no URL to report).
  A future `serve` mode would put the real URL.
- `data_until` — the freshness timestamp. Source, in priority order:
  1. the parent's replication timestamp if osmflat exposes one (the ext header
     already carries `parent_replication_sequence_number`; check whether a
     timestamp is reachable from the parent header);
  2. else the parent archive directory mtime;
  3. else the process start time.
  Formatted as taginfo does: `YYYY-MM-DDThh:mm:ssZ` (UTC, second precision).
- `page`, `rp` — echo the effective pagination. When `--rp 0` (all), emit
  `rp: 0` and `page: 1`, matching taginfo's "all rows" convention.
- `total` — total rows **before** paging. Note taginfo serializes some numeric
  envelope values (`total`) as JSON **strings** in places (observed `"29647"`);
  match the per-endpoint observed type exactly during implementation by snapshot
  comparison (§9). Default assumption: `total` as a number; revisit if a golden
  diff shows a string.

### 4.2 `keys` → `/api/4/keys/all` item

```json
{
  "key": "highway",
  "count_all": 123456,
  "count_all_fraction": 0.0123,
  "count_nodes": 1000,
  "count_nodes_fraction": 0.0002,
  "count_ways": 122000,
  "count_ways_fraction": 0.0500,
  "count_relations": 456,
  "count_relations_fraction": 0.0010,
  "values_all": 312,
  "users_all": null,
  "in_wiki": null,
  "projects": null
}
```

Source map:

| field | source |
|---|---|
| `key` | `KeyView::key()` (lossy UTF-8) |
| `count_nodes/ways/relations` | `KeyView::counts()` |
| `count_all` | sum of the three |
| `count_*_fraction` | `count_* / total_*` (the parent denominators, §2); `count_all_fraction = count_all / total_objects` |
| `values_all` | `KeyView::distinct_values()` |
| `users_all` | **stub null** (§4.4) |
| `in_wiki` | **stub null** (§4.4) |
| `projects` | **stub null** (§4.4) |

Fraction encoding: taginfo emits these as JSON numbers in `[0,1]`. Match its
rounding by snapshot (it appears to round to a fixed number of decimals); start
with full `f64` and tighten to taginfo's precision once a golden file is
captured (§9).

### 4.3 `key … values` → `/api/4/key/values` item

```json
{
  "value": "primary",
  "count": 8000,
  "fraction": 0.0640,
  "in_wiki": false,
  "description": null,
  "desclang": null,
  "descdir": null
}
```

| field | source |
|---|---|
| `value` | `ValueView::value()` |
| `count` | total of `ValueView::counts()` across the three types |
| `fraction` | `count / KeyView::count_all` (taginfo's "fraction of this key's objects") |
| `in_wiki` | stub `false` |
| `description`, `desclang`, `descdir` | stub `null` (wiki-sourced) |

### 4.4 `key … stats` → `/api/4/key/stats` item, and `tag … stats`

Four rows, one per type plus `all` (taginfo's exact shape, includes per-type
distinct `values`):

```json
{ "type": "all",       "count": 123456, "count_fraction": 0.0123, "values": 312 },
{ "type": "nodes",     "count": 1000,   "count_fraction": 0.0002, "values": 50  },
{ "type": "ways",      "count": 122000, "count_fraction": 0.0500, "values": 280 },
{ "type": "relations", "count": 456,    "count_fraction": 0.0010, "values": 12  }
```

- `count` / `count_fraction` — from `counts()` and the §2 denominators.
- `values` — distinct values **for that object type**. ⚠️ The sidecar stores
  distinct values per key (`distinct_values()`), not per (key,type). Per-type
  distinct-value counts require scanning the key's `ValueView`s and counting
  those with a non-empty postings list for that type — O(values), still cheap.
  The `all` row's `values` = `distinct_values()`. (Document this derivation; it
  is the one place a stat is computed in this crate rather than read O(1).)
- `tag … stats` (`/api/4/tag/stats`) uses the same four `all`/`nodes`/`ways`/
  `relations` rows with `count` + `count_fraction` from `ValueView::counts()` and
  the §2 denominators — but **drops the `values` column** (a tag has no distinct
  values). Confirmed against the live API. Implemented as a distinct `TagStatRow`
  so the field set differs from `key … stats` exactly as taginfo's does.

### 4.5 `key … combinations` → `/api/4/key/combinations` item

```json
{ "other_key": "name", "together_count": 90000, "to_fraction": 0.30, "from_fraction": 0.73 }
```

- `other_key`, `together_count` — `CombinationView::{key, together_count}`.
- `from_fraction` = `together_count / count_all(this key)`.
- `to_fraction` = `together_count / count_all(other_key)` — requires a second
  `KeyView::counts()` lookup on `other_key` (O(log K) each; fine).
- Empty `data` (with a stderr note) when the sidecar was built without
  `--combinations`, matching the example's behavior.

### 4.6 `tag … combinations` → `/api/4/tag/combinations` item

```json
{ "other_key": "surface", "other_value": "asphalt", "together_count": 5000,
  "to_fraction": 0.40, "from_fraction": 0.62 }
```

From `TagCombinationView::{key, value, together_count}`; fractions analogous to
§4.5 (`from` over this tag's count; `to` over the other tag's count via
`tq.kv(other_key, other_value)?.counts()`).

### 4.4-bis Stubbed fields — the honesty contract

osmflat carries no wiki, JOSM, project, or per-user data, so these taginfo fields
have no source. Every one is emitted as **`null`** — the honest "unknown" rather
than an asserted `false`/`0` — so the JSON **shape** stays drop-in while never
claiming a fact we can't back. The substitution is documented in `--help` and the
README:

| field(s) | stub | reason |
|---|---|---|
| `users_all`, per-type users | `null` | osmflat has no changeset/user attribution by default |
| `in_wiki` | `null` | no wiki crawl |
| `projects` | `null` | no project (JOSM/iD/etc.) catalog |
| `description`, `desclang`, `descdir` | `null` | wiki-sourced |

A `--strict-fields` flag (phase 2) could instead **omit** unsupported fields
entirely, for consumers that prefer absence over a `null`.

---

## 5. Sorting & pagination

Implemented in this crate (the query layer returns full, string-sorted result
sets; count-based ordering is a CLI concern):

- Allowed `--sortname` per endpoint matches taginfo's (`count_all`, `count_nodes`,
  `count_ways`, `count_relations`, `values_all`, `key` for `keys`; `count`,
  `fraction`, `value` for values; `together_count`, `other_key` for
  combinations). Reject unknown sort fields with a clear error listing the
  allowed set.
- Default sort per endpoint matches the website (e.g. `keys` → `count_all desc`,
  `values` → `count desc`).
- Paging: compute `total` from the full set, then `skip = (page-1)*rp`,
  `take = rp` (or all when `rp == 0`). `page` out of range ⇒ empty `data`,
  correct `total`.

Determinism: ties broken by the secondary string key (taginfo is stable-ish;
we make it fully deterministic so golden tests don't flake).

---

## 6. Crate / binary structure

Single binary crate; small, testable modules. No lib split needed (the lib is
`osmflat-ext`).

```
osmflat-taginfo/
  Cargo.toml         edition 2021 to match the workspace; deps below
  src/
    main.rs          clap parse → dispatch → print
    cli.rs           clap derive types (global args + subcommands, §3)
    open.rs          open parent + ext, verify fingerprint, compute denominators
    model.rs         serde structs for every item shape + the envelope (§4)
    endpoints/
      keys.rs        keys/all
      key_stats.rs   key/stats
      key_values.rs  key/values
      key_combos.rs  key/combinations
      tag_stats.rs   tag/stats
      tag_combos.rs  tag/combinations
    page.rs          sort + paginate (§5)
    fmt.rs           json | pretty | table writers
    freshness.rs     data_until resolution (§4.1)
```

Dependencies:

```toml
[dependencies]
osmflat-ext = { path = "../osmflat-ext/osmflat-ext" }   # or git, matching ext's pin
osmflat     = { git = "https://github.com/boydjohnson/osmflat-rs", branch = "feature/spatial-index" }
clap        = { version = "4", features = ["derive"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
anyhow      = "1"          # CLI-level error context
time        = { version = "0.3", features = ["formatting"] }  # data_until RFC3339-ish
```

(Confirm the exact `osmflat-ext` path/crate name — the lib lives at
`../osmflat-ext/osmflat-ext` inside that workspace. Decide path-dep vs. git-dep;
path is simplest for co-development, git matches how ext pins osmflat.)

Each endpoint module is a pure function
`fn run(ctx: &Ctx, args: &EndpointArgs) -> anyhow::Result<Envelope<Item>>`, where
`Ctx` holds the opened `ExtArchive`, the `TaginfoQuery`, and the denominators.
`main` matches the subcommand, calls `run`, hands the typed `Envelope` to
`fmt::write`. This keeps every endpoint independently unit-testable and the JSON
shape centralized in `model.rs`.

---

## 7. Error handling & exit codes

- Missing/mismatched sidecar (`fingerprint::Mismatch`) → exit 2, message:
  "Ext sidecar was built against a different parent archive; rebuild with
  `osmflat-extc --taginfo`."
- Sidecar lacks `--taginfo` sub-archive → exit 2, "sidecar has no taginfo index".
- Unknown key / `key=value` → **not** an error: emit a valid envelope with
  `total: 0`, `data: []` (taginfo returns empty, not 404), exit 0. (A `--strict`
  flag could make a miss exit non-zero.)
- `combinations` on a non-`--combinations` sidecar → empty `data` + a one-line
  stderr hint, exit 0.
- Bad args (unknown `--sortname`, malformed `key=value`) → clap/explicit error,
  exit 2.

---

## 8. Phased roadmap

1. **Skeleton + read path.** ✅ `open.rs` (open parent+ext, verify fingerprint,
   compute denominators), `cli.rs` global args, output (json/pretty/table), and
   `keys` + `key … stats`. Proves the §2 wiring and the envelope. **(the MVP)**
2. **Values + tag stats.** ✅ `key … values`, `tag … stats` (per-type `values`
   distinct counts in key/stats §4.4; no `values` column in tag/stats), plus the
   `--search` prefix on `keys` and per-endpoint sort/paginate. `tag` accepts both
   `KEY VALUE` and the `KEY=VALUE` token.
3. **Sorting + pagination** (§5) — landed alongside phases 1–2 (shared
   `output::emit` paginator + per-endpoint sort); `table` formatter done.
4. **Combinations.** ✅ `key … combinations`, `tag … combinations`, with
   `to_fraction` (over the *other* key/tag) and `from_fraction` (over *this*
   one), plus the empty-result + stderr-hint "no `--combinations` sidecar" path.
5. **Fidelity hardening.** Golden snapshots vs. live taginfo on a known extract
   (§9); tighten fraction rounding and any string-vs-number envelope quirks
   (e.g. `total`); finalize the stubbed-field contract and `--help` wording.

---

## 9. Testing strategy

- **Unit per endpoint.** Build a tiny parent + Ext sidecar in-memory via
  osmflat-ext's `test-support` feature (the README documents
  `cargo test -p osmflat-extc --features test-support` building synthetic parent
  archives), run each endpoint's `run()`, assert the serde `Item`/`Envelope`
  field-by-field against a hand-computed expectation. The brute-force oracle is
  the same one osmflat-ext tests use.
- **Golden JSON snapshots.** For a fixed small extract (e.g. district-of-columbia,
  already referenced in the ext README), capture the live taginfo API response
  for the same key/tag and diff **shape** (field names, types, presence) — not
  the absolute counts, which differ by data vintage. This is what locks
  "exact field names" and surfaces the string-vs-number and rounding quirks
  flagged in §4.1/§4.2. Store goldens under `tests/golden/`.
- **Envelope invariants (property).** For any endpoint/args:
  `len(data) <= rp` (when `rp>0`); `total >=` `len(data)`; `page>=1`; every
  fraction in `[0,1]`; `count_all == nodes+ways+relations`; sum over a key's
  `values` counts `==` the key's `count_all` (the osmflat-ext invariant, surfaced
  through JSON).
- **Pagination/sort.** Same query at `--rp 10 --page 1..N` concatenated equals
  the un-paged result; sort orders are stable and reversible.
- **Stub contract.** Assert `users_all==0`, `in_wiki==false`, etc., are present
  and type-correct, so a future real source is an additive change.
- **Round-trip.** `--format json` output parses back into the `model.rs` structs.

---

## 10. Known limitations (state in `--help` / README)

1. **Reader only.** Needs a prebuilt, matching Ext sidecar; staleness is caught
   by the ext fingerprint, not silently.
2. **Stubbed metadata.** `users_all`, `in_wiki`, `projects`, value descriptions
   are placeholders — osmflat has no wiki/project/user data (§4.4).
3. **Counts reflect the extract, not the planet.** Fractions are over the loaded
   archive's totals; an extract's `count_all_fraction` is not comparable to the
   live planet site's.
4. **UTF-8 lossy.** Non-UTF-8 OSM strings are rendered with replacement chars in
   JSON.
5. **Combinations need a `--combinations` sidecar;** otherwise those endpoints
   return empty.
6. **Endpoint subset.** v1 covers the six §3 endpoints; taginfo's wiki, project,
   search, and relation endpoints are out of scope.

---

## 11. Open questions / future

- **`serve` mode.** Wrap the same `endpoints::*::run` functions behind the actual
  taginfo URL routes (`/api/4/...`) with a tiny HTTP layer, turning the CLI into
  a drop-in local taginfo API server. The §6 structure (pure `run` functions
  returning typed envelopes) is deliberately shaped to make this a thin add-on.
- **Spatial filter flag.** osmflat-ext exposes `ValueView::*_in_bbox` merge-joins;
  a `--bbox W S E N` filter on `tag … stats`/example output would be a natural,
  taginfo-doesn't-have-it bonus. Out of v1 to keep JSON parity clean.
- **`data_until` source.** Confirm whether the parent header exposes a usable
  replication timestamp; fall back chain in §4.1 otherwise.
- **Envelope numeric types.** Resolve via golden capture (§9) whether `total`
  (and any fraction) must serialize as JSON strings to be byte-faithful.
- **Path vs. git dependency** on `osmflat-ext`/`osmflat` (§6) for co-development.
- **`--strict-fields`** (omit unsupported) vs. the default stub-with-neutral
  approach (§4.4-bis).

[taginfo.openstreetmap.org]: https://taginfo.openstreetmap.org
[osmflat]: https://docs.rs/osmflat
[`osmflat-ext`]: ../osmflat-ext
