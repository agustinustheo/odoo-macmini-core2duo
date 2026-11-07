# Functional specification

## Purpose

Let Claude Code threads (and the user) read and change Odoo data from a shell on
a low-power machine (Intel Core 2 Duo P8600, 2 cores, no AES-NI, no SSE4.2,
7.5 GB RAM), without running an MCP server or any background process.

## Non-goals

- No MCP server, no daemon, no web UI.
- No hosted or multi-user service.
- Not a replacement for the Pantalytics connector in the Claude web UI.

## Requirements

### Distribution
- R1. One statically linked x86-64 Linux binary (musl). No runtime, no
  interpreter, no package manager, no OpenSSL, no system CA store needed.
- R2. Runs on baseline x86-64 (no instructions newer than the Core 2 Duo has).

### Connection
- R3. `setup` stores URL, database, login, protocol, server version, uid and
  partner id in `~/.config/odoo-cli/config.json` and the API key in
  `~/.config/odoo-cli/<profile>.key`, both mode 0600. No encryption.
- R4. `ODOO_API_KEY` and `ODOO_PROFILE` override the stored values.
- R5. Protocol detection: JSON-2 (`POST /json/2/<model>/<method>`, bearer key,
  `X-Odoo-Database`) on Odoo 18.x and later when it answers; otherwise
  `execute_kw` over `/jsonrpc` with `common.authenticate` for the uid.
  Overridable with `--protocol`.
- R6. Odoo 20 API keys need the **RPC** scope.

### Output contract
- R7. Success: exactly one JSON document on stdout.
- R8. Failure: one JSON object `{"error": {"kind", "message", ...}}` on stderr
  and exit code 1 (Odoo refused), 2 (bad input), 3 (config/auth), 4 (network).
- R9. Many2one values are `[id, "name"]` everywhere, including aggregates.
- R10. Searches default to 50 records and report `truncated` and `next_offset`.

### Accuracy
- R11. Field names in `-f`, domains, `create` and `update` are checked against
  the model's `fields_get` before sending. Unknown names fail with exit 2 and
  ranked suggestions. A miss refreshes the cache once before failing.
- R12. Writes to fields Odoo marks read-only produce a warning on stderr.
- R13. `--dry-run` on every write prints the exact request without sending.
- R14. `delete` refuses without `--yes`.

### Domains of use
- R15. Tasks: list with `--mine`, `--open`, `--project`, `--search`,
  `--due-before`; version-aware open filter (`state` on 17+, folded stage).
- R16. Projects, finance, people: generic commands plus recipes in the guide.
- R17. Timesheets: `log` (project inferred from task) and `list`.
- R18. Messages: read and post chatter on any record (message or internal
  note); list, read and send Discuss channel messages.
- R19. Attachments: every message lists its files, including inline images
  referenced as `/web/image/<id>` or `/web/content/<id>`. Files download to
  disk on request and report a `path`; `--base64` returns content inline.
  Downloads are cached by id and size. Uploads attach files to records or to
  new messages. Image fields on records save to files with the format sniffed
  from the bytes.
- R20. Aggregates: grouped totals across Odoo versions (20 `read_group`,
  19 `formatted_read_group`, older legacy `read_group`).
- R21. Introspection: `models`, `fields`, `modules`, `access`, `ref`.

### Companies
- R25. Without a company choice, Odoo 13+ applies its own default: reads and
  record rules cover all of the user's allowed companies (`env.companies` =
  `res.users.company_ids`); the current company (`env.company`: new records,
  company-dependent fields) is the user's default company
  (`res.users.company_id`). Confirmed in Odoo's `Environment.company` and
  `.companies` (13.0 to 19.0) and live on 19 and 20.
- R26. `--company ID[,ID...]` sets `allowed_company_ids` (first id = current
  company). A different `allowed_company_ids` in `--context` is a usage error
  (exit 2). Refused on Odoo 12 and older, which ignore the key.
- R27. `companies` lists the allowed companies (id, name, country, currency,
  parent) with `default_company`, `current_company` and
  `selected_company_ids` for the call; `whoami` includes the same in short.
  An id outside the allowed companies fails with exit 2 and the list.
- R28. `timesheet log` reads back and prints `employee_id` and `company_id`;
  it warns when no company or employee was chosen and the user has employees
  in several companies; Odoo's "active employee in the selected companies"
  error gets a hint with the user's employees per company.
- R29. `fields` shows `company_dependent: true`. Dry runs show the merged
  context. Odoo's multi-company errors get a `hint`.

### Performance
- R22. No async runtime; one blocking HTTP agent per process; gzip responses.
- R23. `fields_get` results cached on disk for 24 hours per profile and model.
- R24. Memory proportional to the response; responses capped at 256 MB,
  attachments at 25 MB by default.

## Version notes found while building

| Behaviour | Odoo ≤17 | 18 | 19 | 20 |
|---|---|---|---|---|
| Protocol | JSON-RPC | JSON-RPC (JSON-2 on some 18.x SaaS builds) | JSON-2 (JSON-RPC deprecated) | JSON-2 |
| Attachment content field | `datas` | `datas` | `datas` | `raw` (`{filename, content, size}`) |
| Grouping | `read_group(fields, groupby, lazy)` | same | `formatted_read_group` | `read_group(groupby, aggregates)` → tuples |
| Access check | `check_access_rights` | `has_access` | `has_access` | `has_access` |
| HTML message bodies | escaped | `body_is_html` | `body_is_html` | `body_is_html` |
| Discuss model | `mail.channel` (≤16), `discuss.channel` (17) | `discuss.channel` | `discuss.channel` | `discuss.channel` |
| API key scopes | none | none | none | `rpc` / `mcp` |
| Default company scope (no `allowed_company_ids`) | 13+: all allowed companies, current = default; ≤12: no such key, user's current company | same | same | same |

The 19 and ≤17 columns follow the documented API; only Odoo 20 was tested live.
