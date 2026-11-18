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

# Pairing items
./target/debug/alfred-passwords search "pin 123456" | python3 -m json.tool

# Generate items, with the "already exists" check
./target/debug/alfred-passwords generate "github.com me@example.com" | python3 -m json.tool

# Run an action as Alfred would: generate and copy (concealed) a password
action=generate ./target/debug/alfred-passwords run

# Copy a password
action=copy-password domain=github.com username=me@example.com ./target/debug/alfred-passwords run
```

## Testing strategy

All logic is pure and unit-tested; the side effects sit behind two traits,
replaced in tests by doubles from `tests/common/mod.rs`: `FakeRunner` (canned
`apw` output keyed by arguments, records every call and its stdin) and
`FakeClipboard` (records what was copied and whether it was concealed). The
suite runs offline, without apw, a browser or iCloud.

| Test file | Covers |
| --- | --- |
| `tests/query_test.rs` | Domain normalisation; `pw` and `pwgen` query parsing, pin commands. |
| `tests/apw_test.rs` | The `apw` commands run; JSON decoding; status 3 / 9 / unpaired; missing binary; timeout; password on stdin only; binary resolution. |
| `tests/alfred_test.rs` | JSON shape of items: variables, ⌘ / ⌥ modifiers, filters, error items; no password in items. |
| `tests/actions_test.rs` | Variables round trip; each action against the fakes; nothing copied when saving or generating fails. |
| `tests/generator_test.rs` | Apple format, one uppercase and one digit that never collide, no ambiguous characters, OS randomness. |
| `tests/runner_test.rs` | `SystemRunner` against shell scripts: stdout, stdin, failures, missing program, timeout kill. |

`SystemClipboard` is not unit-tested (it writes the real pasteboard); check it
with the `run` smoke test above.

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

`workflow/info.plist` defines two Script Filters (`pw`, `pwgen`), one Run
Script and one notification. If you edit the workflow in Alfred's UI while it
is symlinked (`./scripts/deploy.sh`), copy the updated `info.plist` back into
`workflow/` before committing. Validate it with:

```sh
plutil -lint workflow/info.plist
```
