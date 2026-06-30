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

## Tests

```sh
cargo test
```

14 unit tests build a synthetic parent + sidecar in memory (via osmflat-extc's
`test-support`) and assert every endpoint's rows against a known fixture: counts,
fractions, per-type distinct values, the null-stub contract, sort, pagination,
the value-count invariant, and JSON round-trip.

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

[taginfo.openstreetmap.org]: https://taginfo.openstreetmap.org
[osmflat]: https://docs.rs/osmflat
[`osmflat-ext`]: ../osmflat-ext
