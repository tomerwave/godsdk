# Security policy

## Supported versions

GodSDK is pre-alpha and no release is currently supported. Security fixes will be published for
the latest released version once releases begin.

## Reporting a vulnerability

Please do not open a public issue for a suspected vulnerability. Use GitHub private vulnerability
reporting when enabled. If it is unavailable, contact the maintainers through the email address
listed in the repository profile and include `GodSDK security report` in the subject.

Include a clear description, reproduction steps or proof of concept, impact, and suggested
mitigation. Please avoid sending real credentials or private specifications.

## Security boundaries

The current scaffold does not read specifications or write generated output. Future generation
must preserve these principles:

- local specifications and generated artifacts stay local unless the user explicitly exports
  them;
- remote reference resolution, if supported, must be explicit and constrained;
- generated authentication code must not log tokens or secrets;
- output paths must be validated before any overwrite behavior is introduced;
- adapters and packaging surfaces must be reviewed as part of the product’s security boundary.
