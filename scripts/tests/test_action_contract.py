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
        self.assertIn("steps.generate.outputs.summary", action)
        self.assertIn("steps.install.outputs.generator-version", action)
        self.assertIn("spec-url must use https://", action)
        self.assertIn("output must stay inside the checked-out repository", action)
        self.assertIn("Windows-X64", action)
        self.assertIn("7z x", action)

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
        self.assertIn("concurrency:", workflow)
        self.assertIn("godsdk/update-${{ github.run_id }}", workflow)
        self.assertIn("Summarize generated changes", workflow)
        self.assertIn("value: ${{ jobs.generate.outputs.changed-files }}", workflow)
        self.assertIn("value: ${{ jobs.generate.outputs.summary }}", workflow)

    def test_reusable_workflow_gates_generated_output_with_godsuite(self) -> None:
        workflow = (ROOT / ".github/workflows/generate-sdk.yml").read_text()
        for input_name, default in {
            "godlint-version": "0.7.0",
            "godharness-version": "0.1.6",
        }.items():
            self.assertGreaterEqual(workflow.count(f"{input_name}:"), 2)
            self.assertIn(f"default: {default}", workflow)
        self.assertEqual(workflow.count("tomerwave/godlint@"), 2)
        self.assertEqual(workflow.count("Run Godharness on generated repository"), 1)
        self.assertEqual(workflow.count("Run Godharness before commit"), 1)
        self.assertEqual(workflow.count("sha256sum --check \"$asset.sha256\""), 2)
        self.assertEqual(workflow.count("gh release download \"v$VERSION\" --repo tomerwave/godharness"), 2)
        self.assertEqual(workflow.count("working-directory: ${{ inputs.output }}"), 4)

    def test_fixture_starter_references_the_pinned_reusable_workflow(self) -> None:
        starter = ROOT / "fixtures" / "action-starter" / ".github" / "workflows" / "generate-sdk.yml"
        workflow = starter.read_text()
        self.assertIn("workflow_dispatch", workflow)
        self.assertIn("tomerwave/godsdk/.github/workflows/generate-sdk.yml@", workflow)
        self.assertIn("spec-path: api/openapi.yaml", workflow)
        self.assertIn("targets: rust,typescript,python", workflow)


if __name__ == "__main__":
    unittest.main()
