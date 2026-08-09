from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSIONS = ROOT / ".github" / "godsuite-versions.yml"
GODLINT_WORKFLOW = ROOT / ".github" / "workflows" / "godlint.yml"
TOOLS = ("godlint", "godharness")
LEVELS = ("patch", "minor", "major")
VERSION_RE = re.compile(r"(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)")


def parse_version(value: str) -> tuple[int, int, int]:
    match = VERSION_RE.fullmatch(value.removeprefix("v"))
    if match is None:
        raise ValueError(f"invalid semantic version: {value}")
    return tuple(int(match.group(name)) for name in ("major", "minor", "patch"))


def permitted(current: tuple[int, int, int], candidate: tuple[int, int, int], level: str) -> bool:
    if level not in LEVELS:
        raise ValueError(f"update level must be one of: {', '.join(LEVELS)}")
    if candidate <= current:
        return False
    if level == "patch":
        return candidate[:2] == current[:2]
    if level == "minor":
        return candidate[0] == current[0]
    return True


def read_versions() -> tuple[str, dict[str, str]]:
    text = VERSIONS.read_text()
    policy_match = re.search(r"^update-policy:\s*(patch|minor|major)\s*$", text, re.MULTILINE)
    if policy_match is None:
        raise ValueError(".github/godsuite-versions.yml has no valid update-policy")
    versions = {}
    for tool in TOOLS:
        match = re.search(rf"^{tool}:\s*(\d+\.\d+\.\d+)\s*$", text, re.MULTILINE)
        if match is None:
            raise ValueError(f".github/godsuite-versions.yml has no {tool} version")
        versions[tool] = match.group(1)
    return policy_match.group(1), versions


def latest_release(tool: str) -> str:
    request = urllib.request.Request(
        f"https://api.github.com/repos/tomerwave/{tool}/releases/latest",
        headers={"Accept": "application/vnd.github+json", "User-Agent": "godsdk-update"},
    )
    with urllib.request.urlopen(request, timeout=20) as response:
        tag = json.load(response)["tag_name"]
    return tag.removeprefix("v")


def replace_version(path: Path, key: str, version: str) -> None:
    text = path.read_text()
    updated, count = re.subn(
        rf"^{key}:\s*\d+\.\d+\.\d+\s*$",
        f"{key}: {version}",
        text,
        count=1,
        flags=re.MULTILINE,
    )
    if count != 1:
        raise ValueError(f"could not update {key} in {path}")
    path.write_text(updated)


def update_tool(tool: str, versions: dict[str, str], level: str) -> bool:
    candidate_text = latest_release(tool)
    current = parse_version(versions[tool])
    candidate = parse_version(candidate_text)
    if not permitted(current, candidate, level):
        return False
    replace_version(VERSIONS, tool, candidate_text)
    versions[tool] = candidate_text
    return True


def sync(level: str | None) -> int:
    configured_level, versions = read_versions()
    selected_level = level or configured_level
    changed = sum(update_tool(tool, versions, selected_level) for tool in TOOLS)
    replace_version(GODLINT_WORKFLOW, "version", versions["godlint"])
    return changed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--level", default="")
    parser.add_argument("--print", dest="print_tool", choices=TOOLS)
    args = parser.parse_args()
    if args.print_tool:
        _, versions = read_versions()
        sys.stdout.write(f"{versions[args.print_tool]}\n")
        return 0
    return sync(args.level)


if __name__ == "__main__":
    raise SystemExit(main())
