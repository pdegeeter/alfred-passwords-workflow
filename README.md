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

pwgen github.com me@example.com
 ┌──────────────────────────────────────────────────────────────┐
 │ Generate and save for me@example.com                           │
 │   New account on github.com · the password is copied           │
 │ Generate a strong password                                     │
 │   Copied, kept out of clipboard history · …                    │
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
  `pwgen` shows "Replace the password of …" and only <kbd>⌥</kbd><kbd>↵</kbd>
  replaces it.
- **Generated passwords** follow the format Apple Passwords suggests: three
  groups of six characters (`abcdef-ghjkmN-pqrst2`), one uppercase letter, one
  digit, no ambiguous characters, from the system's secure random source.

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

**Generate**: `pwgen`

- `pwgen` and <kbd>↵</kbd>: copy a new strong password.
- `pwgen <domain> <username>` and <kbd>↵</kbd>: create the account in Apple
  Passwords with a new password, and copy it. For an existing account, use
  <kbd>⌥</kbd><kbd>↵</kbd> to replace its password.

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
