# Provenance

This is an independent Rust implementation. No source code from the projects
below was copied, translated or adapted. They were read to learn which
features are useful and how Odoo behaves; the code here was written from
Odoo's documented API and from observed server behaviour.

Personal, private use. Review again before any distribution.

## Projects consulted

| Project | License | What was looked at | What we took |
|---|---|---|---|
| [pantalytics/odoo-mcp-pro](https://github.com/pantalytics/odoo-mcp-pro) | Mixed: Elastic License 2.0 (Pantalytics additions), MPL-2.0 (files from upstream) | README tool list; issue titles; PR #116 description (`get_image`) | Feature ideas: grouped aggregates, image fields saved as pictures, keeping Odoo's error text. Gap confirmed: images attached to messages are not retrievable there. |
| [ivnvxd/mcp-server-odoo](https://github.com/ivnvxd/mcp-server-odoo) | MPL-2.0 | README tool list and sections (attachments, per-call context, blocked methods) | Feature ideas: attachment read/list/upload, `post_message`, per-call `context` (lang, tz, allowed_company_ids). |
| [tinyerp/odooly](https://github.com/tinyerp/odooly) | BSD-3-Clause style (Florent Xicluna) | README; public method names and docstrings of `access`, `ref`, `modules`, `generate_api_key`, `get_metadata` | Feature ideas: `access`, `ref`, `modules`, including archived records (`active_test`). Confirms JSON-2 for 19+ and JSON-RPC fallback. |
| [joergsteffens/odoo-cli](https://github.com/joergsteffens/odoo-cli) | AGPL-3.0 | README only (usage section) | Nothing beyond the general idea of a generic `call` command, which is also Odoo's own documented API shape. No code read beyond the README. |

## Authoritative references used for the implementation

- Odoo external API documentation (JSON-2 for 19+, `execute_kw` over
  `/jsonrpc` for older versions): https://www.odoo.com/documentation/19.0/developer/reference/external_rpc_api.html
- Odoo 20 source, LGPL-3.0, read to understand server behaviour:
  - `odoo/addons/base/models/ir_attachment.py`: `datas` was removed; file
    content goes through `raw`, which reads back as
    `{"filename", "content", "size"}`.
  - `odoo/orm/models.py`: `read_group(domain, groupby, aggregates, ...)` returns
    tuples in 20.
- Behaviour observed against Odoo's public demo server (Odoo 20.0+e):
  API key scopes (`rpc` / `mcp`), `has_access`, `message_post` with
  `attachment_ids` and `body_is_html`, `ir.model.access` not exposed over JSON-2.
