# Alfred Passwords Workflow

An [Alfred](https://www.alfredapp.com/) workflow, written in Rust, to use the
macOS **Passwords** app (iCloud Keychain) from Alfred: search an account,
copy its password or username, and generate a strong password, optionally
saving it.

```
pw github.com
 ┌──────────────────────────────────────────────────────────────┐
 │ me@example.com                                                 │
 │   github.com · ↵ copy password · ⌘↵ copy username              │
 │ ci-bot                                                         │
 │   github.com · CI robot · ↵ copy password · ⌘↵ copy username   │
 └──────────────────────────────────────────────────────────────┘

pwg 24 -s
 ┌──────────────────────────────────────────────────────────────┐
 │ Generate a strong password                                     │
 │   24 characters · Aa 345 · no look-alikes · copied, …          │
 │ bevf****                                                       │
 │   Generated 5 min ago · Apple format · saved for me (github…)  │
 │ Xy7@****                                                       │
 │   Generated yesterday · 16 characters · Aa 345 @&$ · …         │
 └──────────────────────────────────────────────────────────────┘
```

## How it works

The Passwords app has no public API, and `security` cannot read iCloud
Keychain entries. The only way in is `PasswordManagerBrowserExtensionHelper`,
the helper behind Apple's browser extension. macOS only lets a browser start
it: launched from anything else (a shell, Python, a Rust binary), it is killed
with `SIGKILL (Launch Constraint Violation)`.

So this workflow talks to [apw](https://github.com/bendews/apw), which runs a
headless Chromium with Apple's extension behind a small daemon, and exposes it
as a CLI. The workflow calls `apw pw list|get|save` and `apw auth`, and decodes
their JSON.

- **Search is by domain.** The helper only answers "which accounts exist for
  this URL", so you type a domain (`github.com`), not a free-text name. Extra
  words filter the usernames and titles: `pw github.com work`.
- **Known domains autocomplete.** Since there is no way to list every
  account, the workflow remembers the domains that returned accounts and
  suggests them while you type (`pw git` → `github.com`), most used first.
  Only domain names are stored (`domains.json` in the workflow's data folder),
  never usernames or passwords. A domain shows up after its first full lookup.
- **Secrets never go through Alfred.** The password is fetched (or generated)
  by the workflow binary and written to the clipboard directly, tagged
  `org.nspasteboard.ConcealedType` so clipboard histories (Alfred's included)
  skip it. Alfred only receives the notification text.
- **Saving never overwrites by accident.** When the account already exists,
  `pwg` shows "Replace the password of …" and only <kbd>⌥</kbd><kbd>↵</kbd>
  replaces it.
- **Generated passwords** come from the system's secure random source, in
  one of two styles:
  - **Apple**, the format Apple Passwords suggests: three groups of six
    characters (`bevfzw-yuzdmg-edx4Fe`), one uppercase letter, one digit, no
    ambiguous characters;
  - **Custom**: 4 to 40 characters, with letters (`Aa`), digits (`345`),
    symbols (`@&$!#?`) and look-alike characters (`1l| O0 Z2`) each on or
    off. Every enabled set appears at least once.
- **The last 20 generated passwords** are kept so `pwg` can copy one again.
  They are stored encrypted (ChaCha20-Poly1305) in `history.bin` in the
  workflow's data folder, with a random key kept in your login Keychain, so a
  copy of the file (backup, sync) is useless on its own. Alfred only ever shows
  the first four characters (`bevf****`).

## Requirements

- macOS (Apple Silicon / arm64), with the Passwords app signed in to iCloud
  Keychain
- [Alfred 5+](https://www.alfredapp.com/) with the **Powerpack** license
- Google Chrome, Brave, Microsoft Edge or Ungoogled Chromium, with the
  [iCloud Passwords extension](https://chromewebstore.google.com/detail/icloud-passwords/pejdijmoenmkgeppbflobdenhhabjlaj)
  installed (apw copies it into its own headless profile)
- [apw](https://github.com/bendews/apw):

  ```sh
  brew install bendews/homebrew-tap/apw
  brew services start apw
  ```

  apw's browser runs headless: use a browser you don't browse with, or close it
  first.

## Install

Download the latest `alfred-passwords.alfredworkflow` from the
[Releases](https://github.com/pdegeeter/alfred-passwords-workflow/releases)
page and double-click it to import into Alfred.

Or build and deploy from source:

```sh
cargo install powerpack-cli   # one-time
./scripts/deploy.sh           # build + symlink into Alfred
```

If apw is not installed by Homebrew, set its path in the workflow's
configuration (**apw binary**).

## Configuration

In Alfred, open the workflow and click **Configure Workflow**:

| Setting | Default | |
| --- | --- | --- |
| apw binary | Homebrew's | Path to `apw` |
| Password style | Apple | Apple (`xxxxxx-xxxxxx-xxxxxx`) or Custom |
| Custom length | 20 | 4 to 40 |
| Custom characters: letters (e.g. Aa) | on | |
| Digits (e.g. 345) | on | |
| Symbols (@&$!#?) | on | |
| Look-alike characters (e.g. 1l\| O0 Z2) | off | |

Alfred cannot hide settings depending on another one, so the custom settings
stay visible with the Apple style: they are ignored then. They still matter
for the options typed in `pwg`: `pwg -s` with the Apple style builds a custom
password from them, without symbols.

The first time `pwg` keeps a password, macOS asks to let `alfred-passwords`
use its Keychain item: choose **Always Allow**. The binary is not signed, so
macOS asks again after each update of the workflow.

## Usage

**Pair apw** once per daemon start (after each login or reboot):

1. `pw pin` and <kbd>↵</kbd>: macOS shows a 6-digit PIN.
2. `pw pin 123456` and <kbd>↵</kbd>.

When apw is not paired, any search offers the first step.

**Search**: `pw <domain> [filter]`. While the query is not a full domain
yet, `pw` lists the known domains that match: <kbd>↵</kbd> or <kbd>⇥</kbd>
completes it. `pw` alone lists the most used ones. On an account:

- <kbd>↵</kbd> copies the password (macOS may ask for Touch ID),
- <kbd>⌘</kbd><kbd>↵</kbd> copies the username,
- <kbd>⌘</kbd><kbd>C</kbd> also copies the username.

**Generate**: `pwg`

- `pwg` and <kbd>↵</kbd>: copy a new strong password in the configured
  style. Below, the last 20 generated passwords (`bevf****`, when, which
  style, which account): <kbd>↵</kbd> copies one again.
- `pwg <domain> <username>` and <kbd>↵</kbd>: create the account in Apple
  Passwords with a new password, and copy it. For an existing account, use
  <kbd>⌥</kbd><kbd>↵</kbd> to replace its password.
- Options change one generation, anywhere in the query:

  | Option | Effect |
  | --- | --- |
  | `4` … `40` | Custom password of that length |
  | `apple` | Apple format |
  | `+l` / `-l` | With / without letters |
  | `+d` / `-d` | With / without digits |
  | `+s` / `-s` | With / without symbols |
  | `+a` / `-a` | With / without look-alike characters |

  A length or a `±` option switches to a custom password built on the
  configured rules, even when the default style is Apple. Examples:
  `pwg 32`, `pwg 16 -s`, `pwg apple`, `pwg 24 +a github.com me`.

## Documentation

- [Architecture](docs/architecture.md): components, data flow, design
  decisions.
- [Development](docs/development.md): build, test, lint, deploy locally.
- [Release process](docs/release.md): release-please and the CI/CD pipeline.

## Credits

[apw](https://github.com/bendews/apw) by Ben Dews does the hard part: driving
Apple's extension and its helper.

## License

MIT
