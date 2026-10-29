# Comments + i18n delivery contract

Status: implementation design; NOT shipped or tested.

## Scope and UX acceptance

General addon comments are separate from issue discussions. The Comments tab must load
comments without a full document navigation. The empty state, composer, pending state,
validation errors and success feedback must be polished and accessible. Editing a
comment swaps its text for a prefilled inline editor; cancel restores the text without
a network request. Delete asks for confirmation and updates the list in place.
Display author name, GitHub avatar when available, created/edited timestamps,
and a visible edited marker. Long content wraps safely. Never render user-provided
HTML without sanitization. Keyboard access and reduced motion are mandatory.

## Permissions

Anonymous users may read comments; only authenticated, non-banned users may post.
Comment authors may edit or delete their own comments, subject to moderation policy.
Addon owners and delegated moderators may moderate comments on their addon;
site administrators may moderate any comment. All decisions must be enforced by
forge-api using authenticated identity and addon ownership, not UI flags.
Use soft deletion to preserve moderation audit data. An API caller must not be able
to edit or delete another author's comment by changing identifiers. Rate-limit
writes; bound text length and pagination.

## Data/API plan

Create a dedicated migration for addon_comments, not reuse addon_issue_comments.
Fields: id, addon_id, author_id, body, created_at, edited_at, deleted_at,
moderation metadata. Index by (addon_id, created_at DESC, id DESC).
API routes: GET/POST /v1/addons/{slug}/comments,
PATCH/DELETE /v1/addons/{slug}/comments/{id}. GET must paginate and exclude
deleted content (or display explicit tombstones per moderation decision).
Use existing error conventions and API auth. Test ownership and cross-addon
identifier isolation, deleted addon behavior, empty/oversized text, XSS,
concurrent edits, rate limits and unauthenticated writes.

## Translation architecture

Server templates must receive a locale selected from an explicit user preference
(cookie, validated against a fixed allowlist), with Accept-Language fallback and
English default. Localize document lang, navigation, buttons, validation messages,
status labels, dates and accessible names. Use shared message keys for server-rendered
templates and client-side dynamic fragments; no machine translation at runtime.
Do not translate user-authored addon descriptions or comments. Maintain English
and French dictionaries with parity checks and missing-key tests. Language
selection must not bypass CSRF protection or introduce an open redirect.

## Release gates

1. Add migration and API handlers with permission tests.
2. Add accessible SSR comments fallback, then progressive enhancement.
3. Add locale infrastructure and complete English/French message catalogs.
4. Run cargo fmt, clippy, API/web tests, Docker build, and browser smoke tests.
5. Validate mobile and keyboard UX and preserve local PostgreSQL volumes.
6. No upstream PR until the broader roadmap is complete.
