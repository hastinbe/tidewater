#!/usr/bin/env python3
"""Semantic versioning from conventional commits, for .github/workflows/release.yml (docs/release.md).

    release.py version   print the next version (no "v"), or nothing if no commit since the last v* tag is releasable
    release.py notes     print the release notes (Markdown) for those commits

The version lives only in the tag. Rules: a "!" after the type or a "BREAKING CHANGE:" footer is a major bump (a minor
one while below 1.0), "feat" is minor, "fix" and "perf" are patch, other types release nothing. With no tag at all the
first release is 0.1.0.
"""
import re
import subprocess
import sys

FIRST = (0, 1, 0)
HEADER = re.compile(r"^(?P<type>[a-z]+)(\([^)]*\))?(?P<bang>!)?: (?P<desc>.+)$")
BREAKING = re.compile(r"^BREAKING[ -]CHANGE:", re.M)
SECTIONS = [("breaking", "Breaking changes"), ("feat", "Features"), ("fix", "Fixes"), ("perf", "Performance")]


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def last_tag():
    """The newest plain vX.Y.Z tag reachable from HEAD, or None."""
    out = subprocess.run(
        ["git", "describe", "--tags", "--abbrev=0", "--match", "v[0-9]*.[0-9]*.[0-9]*", "--exclude", "*-*"],
        capture_output=True,
        text=True,
    )
    return out.stdout.strip() or None


def commits(since):
    """(subject, body) of the non-merge commits after `since` (a tag, or None for all of history)."""
    rng = [f"{since}..HEAD"] if since else ["HEAD"]
    raw = git("log", "--no-merges", "--format=%s%x1f%b%x1e", *rng)
    out = []
    for rec in raw.split("\x1e"):
        if rec.strip():
            subject, _, body = rec.partition("\x1f")
            out.append((subject.strip(), body))
    return out


def classify(subject, body):
    """'breaking', 'feat', 'fix', 'perf', or None for a commit that releases nothing."""
    m = HEADER.match(subject)
    if not m:
        return None
    if m["bang"] or BREAKING.search(body):
        return "breaking"
    return m["type"] if m["type"] in ("feat", "fix", "perf") else None


def parse(tag):
    return tuple(int(x) for x in tag.lstrip("v").split("."))


def bump(prev, kinds):
    """The next (major, minor, patch) from the previous one (None: no tag yet) and the commit kinds, or None."""
    kinds = set(kinds)
    if not kinds:
        return None
    if prev is None:
        return FIRST
    major, minor, patch = prev
    if "breaking" in kinds:
        return (major + 1, 0, 0) if major >= 1 else (0, minor + 1, 0)
    if "feat" in kinds:
        return (major, minor + 1, 0)
    return (major, minor, patch + 1)


def notes(items):
    """Markdown release notes grouped by type, from (subject, body) pairs."""
    groups = {k: [] for k, _ in SECTIONS}
    for subject, body in items:
        kind = classify(subject, body)
        if kind:
            groups[kind].append(HEADER.match(subject)["desc"])
    lines = []
    for key, title in SECTIONS:
        if groups[key]:
            lines += [f"## {title}", ""] + [f"- {d}" for d in groups[key]] + [""]
    return "\n".join(lines).strip() + "\n" if lines else "Maintenance release.\n"


def main(argv):
    tag = last_tag()
    items = commits(tag)
    if argv[1:] == ["version"]:
        kinds = [k for k in (classify(*c) for c in items) if k]
        v = bump(parse(tag) if tag else None, kinds)
        print(".".join(map(str, v)) if v else "")
    elif argv[1:] == ["notes"]:
        sys.stdout.write(notes(items))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
