from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures" / "openapi"


class FixtureCorpusTests(unittest.TestCase):
    def test_valid_fixtures_are_openapi_31_and_have_operations(self) -> None:
        valid = sorted(path for path in FIXTURES.glob("*.yaml"))
        self.assertGreaterEqual(len(valid), 6)

        for path in valid:
            text = path.read_text(encoding="utf-8")
            self.assertIn("openapi: 3.1.1", text, path)
            self.assertIn("operationId:", text, path)
            self.assertIn("paths:", text, path)

    def test_corpus_contains_required_behavior_categories(self) -> None:
        names = {path.name for path in FIXTURES.glob("*.yaml")}
        self.assertTrue({"minimal-3.1.yaml", "minimal-3.1-changed-operation.yaml"} <= names)
        self.assertIn("parameters-and-errors-3.1.yaml", names)
        self.assertIn("schemas-composition-3.1.yaml", names)
        self.assertIn("security-3.1.yaml", names)
        self.assertIn("refs-3.1.yaml", names)

    def test_external_reference_fixture_is_paired_with_referenced_document(self) -> None:
        entrypoint = (FIXTURES / "refs-3.1.yaml").read_text(encoding="utf-8")
        referenced = (FIXTURES / "refs" / "models.yaml").read_text(encoding="utf-8")
        self.assertIn("./refs/models.yaml#", entrypoint)
        self.assertIn("components:", referenced)

    def test_invalid_fixture_is_explicitly_outside_valid_corpus(self) -> None:
        invalid = FIXTURES / "invalid" / "missing-path-parameter-3.1.yaml"
        text = invalid.read_text(encoding="utf-8")
        self.assertIn("/users/{user_id}", text)
        self.assertNotIn("in: path", text)


if __name__ == "__main__":
    unittest.main()
