# osmflat-taginfo

A [taginfo.openstreetmap.org] -compatible CLI over an [osmflat] archive and its
[`osmflat-ext`] `Taginfo` sidecar. It prints **taginfo API v4-shaped JSON**
behind an ergonomic noun/verb command surface.

This binary is a pure **reader/formatter**: index building lives in
`osmflat-extc`, querying lives in `osmflat-ext`, and this crate parses args →
calls the `osmflat_ext::taginfo` query API → assembles the taginfo envelope →
prints. Full design: [`osmflat-taginfo-design.md`](./osmflat-taginfo-design.md).

## Prerequisites

A parent osmflat archive and a sibling Ext sidecar built with the taginfo index:

```sh
osmflat-extc --taginfo --out planet.osm.ext planet.osm.flatdata
```

## Usage

```sh
osmflat-taginfo -a <ARCHIVE> -x <EXT> <COMMAND>

# the keys table (taginfo /api/4/keys/all), top 20 by object count
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext keys --rp 20

# only keys with a prefix (taginfo's key search box)
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext keys --search addr:

# one key's per-type counts + distinct values (taginfo /api/4/key/stats)
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext key highway stats
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext key highway        # stats is the default verb

# a key's distinct values with counts (taginfo /api/4/key/values)
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext key highway values --rp 20

# one tag's per-type counts (taginfo /api/4/tag/stats)
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext tag highway primary stats
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext tag highway=primary        # KEY=VALUE token, stats default

# co-occurring keys / tags (taginfo /api/4/{key,tag}/combinations)
# needs a sidecar built with `osmflat-extc --combinations`
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext key highway combinations --rp 20
osmflat-taginfo -a planet.osm.flatdata -x planet.osm.ext tag highway primary combinations --rp 20
```

Global flags: `--format json|pretty|table` (default `pretty`), `--page`, `--rp`
(0 = all), `--sortname`, `--sortorder asc|desc`, `--no-envelope`.

## Implemented

| Command | taginfo route |
|---|---|
| `keys` (+`--search`) | `/api/4/keys/all` |
| `key <KEY> stats` | `/api/4/key/stats` |
| `key <KEY> values` | `/api/4/key/values` |
| `tag <KEY> <VALUE> stats` (or `tag <KEY=VALUE>`) | `/api/4/tag/stats` |
| `key <KEY> combinations` | `/api/4/key/combinations` |
| `tag <KEY> <VALUE> combinations` | `/api/4/tag/combinations` |

Output matches taginfo's envelope (`url, data_until, page, rp, total, data`) and
per-item field order byte-for-byte (`serde_json` `preserve_order`).

The `combinations` commands need a sidecar built with `osmflat-extc
--combinations`; without it they return an empty result (with a stderr hint).

## Performance

Measured on an Apple M1 Pro (10 cores, 16GB), median of 3 warm runs. The
archives are mmap'd, so a cold first touch measures the page cache rather than
the query; every figure below is warm.

| extract | objects | distinct keys | `.osm.flat` | `.osmflat.ext` |
|---|---|---|---|---|
| Minnesota | ~37M | 3,022 | 713MB | 354MB |
| United States | ~1,738M | 25,078 | 50GB | 44GB |

Sidecars built with `--taginfo --combinations --key-postings --value-search`.
`--key-postings` matters: without it every bbox-clipped key count merges that
key's per-value postings at query time, which is what the `--bbox` column below
would otherwise be dominated by.

`--bbox` uses a dense city inside each extract -- Minneapolis for Minnesota,
Manhattan for the US -- so the two columns compare like with like rather than
one region's city against another's countryside.

| command | Minnesota | United States | rows (US) |
|---|---|---|---|
| `keys` | 0.011s | 0.060s | 25,078 |
| `key <k> stats` | 0.005s | 0.007s | 4 |
| `key <k> values` | 0.005s | 0.007s | 172 |
| `key <k> combinations` | 0.006s | 0.022s | 6,451 |
| `tag <k=v> stats` | 0.005s | 0.007s | 4 |
| `tag <k=v> combinations` | 0.178s | **9.371s** | 4,207,984 |
| `keys --bbox` | 0.102s | 0.923s | 2,596 |
| `key <k> stats --bbox` | 0.059s | 0.079s | 4 |
| `key <k> values --bbox` | 0.058s | 0.081s | 40 |
| `key <k> combinations --bbox` | 0.090s | 0.236s | 607 |
| `tag <k=v> stats --bbox` | 0.056s | 0.078s | 4 |
| `tag <k=v> combinations --bbox` | 0.152s | 4.358s | 4,540 |

`<k>` is `highway`, `<k=v>` is `highway=residential`.

Everything except `tag ... combinations` stays under a second even on a
1.7-billion-object archive, because the counts are stored aggregates rather than
scans.

What the two columns scale with is worth reading off directly. The US archive
holds ~47x Minnesota's objects, but the single-key lookups (`key stats`,
`key values`, `tag stats`) are only ~1.4x slower with or without a bbox: they
touch one key's stored aggregate, so the size of the rest of the archive barely
registers. The whole-table sweeps track the *key count* instead, not the object
count -- `keys --bbox` is 9.0x slower against 8.3x as many distinct keys (25,078
vs 3,022).

### The `tag ... combinations` outlier

It returns every distinct *tag* co-occurring with the queried one -- 4.2M rows
on the US extract -- so most of that 9.4s is producing and serialising rows, not
finding them.

A page is cheaper than the whole table. Without `--bbox`, the requested page is
chosen from each combination's stored count and key/value strings first, and
only that page's rows are built -- the per-row `to_fraction` lookup of the other
tag is what dominates building all of them. Measured on a South America extract
(644M nodes, 52M ways; `highway=residential` co-occurs with 1.2M tags), same
machine, median of 3 warm runs:

| | before | after |
|---|---|---|
| `--rp 100` (one page) | 1.80s | 0.39s |
| `--rp 100 --page 50` | 1.80s | 0.38s |
| `--rp 100 --sortname other_key` | 1.83s | 0.45s |
| `--rp 0` (all 1.2M rows) | 2.79s | 2.79s |
| `--rp 100 --sortname to_fraction` | 2.04s | 2.03s |
| `--rp 100 --bbox` (São Paulo) | 6.30s | 6.33s |

What remains in a page is reading every combination's key and value strings to
sort on. Three cases still build every row, because each row's sort key or
existence is the expensive part: `--sortname to_fraction` (needs the other
tag's total), `--bbox` (`together_count` is recomputed per row from clipped
postings, and rows that drop to zero are removed), and `--rp 0`.

## Tests

```sh
cargo test
```

18 unit tests (29 with `--features serve`) build a synthetic parent + sidecar in
memory (via osmflat-extc's `test-support`) and assert every endpoint's rows
against a known fixture: counts,
fractions, per-type distinct values, the null-stub contract, sort, pagination
(including that `tag ... combinations`' page selection matches slicing the full
sorted table), the value-count invariant, and JSON round-trip.

## Caveats

- Counts reflect the **loaded extract**, not the live planet, so fractions are
  not comparable to the website.
- Fractions are rounded to **4 decimal places** to match taginfo; recompute from
  the integer counts if you need exact ratios.
- osmflat carries no wiki/JOSM/project/user data, so `users_all`, `in_wiki`,
  `projects`, and value descriptions are emitted as documented neutral stubs.
- Strings are rendered lossy UTF-8.
- The sidecar must match the exact parent build; a mismatch is caught by the
  osmflat-ext fingerprint and refuses to open.

## Licenses

Two separate things are licensed here: this code, and the OpenStreetMap data it
reads. They are not the same license, and the data one is the one with ongoing
obligations.

### This code

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. This matches [`osmflat-ext`] and [osmflat] upstream. Unless you
explicitly state otherwise, any contribution intentionally submitted for
inclusion in this work shall be dual-licensed as above, without any additional
terms or conditions.

Note that the *binary* pulls in an Apache-2.0-only dependency (`flatdata`), plus
BSD-3-Clause and Unicode-3.0 components, so redistributing a built binary means
carrying those attributions even if you take the MIT option for this crate's own
source. No dependency in the tree is copyleft.

### The data

OpenStreetMap data is © OpenStreetMap contributors and licensed under the [Open
Database License][odbl] (ODbL). That license follows the data through this tool:
an osmflat archive is a derived database, the taginfo tables this CLI computes
are derived from it, and the JSON it prints is produced from that.

So if you publish this tool's output — run the `serve` subcommand publicly,
ship the JSON, or put the numbers in something you distribute — the ODbL
attribution and share-alike terms apply to you, not to this tool. At minimum,
attribute
"© OpenStreetMap contributors" wherever the output is surfaced. See the
[OSMF copyright page][osmf] for what attribution has to look like in practice.

This tool emits no attribution string of its own, deliberately: it has no way to
know how its output will be surfaced.

[odbl]: https://opendatacommons.org/licenses/odbl/1-0/
[osmf]: https://www.openstreetmap.org/copyright

[taginfo.openstreetmap.org]: https://taginfo.openstreetmap.org
[osmflat]: https://docs.rs/osmflat
[`osmflat-ext`]: https://github.com/boydjohnson/osmflat-ext
