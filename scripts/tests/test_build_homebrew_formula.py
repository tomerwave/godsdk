import hashlib
import tempfile
import unittest
from pathlib import Path

from build_homebrew_formula import render


class HomebrewFormulaTest(unittest.TestCase):
    def test_renders_verified_platform_urls(self):
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            for target in (
                "aarch64-apple-darwin",
                "x86_64-apple-darwin",
                "aarch64-unknown-linux-gnu",
                "x86_64-unknown-linux-gnu",
            ):
                archive = assets / f"godsdk-0.1.0-{target}.tar.gz"
                archive.write_bytes(target.encode())
                digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                (assets / f"{archive.name}.sha256").write_text(f"{digest}  {archive.name}\n")

            formula = render("0.1.0", assets)

        self.assertIn('class Godsdk < Formula', formula)
        self.assertIn('version "0.1.0"', formula)
        self.assertEqual(formula.count('sha256 "'), 4)
        self.assertIn('bin.install "godsdk"', formula)


if __name__ == "__main__":
    unittest.main()
