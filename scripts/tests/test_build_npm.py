import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class NpmPackagingTests(unittest.TestCase):
    def make_binaries(self, root: Path, targets: dict[str, tuple[str, str, str]]) -> Path:
        binaries = root / "binaries"
        for target, (_, _, executable) in targets.items():
            location = binaries / f"binary-{target}"
            location.mkdir(parents=True)
            (location / executable).write_bytes(target.encode())
        return binaries

    def run_builder(self, binaries: Path, output: Path, targets: list[str]) -> None:
        command = [
            "python3",
            str(ROOT / "packaging/build_npm.py"),
            "0.1.0",
            "--binaries",
            str(binaries),
            "--out",
            str(output),
        ]
        for target in targets:
            command.extend(["--only", target])
        subprocess.run(command, cwd=ROOT, check=True)

    def test_builds_both_front_doors_and_platform_packages(self) -> None:
        targets = {
            "aarch64-apple-darwin": ("darwin", "arm64", "godsdk"),
            "x86_64-apple-darwin": ("darwin", "x64", "godsdk"),
            "x86_64-pc-windows-msvc": ("win32", "x64", "godsdk.exe"),
        }
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binaries = self.make_binaries(root, targets)
            output = root / "packages"
            self.run_builder(binaries, output, list(targets))
            front_doors = [output / "godsdk/package.json", output / "cli/package.json"]
            for path in front_doors:
                package = json.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(package["bin"]["godsdk"], "bin/godsdk.js")
                self.assertEqual(set(package["optionalDependencies"]), {
                    "@godsdk/cli-darwin-arm64",
                    "@godsdk/cli-darwin-x64",
                    "@godsdk/cli-win32-x64",
                })
            order = (output / "publish-order").read_text(encoding="utf-8").splitlines()
            self.assertEqual([Path(path).name for path in order[-2:]], ["godsdk", "cli"])


if __name__ == "__main__":
    unittest.main()
