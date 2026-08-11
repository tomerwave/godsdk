# GodSDK action starter

This fixture is the smallest repository shape used to exercise the reusable
generation workflow. A real generated repository receives the complete SDK
tree; this starter only supplies the checked-in OpenAPI source and workflow
entry point.

Run it manually from the Actions tab in `dry-run` mode first. Set `commit: true`
only after reviewing the generated artifact; commit mode pushes an isolated
`godsdk/update-<run-id>` branch for pull-request review.
