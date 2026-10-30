# Upstream integration review — 2026-10-30

## Branch safety
- Integration branch: `integration/upstream-2026-10-30`, snapshot of `feat/internal-issues` at `ba6e61ed098bc1dd2eab330bf81cc7a8aab1e1a9`.
- Upstream: `Rezondes/SurvivorsForge@34d7ae46d2f69d51a871cc5da726ee414427b900`.
- Merge base: `3f79640e34d416b215d17d94189025fab5da7e12`.
- Comparison: 37 upstream-only commits, 299 local-only commits at comparison time.
- **No merge has been performed.** This file is a conflict inventory, not a declaration of merge success.

## Changed-file inventory
Upstream changes 65 paths, 20 also modified locally, 45 changed only upstream.

### Shared paths needing three-way resolution
- `crates/forge-api/src/app.rs`
- `crates/forge-api/src/routes/mod.rs`
- `crates/forge-web/src/admin.rs`
- `crates/forge-web/src/app.rs`
- `crates/forge-web/src/templates.rs`
- `crates/forge-web/static/app.css`
- `crates/forge-web/static/view.js`
- `crates/forge-web/templates/admin/addons.html`
- `crates/forge-web/templates/base.html`
- `crates/forge-web/templates/creator/delete.html`
- `crates/forge-web/templates/creator/dependencies.html`
- `crates/forge-web/templates/creator/edit.html`
- `crates/forge-web/templates/creator/images.html`
- `crates/forge-web/templates/creator/members.html`
- `crates/forge-web/templates/creator/my_addons.html`
- `crates/forge-web/templates/creator/nav.html`
- `crates/forge-web/templates/creator/version.html`
- `crates/forge-web/templates/packs/new.html`
- `crates/forge-web/templates/packs/show.html`
- `crates/forge-web/templates/versions.html`

### CRITICAL: migration version collisions
Upstream introduced `0022_refresh_requested.sql` (users.refresh_requested_at) and `0023_pack_latest.sql` (pack_versions.latest).
Our branch already has `0022_internal_issues.sql`, `0023_issue_acknowledgement.sql`, `0024_issue_moderation.sql`, `0025_addon_comments.sql`, `0026_threaded_comments_reports.sql`.
SQLx uses numeric migration versions; duplicate 0022/0023 is not a valid merged migration history.
**Do not blindly rename already-applied local migrations**: that may cause SQLx to attempt reapplication and conflict with `_sqlx_migrations` history.
Safe integration requires checking the local `_sqlx_migrations` versions/checksums and designing an additive versioned migration path. Candidate: retain local applied 0022–0026, introduce upstream schema changes as 0027/0028 only on the integration branch, and ensure original upstream 0022/0023 are not also present. Verify compatibility on fresh DB and DB upgraded from both histories. Upstream PRs should use an upstream-first numbering strategy, distinct from this local integration approach.

## Proposed local merge workflow (do not run blindly)
```powershell
git fetch origin
git remote add upstream https://github.com/Rezondes/SurvivorsForge.git
git fetch upstream main
git switch integration/upstream-2026-10-30
git merge --no-commit --no-ff upstream/main
git status
```
Do not commit until migrations and 20 shared files are resolved. Never use `-X ours` or `-X theirs` globally. Run Rust tests, CI, i18n checker and fresh/existing database migration tests before merging back to `feat/internal-issues`.

## GitHub connector limitation
The available connector can read GitHub resources and write individual Git trees/commits, but it does not expose Git's three-way merge operation. The sandbox has no DNS access to GitHub for `git fetch`. Creating a two-parent commit with an unmerged tree would falsely mark upstream changes as merged and risk losing upstream features. No such commit was created.
