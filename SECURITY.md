# Security Policy

Riff is in early development and does not yet ship Spotify authentication or playback. Security-sensitive surfaces will grow as those features land.

## Reporting a vulnerability

Please avoid publishing credentials, tokens, session data, or exploit details in a public issue.

For now, contact the repository owner through GitHub with a minimal description of the affected component and enough information to reproduce the problem safely. Public disclosure can happen after a fix is available.

## Secrets

Never commit Spotify credentials, access tokens, session blobs, cookies, or local authentication state. Example files and bug reports should use placeholders and redact private values.

## Supported versions

Until Riff has tagged stable releases, security fixes target the latest commit on `main`.
