#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import re
from pathlib import Path


TARGETS = {
    "aarch64-apple-darwin": ("macos", "arm"),
    "x86_64-apple-darwin": ("macos", "intel"),
    "aarch64-unknown-linux-gnu": ("linux", "arm"),
    "x86_64-unknown-linux-gnu": ("linux", "intel"),
}


def archive_data(version: str, assets: Path, target: str) -> tuple[str, str]:
    filename = f"godsdk-{version}-{target}.tar.gz"
    archive = assets / filename
    checksum = assets / f"{filename}.sha256"
    if not archive.is_file() or not checksum.is_file():
        raise ValueError(f"missing release assets for {target}")
    expected = checksum.read_text(encoding="utf-8").split()[0]
    actual = hashlib.sha256(archive.read_bytes()).hexdigest()
    if expected != actual:
        raise ValueError(f"checksum mismatch for {filename}")
    return filename, expected


def render(version: str, assets: Path) -> str:
    data = {target: archive_data(version, assets, target) for target in TARGETS}
    lines = [
        "class Godsdk < Formula",
        '  desc "Technical SDK generator for the Godsuite"',
        '  homepage "https://github.com/tomerwave/godsdk"',
        f'  version "{version}"',
        "",
        "  on_macos do",
        "    if Hardware::CPU.arm?",
        formula_url(data["aarch64-apple-darwin"], version),
        "    else",
        formula_url(data["x86_64-apple-darwin"], version),
        "    end",
        "  end",
        "",
        "  on_linux do",
        "    if Hardware::CPU.arm?",
        formula_url(data["aarch64-unknown-linux-gnu"], version),
        "    else",
        formula_url(data["x86_64-unknown-linux-gnu"], version),
        "    end",
        "  end",
        "",
        "  def install",
        '    bin.install "godsdk"',
        "  end",
        "end",
        "",
    ]
    return "\n".join(lines)


def formula_url(data: tuple[str, str], version: str) -> str:
    filename, checksum = data
    return f'      url "https://github.com/tomerwave/godsdk/releases/download/v{version}/{filename}"\n      sha256 "{checksum}"'


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("version")
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"\d+\.\d+\.\d+", args.version):
        raise SystemExit("version must be semver without a leading v")
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(render(args.version, args.assets), encoding="utf-8")


if __name__ == "__main__":
    main()
