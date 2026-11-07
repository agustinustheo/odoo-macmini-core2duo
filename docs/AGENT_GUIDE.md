# odoo-cli agent guide

`odoo-cli` talks to Odoo from the shell. It is one static binary, holds no
server process, and prints one JSON document on stdout per call.

## Ground rules

1. **Look before you write.** Field names differ between Odoo versions and
   installed apps. Run `odoo-cli fields MODEL --search WORD` before you create
   or update anything you have not touched in this session. Unknown fields fail
   with exit 2 and a `did_you_mean` list; nothing is sent to Odoo.
2. **Dry-run risky writes.** `create`, `update`, `delete`, `call`,
   `timesheet log`, `chatter post` and `chat send` accept `--dry-run`, which
   prints the exact request without sending it.
3. **Read results, not assumptions.** Many2one values come back as
   `[id, "Display name"]`. Search output has `truncated: true` and a
   `next_offset` when there are more records than `--limit` (default 50).
4. **Never guess ids.** Look them up with `search` or `ref module.name`.
5. **Prefer archiving to deleting.** `update MODEL ID '{"active": false}'`.
   `delete` needs `--yes` and cannot be undone.
6. **Messages notify people.** `chatter post` (without `--note`) and
   `chat send` reach real followers. Only post what the user asked for.

Exit codes: `0` ok · `1` Odoo refused (access, validation, missing record) ·
`2` bad input or unknown field · `3` profile or authentication · `4` network.
Errors are JSON on stderr: `{"error": {"kind", "message", ...}}`. Add
`--debug` to include the server traceback.

## Discover

```bash
odoo-cli whoami                          # who am I, which server, version, companies
odoo-cli companies                       # company ids, names, currencies; which one is used
odoo-cli modules project                 # is an app installed?
odoo-cli models invoice                  # find model names by label or name
odoo-cli fields project.task --search deadline
odoo-cli fields account.move -f state,move_type     # selection values included
odoo-cli access account.move             # can I read/write/create/delete?
odoo-cli access project.task --ids 42    # ...these records (Odoo 18+)
odoo-cli ref base.main_company           # external id -> model + id
```

## Generic records (works for every model)

```bash
odoo-cli search MODEL '[["field","op",value], ...]' -f f1,f2 -l 20 --order "date desc"
odoo-cli search res.partner '[["name","ilike","acme"]]' --archived   # include archived
odoo-cli count MODEL '[...]'
odoo-cli get MODEL 1,2,3 -f f1,f2        # missing_ids lists ids not found
odoo-cli create MODEL '{"name": "X"}'    # or a JSON array for several; prints ids
odoo-cli update MODEL 7 '{"field": value}'
odoo-cli delete MODEL 7 --yes
odoo-cli call MODEL METHOD '{"kwarg": value}' --ids 7   # any public method
odoo-cli aggregate MODEL '[...]' -g field,date:month -a amount:sum,__count
```

Domains are JSON: `[["state","=","posted"],["date",">=","2026-01-01"]]`.
Prefix operators work too: `["|", ["a","=",1], ["b","=",2]]`.
Relational writes use Odoo command lists:
`[[4, id]]` link, `[[3, id]]` unlink, `[[6, 0, [ids]]]` replace.
Any argument that takes JSON or text also accepts `-` (stdin) or `@file`.

## Tasks (project.task)

```bash
odoo-cli tasks --mine --open                   # my open tasks, most urgent first
odoo-cli tasks --project 5 --open -s "invoice" # filter by project and name
odoo-cli tasks --mine --stage Review            # what is waiting in Review
odoo-cli tasks --mine --open --due-before 2026-11-14
odoo-cli get project.task 42 -f name,description,stage_id,user_ids,date_deadline
odoo-cli fields project.task -f stage_id,state  # what stages/states exist
odoo-cli search project.task.type '[["project_ids","in",[5]]]' -f name,fold   # stages of project 5
odoo-cli update project.task 42 '{"stage_id": 3}'
odoo-cli update project.task 42 '{"user_ids": [[4, 7]]}'     # add assignee 7
odoo-cli update project.task 42 '{"date_deadline": "2026-11-20"}'
odoo-cli create project.task '{"name": "Fix login", "project_id": 5, "user_ids": [[6,0,[7]]]}'
```

`--open` excludes Odoo's done/cancelled states (`1_done`, `1_canceled` on 17+)
and every stage that is folded or named Done*, Cancel*, Closed* or Complete*.
The output lists those stages in `excluded_stages`; if a board uses other
names for finished work, filter with `--stage` or a `--domain` instead.
`--stage Review` filters by stage name.

## Projects (project.project)

```bash
odoo-cli search project.project '[]' -f name,user_id,partner_id,date_start,date
odoo-cli create project.project '{"name": "Website revamp", "user_id": 2}'
odoo-cli update project.project 5 '{"active": false}'     # archive
```

## Timesheets (account.analytic.line)

```bash
odoo-cli timesheet log --task 42 --hours 1.5 -d "Reviewed PR" [--date 2026-11-06]
odoo-cli timesheet log --project 5 --hours 0.5 -d "Planning call"
odoo-cli timesheet list --mine --from 2026-11-01 --to 2026-11-07
odoo-cli aggregate account.analytic.line '[["project_id","!=",false],["date",">=","2026-11-01"]]' -g project_id -a unit_amount:sum
odoo-cli update account.analytic.line 563 '{"unit_amount": 2}'
```

`timesheet log` fills the project from the task. The employee defaults to the
one linked to the API key's user. The output shows the `employee_id` and
`company_id` Odoo chose. If you have employees in several companies, the line
goes to the task's or project's company, or to your **default** company when
the project has none, and the output carries a `warning`. Choose with
`--company ID` (see below).

## Finance (account.*)

```bash
# Customer invoices (move_type: out_invoice, out_refund, in_invoice = vendor bill, in_refund, entry)
odoo-cli search account.move '[["move_type","=","out_invoice"],["payment_state","!=","paid"],["state","=","posted"]]' -f name,partner_id,invoice_date,invoice_date_due,amount_total,amount_residual
odoo-cli get account.move 12 -f name,state,payment_state,invoice_line_ids,amount_total
odoo-cli search account.move.line '[["move_id","=",12]]' -f name,product_id,quantity,price_unit,price_subtotal
odoo-cli aggregate account.move '[["move_type","=","out_invoice"],["state","=","posted"]]' -g invoice_date:month -a amount_total:sum,__count
odoo-cli create account.move '{"move_type":"out_invoice","partner_id":9,"invoice_line_ids":[[0,0,{"name":"Consulting","quantity":2,"price_unit":150}]]}'
odoo-cli call account.move action_post --ids 12          # confirm a draft invoice
odoo-cli call account.move button_draft --ids 12         # back to draft
odoo-cli search account.payment '[]' -f name,partner_id,amount,date,state
odoo-cli search account.account '[["code","=like","4%"]]' -f code,name   # chart of accounts
```

Posting, resetting and paying are business actions: confirm with the user
first, and `--dry-run` before you send.

## Messages

Chatter (the message thread on any record: task, project, invoice, partner):

```bash
odoo-cli chatter read project.task 42 -l 10                  # newest first, HTML -> text
odoo-cli chatter read project.task 42 --download images      # also save images to disk
odoo-cli chatter post project.task 42 "Deployed to staging"  # notifies followers
odoo-cli chatter post project.task 42 "Internal: blocked on API" --note
odoo-cli chatter post project.task 42 "Screenshot attached" --attach ./shot.png
```

Discuss (channels and direct messages):

```bash
odoo-cli chat channels                     # channels and DMs I am in
odoo-cli chat read 9 -l 20 --download images
odoo-cli chat send 9 "On my way"
odoo-cli chat send 9 - < message.txt       # longer text from stdin
```

Bodies are sent as plain text. `--html` sends HTML (Odoo 18+).

### Images and files in messages

Every message lists its files under `attachments` (id, name, mimetype,
file_size), including images pasted inline. In the text body, inline images
show up as `[image #ID]`. With `--download images` (or `--download all`) each
file gets a `path` on disk. **Open that path with your file/image reader** to
see the picture. Files are cached; a second read does not download again.

```bash
odoo-cli attachment get 3265,3266            # save by id, prints paths
odoo-cli attachment get 3265 --base64        # base64 in the JSON instead
odoo-cli attachment list project.task 42     # files attached to a record
odoo-cli attachment upload project.task 42 ./spec.pdf
odoo-cli image res.partner 9                 # image fields: photos, avatars, logos
odoo-cli image product.template 3 --field image_1920
```

Default folder: `~/.cache/odoo-cli/<profile>/attachments/` (`--dir` to change).
Files over 25 MB are skipped unless you raise `--max-bytes`.

## People and users

```bash
odoo-cli search res.users '[]' -f name,login,active,share       # share=true: portal user
odoo-cli search hr.employee '[]' -f name,job_title,department_id,parent_id,user_id,work_email
odoo-cli search res.partner '[["is_company","=",true]]' -f name,email,phone
odoo-cli fields res.users --search group      # group field name differs by version
odoo-cli create res.users '{"name": "Ana Lima", "login": "ana@example.com"}'
odoo-cli update res.users 8 '{"active": false}'                  # deactivate a user
odoo-cli update hr.employee 4 '{"job_title": "Lead Engineer"}'
```

Creating or deactivating users changes who can log in (and can affect the
subscription seat count). Confirm with the user first.

## Multiple companies

One database can hold several companies. `odoo-cli companies` lists yours;
`whoami` shows the same in short.

- **Reads span all your companies** by default: searches, counts and
  aggregates mix records of every company you can use, plus shared records
  (`company_id` false). Add `company_id` to `-f` when that matters. Names
  repeat across companies (each company has its own "Sales" journal).
- **`--company ID[,ID...]`** works like the company switcher in Odoo: reads
  see only those companies (plus shared records), and the **first** id is the
  current company. Example: `odoo-cli --company 3 search account.journal '[]'`.
- **New records** without `company_id` go to the current company: the first
  `--company` id, otherwise your *default* company (`default_company` in
  `companies`). Pass `--company` on every write meant for another company.
  Invoices follow their journal's company.
- **Company-dependent fields** (`fields` marks them `"company_dependent": true`,
  e.g. product cost `standard_price`, a partner's receivable account) hold one
  value per company. Reads and writes use the current company only:
  `odoo-cli --company 2 update product.product 7 '{"standard_price": 12}'`
  changes company 2's cost and leaves the others alone.
- **Money across companies**: `aggregate ... -a amount_total:sum` adds amounts
  of every selected company together. Group by `company_id` (or pass
  `--company`), and by `currency_id` when currencies differ.
- **Cross-company errors** exit 1 with Odoo's message ("multi-company issue",
  "company inconsistencies") and a `hint`. A record of another company reads
  as "no access" until that company is selected.
- `--dry-run` shows the company context that would be sent.
- `--company` and `--context '{"allowed_company_ids":[...]}'` do the same
  thing; giving both with different ids fails with exit 2. Not available on
  Odoo 12 and older.

## Options for every command

`--profile NAME` (or `ODOO_PROFILE`) · `--company ID[,ID...]` · `--pretty` ·
`--context '{"lang":"fr_FR"}'` · `--timeout SECONDS` · `--debug`.
