# Security Policy

SurvivorsForge installs code from third parties into a game, and the platform scans and verifies that
code. Security reports are taken seriously.

## Reporting a vulnerability

Please do **not** open a public issue. Report it privately through GitHub's
[private vulnerability reporting](https://github.com/Rezondes/SurvivorsForge/security/advisories/new).

Include what you found, how to reproduce it and what an attacker could do with it. You get an answer
as soon as possible, and a fix is released before details are made public.

## Scope

- The SurvivorsForge launcher
- The platform API, website and worker (`crates/`)
- The addon scanner (`crates/forge-scan`): ways to get harmful code past the scanner count as
  vulnerabilities too

Harmful addons themselves are not vulnerabilities in SurvivorsForge. Report them with the report
button on the addon's page.
