---
name: odoo
description: Read and change the user's Odoo data from the shell with the `odoo-cli` binary — tasks, projects, timesheets, invoices and other finance records, chatter and Discuss messages (including images and files attached to messages), users and employees. Use whenever a task involves Odoo on this machine, e.g. "check my Odoo tasks", "log 2 hours on task 42", "what did they send in the project chat", "which invoices are unpaid". Prefer it over any Odoo MCP server here.
---

# Odoo via odoo-cli

`odoo-cli` is installed in `~/.local/bin`. It prints JSON, needs no server
process, and is already configured for the user's Odoo.

1. Run `odoo-cli guide` once per session and follow it. It has the rules and
   copy-ready recipes for tasks, projects, timesheets, finance, messages,
   attachments and users.
2. Run `odoo-cli whoami` if unsure which server or user you are acting as.
   Exit code 3 means the profile is missing or the key expired: tell the user
   to run `odoo-cli setup --url URL --db DB --login LOGIN` themselves; never ask
   for the API key in chat.
3. Check field names with `odoo-cli fields MODEL --search WORD` before writing.
   The user has several companies: reads cover all of them, but writes go to
   one. Pass `--company ID` (ids from `odoo-cli companies`) on writes and
   timesheets for any company other than the default, and check `company_id`
   in the output.
4. Use `--dry-run` before writes, and confirm with the user before posting
   messages to followers, posting or resetting invoices, deleting, or changing
   users.
5. To see an image someone sent: `odoo-cli chatter read MODEL ID --download images`
   (or `chat read CHANNEL --download images`), then open each `path` from the
   output with the Read tool.
