from pathlib import Path
import unittest


ROOT = Path(__file__).parents[2]


class ActionContractTests(unittest.TestCase):
    def test_composite_action_requires_integrity_for_remote_specs(self) -> None:
        action = (ROOT / "action.yml").read_text()

        self.assertIn("spec-sha256:", action)
        self.assertIn("spec-sha256 is required when spec-url is used", action)
        self.assertIn("sha256sum --check", action)
        self.assertIn("--remote-ref-host", action)
        self.assertIn("--remote-ref-pin", action)
        self.assertIn("id: generate", action)
        self.assertIn("steps.generate.outputs.changed-files", action)

    def test_reusable_workflow_exposes_the_same_security_inputs(self) -> None:
        workflow = (ROOT / ".github/workflows/generate-sdk.yml").read_text()

        for input_name in (
            "spec-url",
            "spec-sha256",
            "remote-ref-hosts",
            "remote-ref-pins",
            "generator-version",
        ):
            self.assertGreaterEqual(workflow.count(f"{input_name}:"), 2)
            self.assertIn(f"{input_name}: ${{{{ inputs.{input_name} }}}}", workflow)

        self.assertIn("contents: read", workflow)
        self.assertIn("contents: write", workflow)


if __name__ == "__main__":
    unittest.main()
