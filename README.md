# odoo-macmini-core2duo

`odoo-cli`: a single static binary for reading and changing Odoo from the
shell, built for an old Mac mini (Intel Core 2 Duo P8600) running Linux, and
meant to be driven by Claude Code threads as much as by hand.

- No MCP server, no daemon, no Python, no Node, no OpenSSL. One 3 MB file.
- About 3.5 MB of RAM per call. No async runtime; blocking HTTP, gzip.
- JSON output and stable exit codes for agents.
- Field names checked against the live server before anything is sent.
- Images and files attached to messages download to disk so an agent can
  look at them.
- Odoo 19+ over JSON-2; older versions over JSON-RPC. Tested live on Odoo 20.

Private, personal use. See [docs/PROVENANCE.md](docs/PROVENANCE.md) before
distributing.

## Install (download the binary)

From a GitHub release:

```bash
gh release download --repo agustinustheo/odoo-macmini-core2duo --pattern 'odoo-cli-x86_64-linux' -O ~/.local/bin/odoo-cli
chmod +x ~/.local/bin/odoo-cli
```

Or build it here (needs Rust; about 2 minutes on the Core 2 Duo):

```bash
cargo build --release --target x86_64-unknown-linux-musl
install -m 0755 target/x86_64-unknown-linux-musl/release/odoo-cli ~/.local/bin/
```

## Set up

1. In Odoo: avatar → **My Profile / Preferences** → **Account Security** →
   **New API Key**. On Odoo 20 choose the **RPC** scope.
2. Run, and paste the key when asked:

```bash
odoo-cli setup --url https://YOURCOMPANY.odoo.com --db YOURDB --login you@example.com
```

The key is stored in `~/.config/odoo-cli/default.key` (mode 0600). No
encryption: anyone who can read that file could also run this binary.

## Use

```bash
odoo-cli whoami
odoo-cli tasks --mine --open
odoo-cli chatter read project.task 42 --download images
odoo-cli timesheet log --task 42 --hours 1.5 -d "Code review"
odoo-cli guide          # every recipe: tasks, projects, finance, messages, users
odoo-cli --help
```

## For Claude Code

Copy the skill so every Claude Code thread on this machine finds the tool:

```bash
mkdir -p ~/.claude/skills/odoo && cp skill/SKILL.md ~/.claude/skills/odoo/
```

## Layout

- `src/client.rs`: JSON-2 and JSON-RPC transport, error mapping.
- `src/schema.rs`: `fields_get` cache, field validation and suggestions.
- `src/attachments.rs`: message and record files, uploads, image fields.
- `src/config.rs`: profiles and API key storage.
- `src/main.rs`: commands.
- `docs/AGENT_GUIDE.md`: printed by `odoo-cli guide`.
- `docs/SPEC.md`: requirements and Odoo version differences.
- `docs/PROVENANCE.md`: projects consulted and their licenses.
