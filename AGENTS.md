# AGENTS.md

Guidance for coding agents working in this repository.

## Keep everything in sync (always)

Every code change must update its tests, the docs under `docs/`, the
`README.md`, and this file in the **same** change — never as a follow-up. If you
change behaviour (queries, actions, the `apw` invocation, the plist, the
release flow), fix every reference to the old behaviour before committing.
`cargo test --all`, `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --all` must all pass.

## What this is

An Alfred (macOS) workflow written in Rust that uses Apple Passwords through
[apw](https://github.com/bendews/apw). One binary, `alfred-passwords`:

- `search [query]` — Script Filter, keyword `pw`: accounts for a domain, the
  known domains while the query is not one yet, and the `pin` pairing items.
- `generate [query]` — Script Filter, keyword `pwg`: generate a password in
  the configured style (`PWG_*` variables) or with the options typed in the
  query (`24`, `apple`, `±l ±d ±s ±a`), optionally save it for
  `<domain> <username>`, and list the last 20 generated passwords.
- `run` — Run Script: reads the item's action from the Alfred variables
  (`action`, `domain`, `username`, `pin`, `style`, `id`) in the environment,
  performs it, and prints the notification text.

Why apw and not a native client: macOS kills `PasswordManagerBrowserExtensionHelper`
(launch constraint) unless a browser starts it. See `docs/architecture.md`.

## Commands

```sh
cargo test --all                          # full suite (offline, no apw needed)
cargo test --test apw_test                # one test file
cargo clippy --all-targets -- -D warnings # lint; warnings are errors (CI gate)
cargo fmt --all                           # format (CI checks with --check)

./scripts/build.sh            # sync plist version + build release + package .alfredworkflow
./scripts/deploy.sh           # build + symlink workflow into Alfred (live dev)
```

`powerpack-cli` is required for build/deploy/packaging: `cargo install powerpack-cli`.

## Rules that matter here

- **Secrets never go through Alfred.** Items carry domain and username only;
  `run` fetches or generates the password and writes it to the clipboard
  itself, concealed (`org.nspasteboard.ConcealedType`). Never put a password in
  an item's `arg`, `variables`, subtitle or the notification. History items
  carry an entry id and show `History::preview` (four characters + `****`).
- **The password history is encrypted** (`src/vault.rs`): ChaCha20-Poly1305 in
  `history.bin`, key in the login Keychain, never in the workflow
  configuration. Never write a generated password anywhere else. A history
  failure must not block the copy, and a denied Keychain must never lead to
  overwriting the file (only an `Unreadable` one is replaced).
- **Passwords go to apw on stdin** (`apw pw save --stdin`), never in `argv`.
- **Side effects behind traits.** Spawning processes goes through
  `runner::CommandRunner`, the clipboard through `clipboard::Clipboard`, the
  Keychain through `vault::KeyStore` and the history storage through
  `vault::HistoryVault`. Tests use `FakeRunner`, `FakeClipboard`,
  `FakeKeyStore` and `FakeVault` from `tests/common/mod.rs`. Keep new logic
  pure.
- **The style travels as a spec.** The item carries `Style::spec()`; the
  action decodes it with `Style::from_spec`, so what is generated is what the
  subtitle announced. Keep both symmetrical (a test round-trips them).
- **The domain index stores domains only** (`domains.json` in
  `alfred_workflow_data`, see `src/domains.rs`). Never add usernames or
  anything secret to it. It is best-effort: a missing or malformed file is
  empty, a save failure is only logged to stderr.
- **Script filters never fail.** An `ApwError` becomes an item explaining the
  fix (`alfred::error`).
- **No `uid` on items**, and `alfredfiltersresults = false`: ordering and
  filtering are done in Rust.
- **Actions are encoded in one place**: `Action::variables` and
  `Action::from_variables` must stay symmetrical (a test round-trips every
  action). A modifier that triggers an action needs a matching connection with
  that modifier in `workflow/info.plist` (⌘ = 1048576, ⌥ = 524288).
- **Overwrites need ⌥.** `pwg <domain> <username>` on an existing account
  must not save on a plain ↵.
- **Editing `workflow/info.plist`**: escape `<`/`>` in strings, validate with
  `plutil -lint workflow/info.plist`. When updating an installed workflow,
  Alfred keeps the keyword of an object whose `uid` it already knows: to
  rename a keyword, also give its object a new `uid` (connections and
  `uidata` included), as `pwgen` → `pwg` did (`A2…` → `A5…`). The binary in `workflow/` is a build
  artifact (gitignored).
- **Icon**: `workflow/icon.png` is generated from `assets/icon.svg` by
  `scripts/icon.sh`. Edit the SVG, not the PNG.
- **Protected `main`**: PRs only (enforced for admins), CI `test` and
  `package` green, signed commits.

## Releases

release-please (`release-type: rust`) on Conventional Commits. Merging the
Release PR tags `vX.Y.Z` and attaches the `.alfredworkflow` (arm64) to the
GitHub Release. Never bump versions by hand. See `docs/release.md`.
