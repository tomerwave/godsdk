import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "update_godsuite.py"
SPEC = importlib.util.spec_from_file_location("update_godsuite", SCRIPT)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("could not load update_godsuite")
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


class UpdatePolicyTests(unittest.TestCase):
    def test_patch_policy_accepts_only_patch_releases(self) -> None:
        self.assertTrue(module.permitted((1, 2, 3), (1, 2, 4), "patch"))
        self.assertFalse(module.permitted((1, 2, 3), (1, 3, 0), "patch"))

    def test_minor_policy_accepts_patch_and_minor_releases(self) -> None:
        self.assertTrue(module.permitted((1, 2, 3), (1, 2, 4), "minor"))
        self.assertTrue(module.permitted((1, 2, 3), (1, 3, 0), "minor"))
        self.assertFalse(module.permitted((1, 2, 3), (2, 0, 0), "minor"))

    def test_major_policy_accepts_a_new_major(self) -> None:
        self.assertTrue(module.permitted((1, 2, 3), (2, 0, 0), "major"))
