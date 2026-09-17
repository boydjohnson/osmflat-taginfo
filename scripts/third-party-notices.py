#!/usr/bin/env python3
"""Generate THIRD-PARTY-NOTICES.md for a released binary.

Apache-2.0, BSD-3-Clause and Unicode-3.0 all require carrying their notices
when a *binary* is redistributed, and the release archives are exactly that.
Generated rather than committed: a static file drifts from Cargo.lock silently,
and the whole point is that it matches what actually got linked.

Run after a build, so the registry sources the license texts come from are
already on disk:

    scripts/third-party-notices.py --target x86_64-unknown-linux-gnu \
        --features serve -o THIRD-PARTY-NOTICES.md

Identical license texts are emitted once with every package that shares them
listed against it -- 90-odd verbatim MIT copies would bury the few that differ.
"""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

# Matched case-insensitively against file names in each package's root.
LICENSE_GLOBS = ("LICENSE*", "COPYING*", "NOTICE*", "UNLICENSE*")


def cargo_metadata(target, features):
    cmd = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--locked",
        # Only what actually links for this platform. cfg-gated deps for other
        # targets (wasi, windows-sys) are not in the shipped binary, and
        # attributing them would be noise.
        "--filter-platform",
        target,
    ]
    if features:
        cmd += ["--features", features]
    out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    return json.loads(out)


def runtime_packages(meta):
    """Packages reachable from the root by non-dev edges.

    Dev-dependencies build the test binary, not the released one, so their
    licenses carry no redistribution obligation here.
    """
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"] or meta["workspace_members"][0]

    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            kinds = {k["kind"] for k in dep["dep_kinds"]}
            # `None` is a normal dependency; "build" still ends up influencing
            # the artifact. "dev" alone does not.
            if kinds == {"dev"}:
                continue
            stack.append(dep["pkg"])

    seen.discard(root)
    return sorted((pkgs[p] for p in seen), key=lambda p: (p["name"], p["version"]))


def _texts_in(directory):
    found = {}
    for pattern in LICENSE_GLOBS:
        for path in directory.glob(pattern):
            if not path.is_file():
                continue
            try:
                text = path.read_text(encoding="utf-8").strip()
            except (UnicodeDecodeError, OSError):
                continue
            if text:
                found[path.name] = text
    return found


def license_texts(pkg):
    """Every license-ish file shipped with the package, as (name, text).

    Registry crates are self-contained, but a git dependency is checked out as
    a whole repository, and a workspace member's LICENSE-* usually sit at the
    repo root rather than beside the member's manifest -- osmflat-ext and
    osmflat are both shaped that way. So fall back to walking up, stopping at
    the checkout root (the directory holding `.git`) to avoid wandering into an
    unrelated parent's licenses.
    """
    directory = Path(pkg["manifest_path"]).parent
    for _ in range(4):
        found = _texts_in(directory)
        if found:
            return sorted(found.items())
        if (directory / ".git").exists() or directory.parent == directory:
            break
        directory = directory.parent
    return []


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--target", required=True, help="target triple to filter to")
    ap.add_argument("--features", default="", help="comma-separated cargo features")
    ap.add_argument("-o", "--output", required=True, type=Path)
    args = ap.parse_args()

    meta = cargo_metadata(args.target, args.features)
    pkgs = runtime_packages(meta)

    # text -> {"packages": [...], "name": <file name it came from>}
    by_text = {}
    missing = []
    for pkg in pkgs:
        label = f"{pkg['name']} {pkg['version']}"
        spdx = pkg.get("license") or "(no SPDX expression declared)"
        texts = license_texts(pkg)
        if not texts:
            missing.append((label, spdx, pkg.get("repository")))
            continue
        for fname, text in texts:
            key = hashlib.sha256(text.encode()).hexdigest()
            entry = by_text.setdefault(key, {"text": text, "packages": [], "file": fname})
            entry["packages"].append((label, spdx))

    out = []
    out.append("# Third-party notices")
    out.append("")
    out.append(
        "`osmflat-taginfo` links the Rust crates below. Their licenses require "
        "that these notices travel with a redistributed binary, so this file "
        "ships in every release archive alongside the executable."
    )
    out.append("")
    out.append(
        f"Generated by `scripts/third-party-notices.py` from `Cargo.lock` for "
        f"`{args.target}`"
        + (f" with features `{args.features}`" if args.features else "")
        + f". {len(pkgs)} packages."
    )
    out.append("")
    out.append("This covers dependencies only. `osmflat-taginfo`'s own license is in")
    out.append("`LICENSE-MIT` and `LICENSE-APACHE`, and the OpenStreetMap data it reads")
    out.append("is ODbL -- see the README.")
    out.append("")

    out.append("## Packages")
    out.append("")
    out.append("| Package | License |")
    out.append("| --- | --- |")
    for pkg in pkgs:
        spdx = pkg.get("license") or "see license file"
        out.append(f"| {pkg['name']} {pkg['version']} | {spdx} |")
    out.append("")

    if missing:
        out.append("## Packages shipping no license text")
        out.append("")
        out.append(
            "These declare a license but bundle no copy of it. The SPDX "
            "expression is authoritative; the canonical text for the common "
            "ones is in this archive's `LICENSE-MIT` / `LICENSE-APACHE`."
        )
        out.append("")
        for label, spdx, repo in missing:
            suffix = f" -- {repo}" if repo else ""
            out.append(f"- **{label}**: {spdx}{suffix}")
        out.append("")

    out.append("## License texts")
    out.append("")
    # Stable order: by the first package each text belongs to.
    for entry in sorted(by_text.values(), key=lambda e: e["packages"][0][0]):
        names = sorted({label for label, _ in entry["packages"]})
        # The file name disambiguates a dual-licensed package contributing two
        # texts -- matchit ships MIT and BSD-3-Clause, and two bare `matchit`
        # headings would read like a duplicate.
        heading = f"### {names[0]} -- `{entry['file']}`"
        if len(names) > 1:
            heading += f" (and {len(names) - 1} more)"
        out.append(heading)
        out.append("")
        if len(names) > 1:
            out.append("Applies to:")
            out.append("")
            for n in names:
                out.append(f"- {n}")
            out.append("")
        out.append("```")
        out.append(entry["text"])
        out.append("```")
        out.append("")

    args.output.write_text("\n".join(out) + "\n", encoding="utf-8")
    print(
        f"wrote {args.output}: {len(pkgs)} packages, "
        f"{len(by_text)} distinct license texts, {len(missing)} without text",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
