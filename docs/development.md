# Development

## Contributing

`main` is a protected branch:

- Changes land via pull requests — direct pushes are rejected (enforced for
  admins too).
- The CI checks `test` and `package` must be green before a PR can be merged.
- **All commits must be signed** (GitHub "require signed commits"). Configure
  SSH commit signing so your commits verify:

  ```sh
  git config gpg.format ssh
  git config user.signingkey ~/.ssh/<your-key>.pub
  git config commit.gpgsign true
  ```

  The signing key must be registered on your GitHub account as a *signing* key,
  and your commit email must be a verified GitHub email, otherwise the signature
  will not verify and the push/merge will be blocked.

## Prerequisites

- Rust (stable) — `rustup` recommended.
- [`powerpack-cli`](https://crates.io/crates/powerpack-cli): `cargo install powerpack-cli`.
- [apw](https://github.com/bendews/apw), running and paired, for end-to-end
  manual testing (see the README).
- Alfred 5+ with Powerpack, to actually run the workflow.

## Common tasks

```sh
cargo test --all                          # run the test suite (offline, no apw needed)
cargo clippy --all-targets -- -D warnings # lint (warnings are errors)
cargo fmt --all                           # format

./scripts/build.sh    # build release binary + package the .alfredworkflow
./scripts/deploy.sh   # build + symlink the workflow into Alfred (live dev)
./scripts/deploy.sh --import  # build + open the .alfredworkflow to import it
./scripts/icon.sh     # regenerate workflow/icon.png from assets/icon.svg
```

The workflow icon (`workflow/icon.png`) is a committed asset rasterized from
`assets/icon.svg`. Regenerate it with `scripts/icon.sh` after changing the SVG;
that needs `rsvg-convert` (`brew install librsvg`). It is not regenerated at
build time, so CI has no SVG-conversion dependency.

`scripts/build.sh` syncs `workflow/info.plist`'s `<version>` from `Cargo.toml`
before packaging, so the version shown in Alfred always matches the crate.

## Manual smoke test (without Alfred)

The script filters speak Alfred's JSON on stdout, and `run` reads the Alfred
variables from the environment:

```sh
cargo build

# Accounts for a domain (needs apw running and paired)
./target/debug/alfred-passwords search github.com | python3 -m json.tool

# Domain index: Alfred passes its data folder in alfred_workflow_data; without
# it, nothing is indexed or suggested
export alfred_workflow_data=/tmp/alfred-passwords-data
./target/debug/alfred-passwords search github.com > /dev/null  # indexes github.com
./target/debug/alfred-passwords search gith | python3 -m json.tool  # suggests it
cat /tmp/alfred-passwords-data/domains.json

# Pairing items
./target/debug/alfred-passwords search "pin 123456" | python3 -m json.tool

# Generate items, with the "already exists" check
./target/debug/alfred-passwords generate "github.com me@example.com" | python3 -m json.tool

# Options and configuration (the PWG_* variables of Configure Workflow)
./target/debug/alfred-passwords generate "24 -s +a" | python3 -m json.tool
PWG_STYLE=custom PWG_LENGTH=12 ./target/debug/alfred-passwords generate "" | python3 -m json.tool

# Run an action as Alfred would: generate and copy (concealed) a password.
# Without alfred_workflow_data, the history is off and the Keychain untouched;
# with it, the first run creates the history key in your login Keychain.
unset alfred_workflow_data
action=generate style=custom:32:lds ./target/debug/alfred-passwords run

# Copy a password
action=copy-password domain=github.com username=me@example.com ./target/debug/alfred-passwords run
```

## Testing strategy

All logic is pure and unit-tested; the side effects sit behind traits,
replaced in tests by doubles from `tests/common/mod.rs`: `FakeRunner` (canned
`apw` output keyed by arguments, records every call and its stdin),
`FakeClipboard` (records what was copied and whether it was concealed),
`FakeKeyStore` (a fixed key, or a denied Keychain) and `FakeVault` (an
in-memory history that can fail to load or save). The suite runs offline,
without apw, a browser, iCloud or the Keychain.

| Test file | Covers |
| --- | --- |
| `tests/query_test.rs` | Domain normalisation; `pw` and `pwg` query parsing, pin commands; `pwg` options anywhere, invalid options, how options combine with the configuration. |
| `tests/domains_test.rs` | Domain index: remember / forget / use counts, suggestion ranking (label prefix, frecency, ties, limit), atomic save, domains-only file, malformed file. |
| `tests/apw_test.rs` | The `apw` commands run; JSON decoding; status 3 / 9 / unpaired; missing binary; timeout; password on stdin only; binary resolution. |
| `tests/alfred_test.rs` | JSON shape of items: variables (style spec, history id), ⌘ / ⌥ modifiers, filters, error items, history items; no password in items. |
| `tests/actions_test.rs` | Variables round trip; each action against the fakes; generated passwords kept, copied again from the history; nothing copied when saving or generating fails; a history failure never blocks the copy. |
| `tests/generator_test.rs` | Apple format; custom length, enabled sets, look-alikes; impossible styles; spec round trip; summaries; configuration defaults and parsing. |
| `tests/history_test.rs` | 20-entry cap, newest first, lookup by id, masked preview, age. |
| `tests/vault_test.rs` | `EncryptedFile` with a fake key: round trip, no clear text, mode 0600, fresh nonce, missing file without Keychain access, wrong key / tampered / foreign file unreadable, denied Keychain. |
| `tests/runner_test.rs` | `SystemRunner` against shell scripts: stdout, stdin, failures, missing program, timeout kill. |

`SystemClipboard` and `KeychainKeyStore` are not unit-tested (they touch the
real pasteboard and Keychain); check them with the `run` smoke test above and
from Alfred.

## Project layout

```
.
├── src/                    Rust sources (see docs/architecture.md)
├── tests/                  Integration tests + shared fakes
├── assets/
│   └── icon.svg            Source SVG for the workflow icon
├── workflow/
│   ├── info.plist          Alfred workflow definition (the binary is added at build time)
│   └── icon.png            Workflow icon, rasterized from assets/icon.svg
├── scripts/
│   ├── build.sh            Build + package the .alfredworkflow
│   ├── deploy.sh           Build + install into Alfred
│   └── icon.sh             Regenerate workflow/icon.png from the SVG
├── docs/                   This documentation
├── .github/workflows/      CI and release-please pipelines
├── release-please-config.json
└── .release-please-manifest.json
```

## Editing the Alfred graph

`workflow/info.plist` defines two Script Filters (`pw`, `pwg`), one Run
Script and one notification, plus the Configure Workflow settings (`APW_BIN`,
`PWG_STYLE`, `PWG_LENGTH`, `PWG_LETTERS`, `PWG_DIGITS`, `PWG_SYMBOLS`,
`PWG_SIMILAR`). If you edit the workflow in Alfred's UI while it
is symlinked (`./scripts/deploy.sh`), copy the updated `info.plist` back into
`workflow/` before committing. Validate it with:

```sh
plutil -lint workflow/info.plist
```

To rename a keyword, give its object a new `uid` too (in `objects`,
`connections` and `uidata`). When a workflow is updated, Alfred keeps the
keyword of an object it already knows by `uid`, so a renamed keyword on the
same `uid` never reaches existing installs: renaming `pwgen` to `pwg` kept
`pwgen` until the Script Filter moved from `A2…` to `A5…`.
