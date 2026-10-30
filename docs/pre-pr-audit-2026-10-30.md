# SurvivorsForge pre-PR audit (2026-10-30)

Target: Rezondes/SurvivorsForge main. Working fork: Maidenquinn/SurvivorsForge feat/internal-issues.

## Status: NOT READY TO MERGE

Comparison at audit start: 292 commits ahead, 37 behind upstream main. Do not open a single aggregate PR. Do not rewrite the working branch or run migrations against existing databases without operator approval.

### Delivered feature areas
- Internal issue tracker: issue kinds, status transitions, acknowledgement, author edits, issue comments, soft deletion and moderation audit events.
- Addon screenshot gallery: screenshot ordering and lightbox with dynamic-tab initialization.
- Community comments: create/edit/delete, replies, quote links, reporting and admin moderation.
- Internationalization: 14 locale dictionaries, checker scripts and localized UI templates.

### Audit fixes on working branch
- b21e3fa: blocked addon issue reads; deleted issue comment submission; deleted comment body redaction with tombstones.
- 38825d2: serialize comment-report submissions per reporter so the 10/day limit cannot be raced.
- 522ae63: ensure submitter-aware serialization is used on dynamic comment forms.

### Remaining release gates (mandatory)
1. **Rust build**: cargo fmt --all -- --check; cargo test --workspace; cargo clippy --workspace --all-targets -- -D warnings. Record exact toolchain and outputs.
2. **i18n**: node scripts/check-i18n.mjs and check all 14 locales for new controls.
3. **Migration**: test 0022-0026 on a fresh isolated database and on a copy of a prior schema; verify rollback plan and indexes. Never test destructive migration on production data.
4. **AuthZ matrix**: anonymous, comment author, unrelated authenticated user, addon collaborator, addon owner and site admin. Test every GET/POST/PATCH/DELETE endpoint including direct API access.
5. **Abuse**: 24h caps under concurrency; report duplicates; invalid parent/quote IDs; cross-addon parent references; report reason and detail length; blocked and deleted addons/issues.
6. **Threading**: root pagination vs reply pagination, deleted parent tombstones, deep nesting, quote target across pages, empty state and cursor navigation. Current flat ID pagination is a known functional limitation.
7. **Browser**: first gallery click after dynamic navigation, browser back/forward, history, lightbox keyboard/escape, mobile viewport, comments without JS, errors with JS, post/quote/reply/report flows.
8. **Security**: CSRF and SameSite cookie tests; HTML escaping and safe URLs; permission TOCTOU under concurrent edits/deletes; HTTP security headers; dependency audit.
9. **Review**: upstream changes and merge conflicts; independent PR branches and clean diffs; changelog and screenshots; maintainer acceptance.

### Proposed PR decomposition
- Issues tracker (migrations 0022-0024 and corresponding API/web/UI/tests).
- Screenshot gallery (related UI/JS/CSS and tests).
- Community comments (migrations 0025-0026 and API/web/UI/tests).
- i18n infrastructure (shared scripts, locale dictionaries, localized templates).

Shared files (addon.html, view.js, app.css, app.rs) require manual hunk-level separation and dependency-aware ordering. Copying the entire working branch to each PR would contaminate the diffs.

### Current caveats
- Local Docker and database are not accessible from this GitHub connector. GitHub writes are not equivalent to successful compilation, migration or end-to-end testing.
- No upstream PR opened and no database migration executed.
