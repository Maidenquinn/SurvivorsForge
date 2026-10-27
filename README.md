# SurvivorsForge

A desktop mod manager for the addons of [Azeroth Survivors](https://wow.aethus.pl), a private World of Warcraft 1.12.1 instance. It installs, updates and removes addons in the game's `AddOns` folder, in the spirit of CurseForge but scoped to this one game.

Addon sources:

- **Verified addons** from a central JSON list (`registry/verified.json`), so the list can change without a new release.
- **GitHub repos** added by hand. SurvivorsForge detects whether a repo is an addon and tracks it for updates.
- **Local zip files**, installed or updated by hand.

Built with Rust and [Slint](https://slint.dev).

## Requirements

- Rust (stable, 2021 edition). Developed against 1.98.
- Windows 10/11 (primary target). Linux builds should work, but nobody tests them.

## Build and run

```bash
cargo run
```

```bash
cargo build --release
```

The release binary is `target/release/SurvivorsForge.exe`. It is a GUI app and opens no console window.

On first start the app asks for the `AddOns` folder, e.g. `<game>\benilla-config\AddOns`.

## Tests

```bash
cargo test
```

```bash
cargo clippy --all-targets -- -D warnings
```

Tests run offline. Tests that need the network are marked `#[ignore]`:

```bash
cargo test -- --ignored
```

To try the app against a copy of an AddOns folder without touching your real settings, point the data folder somewhere else:

```bash
SURVIVORSFORGE_DATA_DIR=/path/to/test-data cargo run
```

## Project layout

| Path | Purpose |
|---|---|
| `src/main.rs` | UI glue: callbacks, background threads, window frame |
| `src/lib.rs` | Shared `Error` type, constants (`APP_REPO`, `APP_VERSION`), small helpers |
| `src/config.rs` | `config.json`, data folder, AddOns folder validation |
| `src/toc.rs` | `.toc` parser (`## Key: Value`, color code stripping) |
| `src/scan.rs` | Scans the AddOns folder for installed addons |
| `src/archive.rs` | Finds addon folders inside a zip, extracts them safely |
| `src/state.rs` | `state.json`: which addon came from which source in which version |
| `src/install.rs` | Atomic install/update, uninstall to the recycle bin |
| `src/registry.rs` | Verified list: load order, validation, asset glob matching |
| `src/github.rs` | GitHub client on github.com web endpoints, the `GitHub` trait |
| `src/updates.rs` | Resolves the remote version per source, decides "update available" |
| `src/repo_detect.rs` | Detection for manually added repos |
| `src/controller.rs` | Pure view logic: rows, badges, filters, zip install plan |
| `src/self_update.rs` | Updates SurvivorsForge itself from its GitHub releases |
| `ui/*.slint` | Theme tokens, components (title bar, rows, dialogs), main window |
| `registry/verified.json` | The verified addon list |
| `tests/` | Integration tests for zip analysis and installation |

## How it works

### Addon detection

WoW 1.12 loads a folder `<Name>/` that holds `<Name>.toc`. The scanner and the zip analysis follow that rule:

- A zip folder `D` containing `D/D.toc` is an addon named `D`.
- A `.toc` in a folder with another name (GitHub source zips such as `owner-repo-<sha>/BordoUI.toc`) makes that folder an addon named after the `.toc` stem.
- If one folder holds several `.toc` files (`pfUI.toc`, `pfUI-tbc.toc`), the one with `## Interface: 11200` wins, then the shortest name.
- `.toc` files nested inside an addon (libraries) don't count as separate addons.
- Backslash paths in zips are normalized. Entries with `..`, absolute paths or drive letters abort the install (zip slip protection).
- Files directly in the AddOns folder (e.g. `account.txt`) are never read or changed.

### Sources and versions

Each managed addon has a source in `state.json`:

| Source | Update check |
|---|---|
| `verified` | Repo and source spec from the verified list |
| `github` | Repo and source spec stored when the repo was added |
| `zip` | None (manual) |

A source spec says where updates come from:

| Spec | Remote version | Download |
|---|---|---|
| `release` | Tag of the release GitHub marks as "latest" | The asset that matches the `asset` glob (`*` wildcard) |
| `tag` | Highest tag by the numbers in its name (`v2.8.10` before `v2.8.9`) | Source zip of that tag |
| `branch` | Newest commit SHA of the default branch | Source zip of that commit |

Tags across the repos are not uniform (`v3.0.36`, `v2.6.5-azs`, `build-<sha>`). An update is available when the remote id differs from the installed one; there is no semver comparison.

Addons found on disk that match a verified entry are adopted on startup with version `toc:<Version>`. The first update check replaces that with the release tag when both match (ignoring a leading `v`).

### Installation

1. The zip is extracted to `<AddOns parent>/.survivorsforge/staging/<id>/`. That folder is on the same drive, so `rename` is atomic, and outside AddOns, so the game never sees a half-extracted addon.
2. Existing folders of the addon move to `.survivorsforge/backup/<id>/`.
3. The new folders move into AddOns.
4. On any error, everything is moved back. On success, backup and staging are deleted.

An install never overwrites folders that belong to another managed addon (`FolderOwnedBy`). Uninstalling moves folders to the recycle bin. SavedVariables in `WTF` are never touched.

### Adding a GitHub repo

Accepts `owner/repo` or a GitHub URL. Detection order:

1. Each `.zip` asset of the latest release is downloaded and analyzed; the first one with an addon wins (installer zips without an addon are skipped). The asset name becomes a glob by replacing the version with `*` (`LuckyOdds-v0.6.0.zip` becomes `LuckyOdds-v*.zip`).
2. Otherwise the source zip of the newest tag.
3. Otherwise the source zip of the default branch.

Repos that are already in the verified list are refused with a pointer to Browse.

### GitHub access

SurvivorsForge does not use the GitHub REST API. Without a token it allows 60 requests per hour, and a single update check needs about one request per addon, so a few app starts would use it up. Everything comes from plain github.com endpoints instead, which have no such budget:

| Need | Endpoint |
|---|---|
| Latest release | `https://github.com/{repo}/releases/latest`, read the `Location` of the redirect (`.../releases/tag/{tag}`; no release redirects to `.../releases`) |
| Release assets | `https://github.com/{repo}/releases/expanded_assets/{tag}`, the `href="/{repo}/releases/download/{tag}/{name}"` links in that HTML fragment |
| Tags, default branch, commit SHAs | `https://github.com/{repo}.git/info/refs?service=git-upload-pack`, the ref list `git ls-remote` reads (`symref=HEAD:refs/heads/...` names the default branch) |
| Downloads | Asset URLs and `https://codeload.github.com/{repo}/zip/{ref}` |

github.com answers `401` for a repo that does not exist (it asks for a login), which maps to "Not found on GitHub".

The asset list is read from HTML. If GitHub changes that fragment, `parses_release_assets` in `src/github.rs` fails against a fresh copy of `tests/fixtures/github/assets_survivor_ui.html`, and the app reports "no matching addon zip". The network test checks all endpoints for real:

```bash
cargo test --lib -- --ignored github_live
```

## Verified addon list

`registry/verified.json`, schema version 1:

```json
{
  "schema": 1,
  "addons": [
    {
      "id": "survivor-ui",
      "name": "Survivor UI",
      "author": "morderous",
      "repo": "morderous/survivor-ui",
      "description": "Complete Survivor UI including Survivor Plates (nameplates).",
      "category": "UI",
      "folders": ["SurvivorUI", "SurvivorPlates"],
      "source": { "type": "release", "asset": "SurvivorUI-v*.zip" }
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `id` | Stable key, stored in `state.json`. Never change it for an existing addon. |
| `folders` | Every folder the addon installs. Used for adoption and conflict checks. |
| `source.type` | `release` (needs `asset`), `tag` or `branch` |

Validation rejects unknown schema versions, duplicate ids and folders claimed by two addons.

Load order at runtime:

1. The remote copy from `https://raw.githubusercontent.com/Rezondes/SurvivorsForge/main/registry/verified.json`.
2. The last valid copy in the cache.
3. The copy compiled into the binary.

To change the list for all users, edit the file on `main`. No new release is needed.

## Data locations

| What | Where |
|---|---|
| Settings (`config.json`) | `%APPDATA%\SurvivorsForge\` (Linux: `~/.config/SurvivorsForge/`) |
| Install state (`state.json`) | Same folder, one section per AddOns folder |
| Verified list cache | `cache/verified.json` in the same folder |
| Staging, backup, downloads | `<AddOns parent>\.survivorsforge\` |

`SURVIVORSFORGE_DATA_DIR` overrides the settings folder. A damaged `state.json` is moved to `state.json.bak` and the app starts with an empty state.

## Releases and self-update

`.github/workflows/ci.yml` runs tests and Clippy on every push and pull request.

`.github/workflows/release.yml` runs on tags `v*`:

1. Fails if the tag does not match the `Cargo.toml` version.
2. Builds the release binary.
3. Publishes `SurvivorsForge-<version>-x86_64-pc-windows-msvc.zip` as a GitHub release.

To cut a release: bump `version` in `Cargo.toml`, commit, then tag and push:

```bash
git tag v0.2.0
```

```bash
git push origin v0.2.0
```

On startup the app checks the latest SurvivorsForge release. If its version is higher (semver), a banner offers "Update and restart". It downloads the zip, swaps the running executable with `self-replace` and starts the new one.

## Window frame

The window runs with `no-frame`. Title bar, caption buttons and resize edges are Slint components (`ui/components.slint`). Moving and resizing go through winit (`drag_window`, `drag_resize_window`), which needs the Slint feature `unstable-winit-030`. When you upgrade Slint, check that this feature still exists.
