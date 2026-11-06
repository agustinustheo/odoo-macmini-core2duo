mod attachments;
mod client;
mod config;
mod error;
mod schema;

use clap::{Args, Parser, Subcommand};
use client::Client;
use config::{Profile, Protocol};
use error::{Error, Result};
use schema::Schema;
use serde_json::{json, Map, Value};
use std::io::{IsTerminal, Read, Write};

const GUIDE: &str = include_str!("../docs/AGENT_GUIDE.md");

/// Odoo from the command line. Prints JSON on stdout; errors are JSON on stderr.
///
/// Exit codes: 0 ok, 1 Odoo refused, 2 bad input or unknown field,
/// 3 profile/auth problem, 4 network. Run `odoo-cli guide` for recipes.
#[derive(Parser)]
#[command(name = "odoo-cli", version, max_term_width = 100)]
struct Cli {
    /// Profile to use (default: $ODOO_PROFILE, then the saved default).
    #[arg(long, global = true)]
    profile: Option<String>,
    /// Pretty-print JSON output.
    #[arg(long, global = true)]
    pretty: bool,
    /// Extra Odoo context as a JSON object, e.g. '{"lang":"en_US"}'.
    #[arg(long, global = true, value_name = "JSON")]
    context: Option<String>,
    /// Request timeout in seconds.
    #[arg(long, global = true, default_value_t = 60)]
    timeout: u64,
    /// Include the Odoo server traceback in errors.
    #[arg(long, global = true)]
    debug: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Save a connection profile. Reads the API key from stdin.
    Setup(SetupArgs),
    /// Show the connected user, server version and protocol.
    Whoami,
    /// Print the agent guide: recipes for tasks, projects, finance, timesheets, messages, users.
    Guide,
    /// List installed models, optionally filtered by technical name or label.
    Models { filter: Option<String> },
    /// Can I read / write / create / delete records of this model?
    Access {
        model: String,
        /// Check these records too (record rules), Odoo 18+.
        #[arg(long)]
        ids: Option<String>,
    },
    /// Resolve an external id like `base.main_company` to model and id.
    Ref { xmlid: String },
    /// Installed Odoo apps/modules, optionally filtered by name.
    Modules {
        filter: Option<String>,
        /// Include modules that are not installed.
        #[arg(long)]
        all: bool,
    },
    /// Show a model's fields (cached for 24h). Use this before writing.
    Fields {
        model: String,
        /// Only fields whose name or label contains TEXT.
        #[arg(long, short)]
        search: Option<String>,
        /// Only these fields (comma-separated).
        #[arg(long, short = 'f', value_delimiter = ',')]
        only: Vec<String>,
        /// Include help texts.
        #[arg(long, short)]
        verbose: bool,
        /// Ignore the cache.
        #[arg(long)]
        refresh: bool,
    },
    /// Search records: search_read with a JSON domain.
    Search(SearchArgs),
    /// Grouped totals computed by Odoo, e.g. invoiced amount per month.
    Aggregate {
        model: String,
        domain: Option<String>,
        /// Group by fields, comma-separated; dates take a granularity: invoice_date:month
        #[arg(long, short, value_delimiter = ',')]
        groupby: Vec<String>,
        /// Aggregates `field:func` (sum, avg, min, max, count_distinct), comma-separated.
        /// `__count` (the default) counts records.
        #[arg(long, short, value_delimiter = ',', default_value = "__count")]
        agg: Vec<String>,
        #[arg(long, short, default_value_t = 200)]
        limit: u64,
        #[arg(long)]
        order: Option<String>,
    },
    /// Save an image or binary field (product photo, avatar, logo...) to a file.
    Image {
        model: String,
        id: i64,
        /// Field to read (default: image_512, then image_1920, then image).
        #[arg(long)]
        field: Option<String>,
        #[arg(long)]
        dir: Option<String>,
    },
    /// Count records matching a JSON domain.
    Count {
        model: String,
        domain: Option<String>,
        /// Include archived records.
        #[arg(long)]
        archived: bool,
    },
    /// Read records by id.
    Get {
        model: String,
        /// Comma-separated ids.
        ids: String,
        #[arg(long, short = 'f', value_delimiter = ',')]
        fields: Vec<String>,
    },
    /// Create one record (JSON object) or several (JSON array). Prints new ids.
    Create {
        model: String,
        /// JSON values, '-' for stdin or '@file'.
        values: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Write values to records.
    Update {
        model: String,
        ids: String,
        /// JSON object, '-' for stdin or '@file'.
        values: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete records. Requires --yes.
    Delete {
        model: String,
        ids: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Call any public model method with named arguments.
    Call {
        model: String,
        method: String,
        /// Named arguments as a JSON object.
        kwargs: Option<String>,
        /// Records to call the method on (comma-separated ids).
        #[arg(long)]
        ids: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Project tasks with filters that adapt to the Odoo version.
    Tasks(TasksArgs),
    /// Log and list timesheet lines.
    #[command(subcommand)]
    Timesheet(TimesheetCmd),
    /// Read or post on a record's chatter (tasks, projects, invoices...).
    #[command(subcommand)]
    Chatter(ChatterCmd),
    /// Discuss channels and direct messages.
    #[command(subcommand)]
    Chat(ChatCmd),
    /// List and download files attached to records or messages (images included).
    #[command(subcommand)]
    Attachment(AttachmentCmd),
}

#[derive(Args, Clone)]
struct DownloadOpts {
    /// Download attached files to disk and print their paths: `images` or `all`.
    #[arg(long, value_parser = ["images", "all"])]
    download: Option<String>,
    /// Where to save files (default: ~/.cache/odoo-cli/<profile>/attachments).
    #[arg(long)]
    dir: Option<String>,
    /// Skip files larger than this many bytes.
    #[arg(long, default_value_t = 25 << 20)]
    max_bytes: u64,
}

#[derive(Subcommand)]
enum AttachmentCmd {
    /// Files attached to a record (not counting those only inside messages).
    List { model: String, id: i64 },
    /// Attach local files directly to a record (no message).
    Upload {
        model: String,
        id: i64,
        #[arg(required = true)]
        files: Vec<String>,
    },
    /// Download attachments by id. Prints the saved paths.
    Get {
        /// Comma-separated attachment ids.
        ids: String,
        #[arg(long)]
        dir: Option<String>,
        /// Print base64 content in the JSON instead of writing files.
        #[arg(long)]
        base64: bool,
        #[arg(long, default_value_t = 25 << 20)]
        max_bytes: u64,
    },
}

#[derive(Args)]
struct SetupArgs {
    /// Base URL, e.g. https://mycompany.odoo.com
    #[arg(long)]
    url: String,
    /// Database name (required for Odoo 18 and older).
    #[arg(long)]
    db: Option<String>,
    /// Login (usually the user's email).
    #[arg(long)]
    login: String,
    /// Profile name.
    #[arg(long, default_value = "default")]
    name: String,
    /// Force a protocol instead of detecting it.
    #[arg(long, value_parser = ["json2", "jsonrpc"])]
    protocol: Option<String>,
    /// Read the API key from this file instead of stdin.
    #[arg(long)]
    api_key_file: Option<String>,
}

#[derive(Args)]
struct SearchArgs {
    model: String,
    /// JSON domain, e.g. '[["state","=","posted"]]'. '-' for stdin.
    domain: Option<String>,
    /// Fields to return (default: id, display_name).
    #[arg(long, short = 'f', value_delimiter = ',')]
    fields: Vec<String>,
    /// Max records; 0 for no limit.
    #[arg(long, short, default_value_t = 50)]
    limit: u64,
    #[arg(long, default_value_t = 0)]
    offset: u64,
    /// Sort, e.g. "date desc, id desc".
    #[arg(long)]
    order: Option<String>,
    /// Include archived records (active = false).
    #[arg(long)]
    archived: bool,
}

#[derive(Args)]
struct TasksArgs {
    /// Only tasks assigned to me.
    #[arg(long)]
    mine: bool,
    /// Only open tasks (not done, not cancelled, not in a folded stage).
    #[arg(long)]
    open: bool,
    /// Only tasks of this project id.
    #[arg(long)]
    project: Option<i64>,
    /// Name contains TEXT.
    #[arg(long, short)]
    search: Option<String>,
    /// Deadline on or before this date (YYYY-MM-DD).
    #[arg(long)]
    due_before: Option<String>,
    /// Extra JSON domain, ANDed with the filters.
    #[arg(long)]
    domain: Option<String>,
    #[arg(long, short = 'f', value_delimiter = ',')]
    fields: Vec<String>,
    #[arg(long, short, default_value_t = 50)]
    limit: u64,
    #[arg(long, default_value = "priority desc, date_deadline asc, id desc")]
    order: String,
}

#[derive(Subcommand)]
enum TimesheetCmd {
    /// Log time on a task or project.
    Log {
        /// Task id (its project is filled in automatically).
        #[arg(long)]
        task: Option<i64>,
        /// Project id (needed when no task is given).
        #[arg(long)]
        project: Option<i64>,
        /// Hours, e.g. 1.5
        #[arg(long)]
        hours: f64,
        /// Description of the work.
        #[arg(long, short)]
        description: String,
        /// Date YYYY-MM-DD (default: today, decided by Odoo).
        #[arg(long)]
        date: Option<String>,
        /// Employee id (default: the employee linked to me).
        #[arg(long)]
        employee: Option<i64>,
        #[arg(long)]
        dry_run: bool,
    },
    /// List timesheet lines.
    List {
        #[arg(long)]
        mine: bool,
        #[arg(long)]
        task: Option<i64>,
        #[arg(long)]
        project: Option<i64>,
        /// From date YYYY-MM-DD (inclusive).
        #[arg(long)]
        from: Option<String>,
        /// To date YYYY-MM-DD (inclusive).
        #[arg(long)]
        to: Option<String>,
        #[arg(long, short, default_value_t = 50)]
        limit: u64,
    },
}

#[derive(Subcommand)]
enum ChatterCmd {
    /// Recent messages on a record, newest first, HTML stripped to text.
    Read {
        model: String,
        id: i64,
        #[arg(long, short, default_value_t = 20)]
        limit: u64,
        /// Keep the HTML body.
        #[arg(long)]
        html: bool,
        #[command(flatten)]
        files: DownloadOpts,
    },
    /// Post a message (followers are notified) or an internal note.
    Post {
        model: String,
        id: i64,
        /// Text, '-' for stdin or '@file'.
        body: String,
        /// Internal note instead of a message to followers.
        #[arg(long)]
        note: bool,
        /// Treat the body as HTML (Odoo 18+).
        #[arg(long)]
        html: bool,
        /// Also notify these partner ids (comma-separated).
        #[arg(long)]
        partners: Option<String>,
        /// Attach a local file (repeatable).
        #[arg(long)]
        attach: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum ChatCmd {
    /// List channels and direct-message conversations.
    Channels {
        #[arg(long, short)]
        search: Option<String>,
        #[arg(long, short, default_value_t = 50)]
        limit: u64,
    },
    /// Read recent messages in a channel.
    Read {
        channel: i64,
        #[arg(long, short, default_value_t = 20)]
        limit: u64,
        #[arg(long)]
        html: bool,
        #[command(flatten)]
        files: DownloadOpts,
    },
    /// Send a message to a channel or direct-message conversation.
    Send {
        channel: i64,
        /// Text, '-' for stdin or '@file'.
        body: String,
        #[arg(long)]
        html: bool,
        /// Attach a local file (repeatable).
        #[arg(long)]
        attach: Vec<String>,
        #[arg(long)]
        dry_run: bool,
    },
}

// ---------------------------------------------------------------- helpers

fn read_input(arg: &str) -> Result<String> {
    if arg == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| Error::usage(format!("reading stdin: {e}")))?;
        Ok(s)
    } else if let Some(path) = arg.strip_prefix('@') {
        std::fs::read_to_string(path).map_err(|e| Error::usage(format!("reading {path}: {e}")))
    } else {
        Ok(arg.to_string())
    }
}

fn parse_json(arg: &str, what: &str) -> Result<Value> {
    let text = read_input(arg)?;
    serde_json::from_str(&text).map_err(|e| Error::usage(format!("{what} is not valid JSON: {e}")))
}

fn parse_domain(arg: Option<&str>) -> Result<Vec<Value>> {
    match arg {
        None => Ok(vec![]),
        Some(a) => match parse_json(a, "domain")? {
            Value::Array(v) => Ok(v),
            _ => Err(Error::usage("domain must be a JSON array, e.g. '[[\"name\",\"ilike\",\"x\"]]'")),
        },
    }
}

fn parse_object(arg: &str, what: &str) -> Result<Map<String, Value>> {
    match parse_json(arg, what)? {
        Value::Object(m) => Ok(m),
        _ => Err(Error::usage(format!("{what} must be a JSON object"))),
    }
}

fn parse_ids(s: &str) -> Result<Vec<i64>> {
    let ids: std::result::Result<Vec<i64>, _> =
        s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(str::parse::<i64>).collect();
    match ids {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(Error::usage(format!("ids must be comma-separated integers, got '{s}'"))),
    }
}

/// Field names used in a domain (first element of each 3-item leaf).
fn domain_fields(domain: &[Value]) -> Vec<&str> {
    domain
        .iter()
        .filter_map(|leaf| leaf.as_array().and_then(|l| l.first()).and_then(Value::as_str))
        .collect()
}

fn kw(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn as_ids(v: &Value) -> Vec<i64> {
    match v {
        Value::Array(a) => a.iter().filter_map(Value::as_i64).collect(),
        Value::Number(n) => n.as_i64().into_iter().collect(),
        _ => vec![],
    }
}

fn dry(model: &str, method: &str, ids: Option<&[i64]>, kwargs: &Map<String, Value>) -> Value {
    json!({ "dry_run": true, "model": model, "method": method, "ids": ids, "kwargs": kwargs })
}

/// HTML to readable text for message bodies. Small on purpose.
fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match (in_tag, c) {
            (false, '<') => {
                in_tag = true;
                tag.clear();
            }
            (true, '>') => {
                in_tag = false;
                let t = tag.trim_start_matches('/').split([' ', '/']).next().unwrap_or("").to_ascii_lowercase();
                if t == "img" {
                    // Keep a marker where the image sat; ids match `attachments`.
                    match attachments::inline_ids(&tag).first() {
                        Some(id) => out.push_str(&format!("[image #{id}]")),
                        None => out.push_str("[image]"),
                    }
                }
                if matches!(t.as_str(), "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4") && !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            (true, c) => tag.push(c),
            (false, c) => out.push(c),
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    out.trim().to_string()
}

fn search_page(client: &Client, model: &str, domain: Vec<Value>, fields: Vec<&str>, limit: u64, offset: u64, order: Option<&str>) -> Result<Value> {
    let mut k = kw(vec![("domain", json!(domain)), ("fields", json!(fields)), ("offset", json!(offset))]);
    if limit > 0 {
        k.insert("limit".into(), json!(limit + 1));
    }
    if let Some(o) = order {
        k.insert("order".into(), json!(o));
    }
    let res = client.call(model, "search_read", None, k)?;
    let mut records = match res {
        Value::Array(a) => a,
        other => return Err(Error::odoo("search_read returned an unexpected value", json!({ "result": other }))),
    };
    let truncated = limit > 0 && records.len() as u64 > limit;
    if truncated {
        records.truncate(limit as usize);
    }
    Ok(json!({
        "model": model,
        "returned": records.len(),
        "truncated": truncated,
        "next_offset": if truncated { json!(offset + limit) } else { Value::Null },
        "records": records,
    }))
}

fn files_dir(client: &Client, dir: Option<&str>) -> std::path::PathBuf {
    dir.map(std::path::PathBuf::from).unwrap_or_else(|| attachments::default_dir(&client.profile.name))
}

fn message_rows(client: &Client, model: &str, res_id: i64, limit: u64, html: bool, files: &DownloadOpts) -> Result<Value> {
    let domain = vec![json!(["model", "=", model]), json!(["res_id", "=", res_id])];
    let fields = vec!["id", "date", "author_id", "message_type", "subtype_id", "body", "attachment_ids"];
    let mut page = search_page(client, "mail.message", domain, fields, limit, 0, Some("id desc"))?;
    let rows = page["records"].as_array().cloned().unwrap_or_default();

    // One metadata read for every file in the page, then optional downloads.
    let ids = attachments::ids_in_messages(&rows);
    let mut meta = attachments::metadata(client, &ids)?;
    if let Some(which) = &files.download {
        let (want, rest): (Vec<Value>, Vec<Value>) = meta.into_iter().partition(|m| {
            which == "all" || m["mimetype"].as_str().is_some_and(|t| t.starts_with("image/"))
        });
        let dir = files_dir(client, files.dir.as_deref());
        meta = attachments::download(client, want, &dir, false, files.max_bytes)?;
        meta.extend(rest);
    }
    let by_id: std::collections::HashMap<i64, Value> =
        meta.into_iter().filter_map(|m| Some((m["id"].as_i64()?, m))).collect();

    if let Some(Value::Array(out)) = page.get_mut("records") {
        for r in out.iter_mut() {
            let own = attachments::ids_in_messages(std::slice::from_ref(r));
            let files: Vec<Value> = own
                .iter()
                .map(|id| by_id.get(id).cloned().unwrap_or_else(|| json!({ "id": id, "error": "not readable" })))
                .collect();
            if let Value::Object(o) = r {
                o.remove("attachment_ids");
                if !files.is_empty() {
                    o.insert("attachments".into(), json!(files));
                }
            }
            if !html {
                if let Some(Value::String(b)) = r.get_mut("body") {
                    *b = html_to_text(b);
                }
            }
        }
    }
    page["attachments_total"] = json!(ids.len());
    Ok(page)
}

/// Grouped aggregates across Odoo versions, normalised to one object per group
/// keyed by the groupby and aggregate specs as given:
/// - 20+: `read_group(domain, groupby, aggregates)` returns tuples.
/// - 19: `formatted_read_group(...)` returns dicts.
/// - older: legacy `read_group(domain, fields, groupby, lazy=False)`.
fn read_groups(c: &Client, model: &str, domain: Vec<Value>, groupby: &[String], agg: &[String], limit: u64, order: Option<&str>) -> Result<Vec<Value>> {
    let major = c.profile.major;
    let mut k = kw(vec![("domain", json!(domain)), ("groupby", json!(groupby))]);
    if limit > 0 {
        k.insert("limit".into(), json!(limit));
    }
    if let Some(o) = order {
        k.insert("orderby".into(), json!(o));
    }
    if major >= 19 {
        if let Some(o) = k.remove("orderby") {
            k.insert("order".into(), o);
        }
        k.insert("aggregates".into(), json!(agg));
        let method = if major >= 20 { "read_group" } else { "formatted_read_group" };
        let rows = c.call(model, method, None, k)?;
        let rows = rows.as_array().cloned().unwrap_or_default();
        if major >= 20 {
            let keys: Vec<&String> = groupby.iter().chain(agg.iter()).collect();
            return Ok(rows
                .into_iter()
                .map(|t| {
                    let vals = t.as_array().cloned().unwrap_or_default();
                    Value::Object(keys.iter().zip(vals).map(|(k, v)| ((*k).clone(), v)).collect())
                })
                .collect());
        }
        return Ok(rows
            .into_iter()
            .map(|r| Value::Object(groupby.iter().chain(agg.iter()).map(|key| (key.clone(), r[key.as_str()].clone())).collect()))
            .collect());
    }
    let fields: Vec<&String> = agg.iter().filter(|a| *a != "__count").collect();
    k.insert("fields".into(), json!(fields));
    k.insert("lazy".into(), json!(false));
    let rows = c.call(model, "read_group", None, k)?;
    Ok(rows
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let mut o = Map::new();
            for g in groupby {
                o.insert(g.clone(), r[g.as_str()].clone());
            }
            for a in agg {
                let src = if a == "__count" { "__count" } else { a.split(':').next().unwrap_or(a) };
                o.insert(a.clone(), r[src].clone());
            }
            Value::Object(o)
        })
        .collect())
}

fn channel_model(client: &Client) -> &'static str {
    if client.profile.major >= 17 || client.profile.major == 0 {
        "discuss.channel"
    } else {
        "mail.channel"
    }
}

#[allow(clippy::too_many_arguments)]
fn post_message(client: &Client, model: &str, id: i64, body: String, note: bool, html: bool, partners: Option<&str>, attach: &[String], dry_run: bool) -> Result<Value> {
    if html && client.profile.major > 0 && client.profile.major < 18 {
        return Err(Error::usage("--html needs Odoo 18 or later; send plain text instead"));
    }
    let mut k = kw(vec![
        ("body", json!(body)),
        ("message_type", json!("comment")),
        ("subtype_xmlid", json!(if note { "mail.mt_note" } else { "mail.mt_comment" })),
    ]);
    if html {
        k.insert("body_is_html".into(), json!(true));
    }
    if let Some(p) = partners {
        k.insert("partner_ids".into(), json!(parse_ids(p)?));
    }
    for f in attach {
        if !std::path::Path::new(f).is_file() {
            return Err(Error::usage(format!("--attach: no such file {f}")));
        }
    }
    if dry_run {
        if !attach.is_empty() {
            k.insert("attachment_ids".into(), json!(attach));
        }
        return Ok(dry(model, "message_post", Some(&[id]), &k));
    }
    if !attach.is_empty() {
        let ids = attach.iter().map(|f| attachments::upload(client, f, "mail.compose.message", 0)).collect::<Result<Vec<i64>>>()?;
        k.insert("attachment_ids".into(), json!(ids));
    }
    let res = client.call(model, "message_post", Some(&[id]), k)?;
    // JSON-2 and jsonrpc both return the new mail.message id (or [id]).
    let msg_id = res.as_i64().or_else(|| as_ids(&res).first().copied());
    Ok(json!({ "ok": true, "model": model, "res_id": id, "message_id": msg_id, "note": note }))
}

// ---------------------------------------------------------------- setup

fn setup(a: SetupArgs) -> Result<Value> {
    let key = match &a.api_key_file {
        Some(p) => std::fs::read_to_string(p).map_err(|e| Error::usage(format!("reading {p}: {e}")))?,
        None => {
            if std::io::stdin().is_terminal() {
                eprint!("Odoo API key (Settings > Users > API Keys): ");
                let _ = std::io::stderr().flush();
            }
            let mut s = String::new();
            std::io::stdin().read_line(&mut s).map_err(|e| Error::usage(e.to_string()))?;
            s
        }
    };
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(Error::usage("empty API key"));
    }
    let url = client::base(&a.url);
    let agent = client::agent(30);

    let info = client::jsonrpc_route(&agent, &format!("{url}/web/webclient/version_info"), json!({}))
        .map_err(|e| Error::network(format!("{url} does not look like an Odoo server: {}", e.message)))?;
    let version = info.get("server_version").and_then(Value::as_str).unwrap_or("").to_string();
    let major = info
        .get("server_version_info")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim_start_matches("saas~").split('.').next()?.parse().ok())))
        .unwrap_or(0);

    let mut profile = Profile {
        name: a.name.clone(),
        url: url.clone(),
        db: a.db.clone(),
        login: a.login.clone(),
        uid: 0,
        partner_id: None,
        protocol: Protocol::Json2,
        version: version.clone(),
        major,
        api_key: key.clone(),
    };

    let forced = a.protocol.as_deref().and_then(Protocol::parse);
    let try_json2 = forced.map_or(major >= 18 || major == 0, |p| p == Protocol::Json2);
    let mut json2_err = None;
    if try_json2 {
        let c = Client::new(profile.clone(), 30);
        match c.call("res.users", "search_read", None, kw(vec![
            ("domain", json!([["login", "=", a.login]])),
            ("fields", json!(["id", "partner_id"])),
            ("limit", json!(1)),
        ])) {
            Ok(Value::Array(rows)) if !rows.is_empty() => {
                profile.uid = rows[0]["id"].as_i64().unwrap_or(0);
                profile.partner_id = rows[0]["partner_id"].get(0).and_then(Value::as_i64);
            }
            Ok(_) => return Err(Error::config(format!("API key works, but no user has login '{}'", a.login))),
            Err(e) if forced.is_some() => return Err(e),
            Err(e) => json2_err = Some(e),
        }
    }
    if profile.uid == 0 {
        let db = a.db.clone().ok_or_else(|| {
            let mut e = Error::config("JSON-2 is not available here, and the legacy protocol needs --db");
            if let Some(j) = &json2_err {
                e = e.with_detail(json!({ "json2_error": j.message }));
            }
            e
        })?;
        profile.protocol = Protocol::JsonRpc;
        profile.uid = client::jsonrpc_authenticate(&agent, &url, &db, &a.login, &key)?;
        let c = Client::new(profile.clone(), 30);
        let rows = c.call("res.users", "read", Some(&[profile.uid]), kw(vec![("fields", json!(["partner_id"]))]))?;
        profile.partner_id = rows.get(0).and_then(|r| r["partner_id"].get(0)).and_then(Value::as_i64);
    }
    config::save(&profile, true)?;
    Ok(json!({
        "ok": true, "profile": profile.name, "url": profile.url, "db": profile.db,
        "login": profile.login, "uid": profile.uid, "partner_id": profile.partner_id,
        "server_version": profile.version, "protocol": profile.protocol.as_str(),
        "config": config::config_dir().display().to_string(),
    }))
}

// ---------------------------------------------------------------- dispatch

fn run(cli: Cli) -> Result<Value> {
    let cmd = match cli.cmd {
        Cmd::Setup(a) => return setup(a),
        Cmd::Guide => {
            print!("{GUIDE}");
            std::process::exit(0);
        }
        other => other,
    };
    let mut client = Client::new(config::load(cli.profile.as_deref())?, cli.timeout);
    client.debug = cli.debug;
    if let Some(c) = &cli.context {
        client.context = Some(parse_object(c, "--context")?);
    }
    let c = &client;

    match cmd {
        Cmd::Setup(_) | Cmd::Guide => unreachable!(),

        Cmd::Whoami => {
            let p = &c.profile;
            let user = c.call("res.users", "read", Some(&[p.uid]), kw(vec![("fields", json!(["name", "login", "company_id", "tz", "lang"]))]))?;
            Ok(json!({
                "profile": p.name, "url": p.url, "db": p.db, "uid": p.uid, "partner_id": p.partner_id,
                "server_version": p.version, "protocol": p.protocol.as_str(),
                "user": user.get(0).cloned().unwrap_or(Value::Null),
            }))
        }

        Cmd::Models { filter } => {
            let mut domain = vec![json!(["transient", "=", false])];
            if let Some(f) = filter {
                domain.extend([json!("|"), json!(["model", "ilike", f]), json!(["name", "ilike", f])]);
            }
            let rows = c.call("ir.model", "search_read", None, kw(vec![
                ("domain", json!(domain)),
                ("fields", json!(["model", "name"])),
                ("order", json!("model")),
            ]))?;
            let map: Map<String, Value> = rows
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| Some((r["model"].as_str()?.to_string(), r["name"].clone())))
                .collect();
            Ok(json!({ "count": map.len(), "models": map }))
        }

        Cmd::Fields { model, search, only, verbose, refresh } => {
            let mut s = Schema::load(c, &model, refresh)?;
            let mut fields = s.fields.clone();
            if !only.is_empty() {
                s.check(c, only.iter().map(String::as_str))?;
                fields.retain(|k, _| only.contains(k));
            }
            Ok(json!({ "model": model, "fields": schema::compact(&fields, search.as_deref(), verbose) }))
        }

        Cmd::Search(a) => {
            let domain = parse_domain(a.domain.as_deref())?;
            let mut s = Schema::load(c, &a.model, false)?;
            let fields: Vec<&str> = if a.fields.is_empty() { vec!["id", "display_name"] } else { a.fields.iter().map(String::as_str).collect() };
            s.check(c, fields.iter().copied().chain(domain_fields(&domain)))?;
            if a.archived {
                let mut ctx = client.context.clone().unwrap_or_default();
                ctx.insert("active_test".into(), json!(false));
                client.context = Some(ctx);
            }
            search_page(&client, &a.model, domain, fields, a.limit, a.offset, a.order.as_deref())
        }

        Cmd::Access { model, ids } => {
            // 18+: has_access(operation) on the recordset (empty = model level,
            // with ids = record rules too). Older: check_access_rights.
            let modern = c.profile.major >= 18 || c.profile.major == 0;
            let ids = ids.as_deref().map(parse_ids).transpose()?;
            if ids.is_some() && !modern {
                return Err(Error::usage("--ids needs Odoo 18 or later"));
            }
            let mut out = Map::new();
            for mode in ["read", "write", "create", "unlink"] {
                let ok = if modern {
                    c.call(&model, "has_access", ids.as_deref(), kw(vec![("operation", json!(mode))]))?
                } else {
                    c.call(&model, "check_access_rights", None, kw(vec![("operation", json!(mode)), ("raise_exception", json!(false))]))?
                };
                out.insert(if mode == "unlink" { "delete".into() } else { mode.into() }, json!(ok.as_bool().unwrap_or(false)));
            }
            let scope = if ids.is_some() { "these records (model rights and record rules)" } else { "model-level rights; record rules can still restrict individual records" };
            Ok(json!({ "model": model, "ids": ids, "access": out, "scope": scope }))
        }

        Cmd::Ref { xmlid } => {
            let (module, name) = xmlid.split_once('.').ok_or_else(|| Error::usage("external id must look like module.name"))?;
            let rows = c.call("ir.model.data", "search_read", None, kw(vec![
                ("domain", json!([["module", "=", module], ["name", "=", name]])),
                ("fields", json!(["model", "res_id"])),
                ("limit", json!(1)),
            ]))?;
            let row = rows.get(0).ok_or_else(|| Error::odoo(format!("external id {xmlid} not found"), Value::Null))?;
            Ok(json!({ "xmlid": xmlid, "model": row["model"], "id": row["res_id"] }))
        }

        Cmd::Modules { filter, all } => {
            let mut domain = vec![];
            if !all {
                domain.push(json!(["state", "=", "installed"]));
            }
            if let Some(f) = filter {
                domain.extend([json!("|"), json!(["name", "ilike", f]), json!(["shortdesc", "ilike", f])]);
            }
            let rows = c.call("ir.module.module", "search_read", None, kw(vec![
                ("domain", json!(domain)),
                ("fields", json!(["name", "shortdesc", "state"])),
                ("order", json!("name")),
            ]))?;
            let list: Vec<Value> = rows
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| if all { json!([r["name"], r["shortdesc"], r["state"]]) } else { json!([r["name"], r["shortdesc"]]) })
                .collect();
            Ok(json!({ "count": list.len(), "modules": list }))
        }

        Cmd::Aggregate { model, domain, groupby, agg, limit, order } => {
            let domain = parse_domain(domain.as_deref())?;
            let field_of = |spec: &str| spec.split(':').next().unwrap_or("").to_string();
            let mut s = Schema::load(c, &model, false)?;
            let names: Vec<String> = groupby.iter().chain(agg.iter()).map(|g| field_of(g)).filter(|f| f != "__count").collect();
            s.check(c, names.iter().map(String::as_str).chain(domain_fields(&domain)))?;
            let mut groups = read_groups(c, &model, domain, &groupby, &agg, limit, order.as_deref())?;
            // Odoo 20 returns bare ids for many2one groups; add names so every
            // version answers `[id, "name"]` like search_read does.
            for g in &groupby {
                let Some(f) = s.fields.get(g.as_str()) else { continue }; // `date:month` etc.
                let (Some("many2one"), Some(rel)) = (f["type"].as_str(), f["relation"].as_str()) else { continue };
                let ids: Vec<i64> = groups.iter().filter_map(|r| r[g.as_str()].as_i64()).collect();
                if ids.is_empty() {
                    continue;
                }
                let names = c.call(rel, "read", Some(&ids), kw(vec![("fields", json!(["display_name"]))]))?;
                let by_id: std::collections::HashMap<i64, Value> = names
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| Some((r["id"].as_i64()?, r["display_name"].clone())))
                    .collect();
                for r in groups.iter_mut() {
                    if let Some(id) = r[g.as_str()].as_i64() {
                        r[g.as_str()] = json!([id, by_id.get(&id).cloned().unwrap_or(Value::Null)]);
                    }
                }
            }
            Ok(json!({ "model": model, "groupby": groupby, "aggregates": agg, "returned": groups.len(), "groups": groups }))
        }

        Cmd::Image { model, id, field, dir } => {
            let s = Schema::load(c, &model, false)?;
            let field = match field {
                Some(f) => {
                    let mut s = s;
                    s.check(c, [f.as_str()])?;
                    f
                }
                None => s
                    .existing(&["image_512", "image_1920", "image", "avatar_512", "logo"])
                    .first()
                    .map(|f| f.to_string())
                    .ok_or_else(|| Error::usage(format!("{model} has no image field; pass --field")))?,
            };
            attachments::save_field(c, &model, id, &field, &files_dir(c, dir.as_deref()))
        }

        Cmd::Count { model, domain, archived } => {
            let domain = parse_domain(domain.as_deref())?;
            Schema::load(c, &model, false)?.check(c, domain_fields(&domain))?;
            let mut k = kw(vec![("domain", json!(domain))]);
            if archived {
                k.insert("context".into(), json!({ "active_test": false }));
            }
            let n = c.call(&model, "search_count", None, k)?;
            Ok(json!({ "model": model, "count": n }))
        }

        Cmd::Get { model, ids, fields } => {
            let ids = parse_ids(&ids)?;
            let mut k = Map::new();
            if !fields.is_empty() {
                Schema::load(c, &model, false)?.check(c, fields.iter().map(String::as_str))?;
                k.insert("fields".into(), json!(fields));
            }
            let rows = c.call(&model, "read", Some(&ids), k)?;
            let found: Vec<i64> = rows.as_array().into_iter().flatten().filter_map(|r| r["id"].as_i64()).collect();
            let missing: Vec<i64> = ids.iter().copied().filter(|i| !found.contains(i)).collect();
            Ok(json!({ "model": model, "records": rows, "missing_ids": missing }))
        }

        Cmd::Create { model, values, dry_run } => {
            let vals = match parse_json(&values, "values")? {
                Value::Object(m) => vec![Value::Object(m)],
                Value::Array(a) if a.iter().all(Value::is_object) && !a.is_empty() => a,
                _ => return Err(Error::usage("values must be a JSON object or a non-empty array of objects")),
            };
            let s = {
                let mut s = Schema::load(c, &model, false)?;
                s.check(c, vals.iter().flat_map(|v| v.as_object().unwrap().keys().map(String::as_str)))?;
                s
            };
            s.warn_readonly(vals.iter().flat_map(|v| v.as_object().unwrap().keys().map(String::as_str)));
            let k = kw(vec![("vals_list", json!(vals))]);
            if dry_run {
                return Ok(dry(&model, "create", None, &k));
            }
            let res = c.call(&model, "create", None, k)?;
            Ok(json!({ "ok": true, "model": model, "ids": as_ids(&res) }))
        }

        Cmd::Update { model, ids, values, dry_run } => {
            let ids = parse_ids(&ids)?;
            let vals = parse_object(&values, "values")?;
            if vals.is_empty() {
                return Err(Error::usage("nothing to update"));
            }
            let mut s = Schema::load(c, &model, false)?;
            s.check(c, vals.keys().map(String::as_str))?;
            s.warn_readonly(vals.keys().map(String::as_str));
            let k = kw(vec![("vals", Value::Object(vals))]);
            if dry_run {
                return Ok(dry(&model, "write", Some(&ids), &k));
            }
            c.call(&model, "write", Some(&ids), k)?;
            Ok(json!({ "ok": true, "model": model, "ids": ids }))
        }

        Cmd::Delete { model, ids, yes, dry_run } => {
            let ids = parse_ids(&ids)?;
            if dry_run {
                return Ok(dry(&model, "unlink", Some(&ids), &Map::new()));
            }
            if !yes {
                return Err(Error::usage(format!("refusing to delete {model} {ids:?} without --yes")));
            }
            c.call(&model, "unlink", Some(&ids), Map::new())?;
            Ok(json!({ "ok": true, "model": model, "deleted_ids": ids }))
        }

        Cmd::Call { model, method, kwargs, ids, dry_run } => {
            let k = match kwargs {
                Some(a) => parse_object(&a, "kwargs")?,
                None => Map::new(),
            };
            let ids = ids.as_deref().map(parse_ids).transpose()?;
            if dry_run {
                return Ok(dry(&model, &method, ids.as_deref(), &k));
            }
            let res = c.call(&model, &method, ids.as_deref(), k)?;
            Ok(json!({ "model": model, "method": method, "result": res }))
        }

        Cmd::Tasks(a) => {
            let mut s = Schema::load(c, "project.task", false)?;
            let mut domain = parse_domain(a.domain.as_deref())?;
            if a.mine {
                let f = if s.has("user_ids") { "user_ids" } else { "user_id" };
                domain.push(json!([f, "in", [c.profile.uid]]));
            }
            if a.open {
                if s.has("state") && s.selection_has("state", "1_done") {
                    domain.push(json!(["state", "not in", ["1_done", "1_canceled"]]));
                }
                domain.push(json!(["stage_id.fold", "=", false]));
            }
            if let Some(p) = a.project {
                domain.push(json!(["project_id", "=", p]));
            }
            if let Some(t) = &a.search {
                domain.push(json!(["name", "ilike", t]));
            }
            if let Some(d) = &a.due_before {
                domain.push(json!(["date_deadline", "<=", d]));
            }
            let fields: Vec<&str> = if a.fields.is_empty() {
                s.existing(&["id", "name", "project_id", "stage_id", "state", "user_ids", "user_id", "priority", "date_deadline", "tag_ids"])
            } else {
                a.fields.iter().map(String::as_str).collect()
            };
            s.check(c, fields.iter().copied().chain(domain_fields(&domain)))?;
            let order: String = a
                .order
                .split(',')
                .filter(|part| part.split_whitespace().next().is_some_and(|f| s.has(f) || f == "id"))
                .collect::<Vec<_>>()
                .join(",");
            search_page(c, "project.task", domain, fields, a.limit, 0, (!order.is_empty()).then_some(order.as_str()))
        }

        Cmd::Timesheet(TimesheetCmd::Log { task, project, hours, description, date, employee, dry_run }) => {
            let project = match (task, project) {
                (_, Some(p)) => p,
                (Some(t), None) => {
                    let rows = c.call("project.task", "read", Some(&[t]), kw(vec![("fields", json!(["project_id"]))]))?;
                    rows.get(0)
                        .and_then(|r| r["project_id"].get(0))
                        .and_then(Value::as_i64)
                        .ok_or_else(|| Error::odoo(format!("task {t} not found or has no project"), Value::Null))?
                }
                (None, None) => return Err(Error::usage("give --task or --project")),
            };
            let mut vals = kw(vec![("name", json!(description)), ("unit_amount", json!(hours)), ("project_id", json!(project))]);
            if let Some(t) = task {
                vals.insert("task_id".into(), json!(t));
            }
            if let Some(d) = date {
                vals.insert("date".into(), json!(d));
            }
            if let Some(e) = employee {
                vals.insert("employee_id".into(), json!(e));
            }
            Schema::load(c, "account.analytic.line", false)?.check(c, vals.keys().map(String::as_str))?;
            let k = kw(vec![("vals_list", json!([vals]))]);
            if dry_run {
                return Ok(dry("account.analytic.line", "create", None, &k));
            }
            let res = c.call("account.analytic.line", "create", None, k)?;
            Ok(json!({ "ok": true, "model": "account.analytic.line", "ids": as_ids(&res) }))
        }

        Cmd::Timesheet(TimesheetCmd::List { mine, task, project, from, to, limit }) => {
            let mut domain = vec![json!(["project_id", "!=", false])];
            if mine {
                domain.push(json!(["user_id", "=", c.profile.uid]));
            }
            if let Some(t) = task {
                domain.push(json!(["task_id", "=", t]));
            }
            if let Some(p) = project {
                domain.push(json!(["project_id", "=", p]));
            }
            if let Some(f) = from {
                domain.push(json!(["date", ">=", f]));
            }
            if let Some(t) = to {
                domain.push(json!(["date", "<=", t]));
            }
            let s = Schema::load(c, "account.analytic.line", false)?;
            let fields = s.existing(&["id", "date", "employee_id", "project_id", "task_id", "name", "unit_amount"]);
            let mut page = search_page(c, "account.analytic.line", domain, fields, limit, 0, Some("date desc, id desc"))?;
            let total: f64 = page["records"].as_array().into_iter().flatten().filter_map(|r| r["unit_amount"].as_f64()).sum();
            page["hours_in_page"] = json!((total * 100.0).round() / 100.0);
            Ok(page)
        }

        Cmd::Chatter(ChatterCmd::Read { model, id, limit, html, files }) => message_rows(c, &model, id, limit, html, &files),

        Cmd::Chatter(ChatterCmd::Post { model, id, body, note, html, partners, attach, dry_run }) => {
            post_message(c, &model, id, read_input(&body)?, note, html, partners.as_deref(), &attach, dry_run)
        }

        Cmd::Chat(ChatCmd::Channels { search, limit }) => {
            let m = channel_model(c);
            let s = Schema::load(c, m, false)?;
            let mut domain = vec![];
            if s.has("is_member") {
                domain.push(json!(["is_member", "=", true]));
            }
            if let Some(t) = search {
                domain.push(json!(["name", "ilike", t]));
            }
            let fields = s.existing(&["id", "name", "channel_type", "description"]);
            search_page(c, m, domain, fields, limit, 0, Some("id desc"))
        }

        Cmd::Chat(ChatCmd::Read { channel, limit, html, files }) => message_rows(c, channel_model(c), channel, limit, html, &files),

        Cmd::Chat(ChatCmd::Send { channel, body, html, attach, dry_run }) => {
            post_message(c, channel_model(c), channel, read_input(&body)?, false, html, None, &attach, dry_run)
        }

        Cmd::Attachment(AttachmentCmd::List { model, id }) => {
            let rows = c.call("ir.attachment", "search_read", None, kw(vec![
                ("domain", json!([["res_model", "=", model], ["res_id", "=", id]])),
                ("fields", json!(["id", "name", "mimetype", "file_size", "type", "create_date"])),
                ("order", json!("id desc")),
            ]))?;
            Ok(json!({ "model": model, "res_id": id, "attachments": rows }))
        }

        Cmd::Attachment(AttachmentCmd::Upload { model, id, files }) => {
            for f in &files {
                if !std::path::Path::new(f).is_file() {
                    return Err(Error::usage(format!("no such file {f}")));
                }
            }
            let ids = files.iter().map(|f| attachments::upload(c, f, &model, id)).collect::<Result<Vec<i64>>>()?;
            Ok(json!({ "ok": true, "model": model, "res_id": id, "attachment_ids": ids }))
        }

        Cmd::Attachment(AttachmentCmd::Get { ids, dir, base64, max_bytes }) => {
            let ids = parse_ids(&ids)?;
            let meta = attachments::metadata(c, &ids)?;
            let found: Vec<i64> = meta.iter().filter_map(|m| m["id"].as_i64()).collect();
            let missing: Vec<i64> = ids.iter().copied().filter(|i| !found.contains(i)).collect();
            let files = attachments::download(c, meta, &files_dir(c, dir.as_deref()), base64, max_bytes)?;
            Ok(json!({ "attachments": files, "missing_ids": missing }))
        }
    }
}

fn main() {
    let cli = Cli::parse();
    let pretty = cli.pretty;
    let out = |v: &Value| if pretty { serde_json::to_string_pretty(v).unwrap() } else { v.to_string() };
    match run(cli) {
        Ok(v) => {
            let mut stdout = std::io::stdout().lock();
            let _ = writeln!(stdout, "{}", out(&v));
        }
        Err(e) => {
            eprintln!("{}", out(&e.to_json()));
            std::process::exit(e.exit_code());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_readable_text() {
        let t = html_to_text("<p>Hi &amp; welcome</p><p>line&nbsp;2<br/>line 3</p>");
        assert_eq!(t, "Hi & welcome\nline 2\nline 3");
    }

    #[test]
    fn images_leave_markers() {
        let t = html_to_text(r#"<p>Bug:</p><p><img src="/web/image/412-abc/s.png" alt="x"></p><img src="data:image/png;base64,AA">"#);
        assert_eq!(t, "Bug:\n[image #412]\n[image]");
    }

    #[test]
    fn ids_parse_and_reject() {
        assert_eq!(parse_ids("1, 2,3").unwrap(), vec![1, 2, 3]);
        assert!(parse_ids("").is_err());
        assert!(parse_ids("1,x").is_err());
    }

    #[test]
    fn domain_fields_skip_operators() {
        let d: Vec<Value> = serde_json::from_str(r#"["|", ["name","ilike","a"], ["stage_id.fold","=",false]]"#).unwrap();
        assert_eq!(domain_fields(&d), vec!["name", "stage_id.fold"]);
    }
}
