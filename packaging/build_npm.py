#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import shutil
from pathlib import Path


PLATFORMS = {
    "aarch64-apple-darwin": ("darwin", "arm64"),
    "x86_64-apple-darwin": ("darwin", "x64"),
    "aarch64-unknown-linux-musl": ("linux", "arm64"),
    "x86_64-unknown-linux-musl": ("linux", "x64"),
    "x86_64-pc-windows-msvc": ("win32", "x64"),
}
SCOPE = "@godsdk"
REPOSITORY = "https://github.com/tomerwave/godsdk"
DESCRIPTION = "The Godsuite technical SDK generator."
SHIM = Path(__file__).parent / "npm" / "shim.js"


def common(version: str) -> dict[str, object]:
    return {
        "version": version,
        "description": DESCRIPTION,
        "license": "MIT",
        "repository": {"type": "git", "url": f"{REPOSITORY}.git"},
        "homepage": f"{REPOSITORY}#readme",
        "keywords": ["sdk", "generator", "openapi", "godsuite"],
    }


def write(path: Path, contents: dict[str, object]) -> None:
    path.write_text(json.dumps(contents, indent=2) + "\n", encoding="utf-8")


def platform_package(out: Path, version: str, target: str, binary: Path) -> str:
    system, architecture = PLATFORMS[target]
    name = f"{SCOPE}/cli-{system}-{architecture}"
    directory = out / f"cli-{system}-{architecture}"
    executable = "godsdk.exe" if system == "win32" else "godsdk"
    directory.mkdir(parents=True)
    shutil.copy2(binary, directory / executable)
    (directory / executable).chmod(0o755)
    (directory / "README.md").write_text(
        f"# {name}\n\nThe Godsdk binary for {system} {architecture}. "
        f"Install `godsdk` instead of this package.\n",
        encoding="utf-8",
    )
    write(
        directory / "package.json",
        {
            "name": name,
            **common(version),
            "os": [system],
            "cpu": [architecture],
            "files": [executable, "README.md"],
        },
    )
    return name


def front_door(out: Path, version: str, platform_names: list[str], package_name: str) -> None:
    directory_name = "cli" if package_name == f"{SCOPE}/cli" else "godsdk"
    directory = out / directory_name
    (directory / "bin").mkdir(parents=True)
    shutil.copy2(SHIM, directory / "bin" / "godsdk.js")
    (directory / "bin" / "godsdk.js").chmod(0o755)
    shutil.copy2("README.md", directory / "README.md")
    shutil.copy2("LICENSE", directory / "LICENSE")
    write(
        directory / "package.json",
        {
            "name": package_name,
            **common(version),
            "bin": {"godsdk": "bin/godsdk.js"},
            "files": ["bin/godsdk.js", "README.md", "LICENSE"],
            "optionalDependencies": dict.fromkeys(sorted(platform_names), version),
        },
    )


def binary_for(binaries: Path, target: str) -> Path:
    executable = "godsdk.exe" if target.endswith("windows-msvc") else "godsdk"
    candidates = (
        binaries / f"binary-{target}" / executable,
        binaries / target / executable,
    )
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise SystemExit(f"no binary for {target} under {binaries}")


def relative(path: Path) -> str:
    return path.as_posix() if path.as_posix().startswith(".") else f"./{path.as_posix()}"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--binaries", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--only", action="append")
    return parser.parse_args()


def validate_targets(targets: list[str]) -> None:
    unknown = sorted(set(targets) - set(PLATFORMS))
    if unknown:
        raise SystemExit(f"unsupported targets: {', '.join(unknown)}")


def write_publish_order(out: Path, targets: list[str]) -> None:
    order = [
        out / f"cli-{system}-{architecture}"
        for system, architecture in (PLATFORMS[target] for target in targets)
    ] + [out / "godsdk", out / "cli"]
    (out / "publish-order").write_text(
        "".join(f"{relative(directory)}\n" for directory in order), encoding="utf-8"
    )


def main() -> int:
    args = parse_args()
    targets = args.only or list(PLATFORMS)
    validate_targets(targets)
    if args.out.exists():
        shutil.rmtree(args.out)
    names = [
        platform_package(args.out, args.version, target, binary_for(args.binaries, target))
        for target in targets
    ]
    front_door(args.out, args.version, names, "godsdk")
    front_door(args.out, args.version, names, f"{SCOPE}/cli")
    write_publish_order(args.out, targets)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
