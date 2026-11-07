# Agent notes: odoo-macmini-core2duo

`odoo-cli` is a single static Rust binary for reading and changing Odoo from the
shell. Its main users are Claude Code threads on this machine (an Intel Core 2
Duo P8600 Mac mini running Linux Mint 22.3). Read `README.md` for usage and
`docs/SPEC.md` for requirements and Odoo version differences.

## Why it exists

- The owner pays for Pantalytics' Odoo MCP connector and uses it in the Claude
  web UI only. This CLI replaces it inside Claude Code, where running an MCP
  server on this weak machine makes no sense.
- Key need: agents must be able to see **images attached to Odoo messages**
  (chatter and Discuss). Pantalytics' connector cannot do this.
- Accuracy matters more than features: agents act on this output.

## Build, test, install

Rust is installed per-user in `~/.cargo` (not on PATH by default).

```bash
~/.cargo/bin/cargo test                                                    # unit tests
~/.cargo/bin/cargo build --release --target x86_64-unknown-linux-musl      # ~1-2 min here
install -m 0755 target/x86_64-unknown-linux-musl/release/odoo-cli ~/.local/bin/odoo-cli
```

- `.cargo/config.toml` sets `CC_x86_64_unknown_linux_musl = "gcc"` so `ring`
  builds for musl without a musl toolchain. Keep it.
- `docs/AGENT_GUIDE.md` is compiled into the binary (`odoo-cli guide`).
  Rebuild after editing it.
- `skill/SKILL.md` is installed at `~/.claude/skills/odoo/SKILL.md`. Copy it
  again after changing it.
- Keep the release build for baseline x86-64: no `target-cpu` flags in CI. The
  CPU has no POPCNT, SSE4.2, AVX or AES-NI.

## Release

Push a `vX.Y.Z` tag. `.github/workflows/release.yml` tests, builds the static
binary and attaches `odoo-cli-x86_64-linux` + `.sha256` to the GitHub release.
Bump `version` in `Cargo.toml` first.

## Git rules

- **Every commit and tag must be GPG-signed** with key `FA3117A12432BDB3`
  (agustinustheoo@gmail.com). It is the global default and signs without a
  prompt. Never use `commit.gpgsign=false` or other bypasses; if signing
  fails, fix the config or ask the owner.
- Tags so far are lightweight (`git -c tag.gpgsign=false tag vX.Y.Z` or
  `git update-ref`); signed annotated tags are fine too.
- The repo is **private**. Do not make it public or distribute the binary
  without reviewing `docs/PROVENANCE.md` (reference projects use ELv2,
  MPL-2.0, BSD and AGPL licenses; no code from them is in this repo; keep it
  that way: implement from Odoo's documented API and observed behaviour).

## Testing against Odoo

- **Owner's instance:** `default` profile (URL and database: `odoo-cli whoami`),
  **Odoo 19.0+e**, JSON-2. Real company data: read freely, but do
  **not** write, post messages or delete there for testing. Ask first.
- **Disposable Odoo 20 for write tests:** Odoo's public demo. Get a database
  with XML-RPC `start` on `https://demo.odoo.com/start` (returns host,
  database, admin/admin). Create an RPC-scoped API key by logging in via
  `/web/session/authenticate`, creating `res.users.apikeys.description`
  (`scope: "rpc"`, `duration: "1"`), calling `make_key`, then
  `res.users.identitycheck.run_check` with `context: {"password": "admin"}`;
  the key is in the returned action's `context.default_key`. Use a separate
  config so the real profile is untouched:
  `XDG_CONFIG_HOME=/tmp/x/config XDG_CACHE_HOME=/tmp/x/cache odoo-cli setup ...`.
- The network here is flaky (occasional DNS `Try again`, TLS timeouts); retry
  before debugging.

## Verified behaviour (do not regress)

| Area | Odoo 19 (owner) | Odoo 20 (demo) |
|---|---|---|
| setup, whoami, search, get, fields, models, modules, ref | yes | yes |
| `tasks --open` (owner's board finishes in an unfolded "Done" stage) | yes, 26 open = per-stage counts | yes |
| aggregate (`formatted_read_group` on 19, new `read_group` on 20) | yes | yes |
| `access` (`has_access`) | yes | yes |
| attachment download (`datas` on 19, `raw` dict on 20) | yes, real screenshot | yes, byte-identical |
| create/update/delete, chatter post/note, `--attach`, `--html`, timesheet log, invoice create + `action_post` | not tested (real data) | yes |
| Odoo 18 and older (JSON-RPC path) | untested | untested |

## Known gaps / ideas

- JSON-RPC path (Odoo ≤18) has never run against a live server.
- `chat` cannot open a new direct-message conversation; it can only post to
  channels/DMs that already exist (`chat channels` lists them).
- Plain-text bodies on Odoo ≤17 are escaped; `--html` needs 18+.
- No CSV export or external-id import (`load`) yet.
- The `--open` heuristic matches stage names Done*/Cancel*/Closed*/Complete*;
  it reports what it excluded in `excluded_stages`.

## Code map

- `src/client.rs`: transports (JSON-2, JSON-RPC), error mapping, exit codes.
- `src/schema.rs`: `fields_get` cache (24h), field validation, suggestions.
- `src/attachments.rs`: message/record files, uploads, image fields, sniffing.
- `src/config.rs`: profiles; key in `~/.config/odoo-cli/<profile>.key` (0600).
- `src/main.rs`: CLI (clap) and commands.
