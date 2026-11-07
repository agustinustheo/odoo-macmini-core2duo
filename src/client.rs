//! Transport. One blocking HTTP agent per process, no async runtime: the CLI
//! makes one or two calls and exits, so a thread pool would only cost RAM.

use crate::config::{Profile, Protocol};
use crate::error::{Error, Result};
use serde_json::{json, Map, Value};
use std::time::Duration;

const MAX_BODY: u64 = 256 << 20;

pub struct Client {
    agent: ureq::Agent,
    pub profile: Profile,
    /// Extra context merged into every call (`--context`).
    pub context: Option<Map<String, Value>>,
    /// Include Odoo's server traceback in errors (`--debug`).
    pub debug: bool,
}

pub fn agent(timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .http_status_as_error(false)
        .user_agent(concat!("odoo-cli/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

pub fn base(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

/// POST JSON and return (status, parsed body or raw text).
pub fn post(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<(u16, std::result::Result<Value, String>)> {
    let mut req = agent.post(url).header("Content-Type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let payload = serde_json::to_vec(body).unwrap();
    let mut resp = req.send(&payload[..]).map_err(|e| Error::network(format!("{url}: {e}")))?;
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_string()
        .map_err(|e| Error::network(format!("{url}: reading response: {e}")))?;
    Ok((status, serde_json::from_str::<Value>(&text).map_err(|_| text)))
}

/// Call an unauthenticated JSON-RPC route like `/web/webclient/version_info`.
pub fn jsonrpc_route(agent: &ureq::Agent, url: &str, params: Value) -> Result<Value> {
    let body = json!({ "jsonrpc": "2.0", "method": "call", "params": params, "id": 1 });
    let (status, parsed) = post(agent, url, &[], &body)?;
    match parsed {
        Ok(v) => {
            if let Some(err) = v.get("error") {
                return Err(rpc_error(err, false));
            }
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        }
        Err(text) => Err(non_json(url, status, &text)),
    }
}

fn non_json(url: &str, status: u16, text: &str) -> Error {
    let snippet: String = text.chars().take(200).collect();
    Error::network(format!("{url}: HTTP {status}, not a JSON response")).with_detail(json!({
        "http_status": status, "body_start": snippet
    }))
}

/// Normalise a JSON-RPC `error` object into our error.
fn rpc_error(err: &Value, debug: bool) -> Error {
    let data = err.get("data").unwrap_or(&Value::Null);
    let message = data
        .get("message")
        .or_else(|| err.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("Odoo error")
        .to_string();
    let name = data.get("name").and_then(Value::as_str).unwrap_or("");
    let mut detail = json!({ "odoo_error": name });
    if debug {
        detail["debug"] = data.get("debug").cloned().unwrap_or(Value::Null);
    }
    classify(name, message, detail)
}

fn classify(name: &str, message: String, detail: Value) -> Error {
    if name.ends_with("AccessDenied") || name.ends_with("SessionExpiredException") {
        Error::config(format!("authentication failed: {message}")).with_detail(detail)
    } else {
        Error::odoo(message, detail)
    }
}

impl Client {
    pub fn new(profile: Profile, timeout_secs: u64) -> Self {
        Client { agent: agent(timeout_secs), profile, context: None, debug: false }
    }

    /// Call `model.method`. `ids` is the recordset (`self`) for record methods;
    /// `kwargs` are named arguments. Both protocols take the same shape.
    pub fn call(&self, model: &str, method: &str, ids: Option<&[i64]>, kwargs: Map<String, Value>) -> Result<Value> {
        let kwargs = self.with_context(kwargs);
        match self.profile.protocol {
            Protocol::Json2 => self.call_json2(model, method, ids, kwargs),
            Protocol::JsonRpc => self.call_jsonrpc(model, method, ids, kwargs),
        }
    }

    /// `kwargs` as they are sent: the global context (`--context`, `--company`)
    /// merged into `kwargs.context`, where keys already set by the command win.
    pub fn with_context(&self, mut kwargs: Map<String, Value>) -> Map<String, Value> {
        if let Some(ctx) = &self.context {
            let entry = kwargs.entry("context").or_insert_with(|| json!({}));
            if let Value::Object(c) = entry {
                for (k, v) in ctx {
                    c.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }
        kwargs
    }

    fn call_json2(&self, model: &str, method: &str, ids: Option<&[i64]>, mut kwargs: Map<String, Value>) -> Result<Value> {
        if let Some(ids) = ids {
            kwargs.insert("ids".into(), json!(ids));
        }
        let url = format!("{}/json/2/{model}/{method}", base(&self.profile.url));
        let auth = format!("bearer {}", self.profile.api_key);
        let mut headers: Vec<(&str, &str)> = vec![("Authorization", &auth)];
        if let Some(db) = &self.profile.db {
            headers.push(("X-Odoo-Database", db));
        }
        let (status, parsed) = post(&self.agent, &url, &headers, &Value::Object(kwargs))?;
        match parsed {
            Ok(v) if (200..300).contains(&status) => Ok(v),
            Ok(v) => {
                let name = v.get("name").and_then(Value::as_str).unwrap_or("");
                let message = v
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("HTTP {status}"));
                let mut detail = json!({ "odoo_error": name, "http_status": status });
                if self.debug {
                    detail["debug"] = v.get("debug").cloned().unwrap_or(Value::Null);
                }
                if status == 401 {
                    return Err(Error::config(format!("authentication failed: {message}")).with_detail(detail));
                }
                Err(classify(name, message, detail))
            }
            Err(text) => {
                if status == 404 {
                    return Err(Error::odoo(
                        format!("{model}.{method} not found (unknown model or method, or JSON-2 unavailable)"),
                        json!({ "http_status": 404 }),
                    ));
                }
                Err(non_json(&url, status, &text))
            }
        }
    }

    fn call_jsonrpc(&self, model: &str, method: &str, ids: Option<&[i64]>, kwargs: Map<String, Value>) -> Result<Value> {
        let db = self
            .profile
            .db
            .as_deref()
            .ok_or_else(|| Error::config("the jsonrpc protocol needs a database name; rerun setup with --db"))?;
        let args = match ids {
            Some(ids) => json!([ids]),
            None => json!([]),
        };
        let params = json!({
            "service": "object",
            "method": "execute_kw",
            "args": [db, self.profile.uid, self.profile.api_key, model, method, args, kwargs],
        });
        let body = json!({ "jsonrpc": "2.0", "method": "call", "params": params, "id": 1 });
        let url = format!("{}/jsonrpc", base(&self.profile.url));
        let (status, parsed) = post(&self.agent, &url, &[], &body)?;
        match parsed {
            Ok(v) => {
                if let Some(err) = v.get("error") {
                    return Err(rpc_error(err, self.debug));
                }
                Ok(v.get("result").cloned().unwrap_or(Value::Null))
            }
            Err(text) => Err(non_json(&url, status, &text)),
        }
    }
}

/// `common.authenticate` for the legacy protocol. Returns the uid.
pub fn jsonrpc_authenticate(agent: &ureq::Agent, url: &str, db: &str, login: &str, key: &str) -> Result<i64> {
    let res = jsonrpc_route(
        agent,
        &format!("{}/jsonrpc", base(url)),
        json!({ "service": "common", "method": "authenticate", "args": [db, login, key, {}] }),
    )?;
    res.as_i64()
        .filter(|uid| *uid > 0)
        .ok_or_else(|| Error::config("authentication failed: check --db, --login and the API key"))
}
