//! Profiles live in `~/.config/odoo-cli/config.json`. The API key lives next to
//! it in `<profile>.key` (mode 0600), or comes from `ODOO_API_KEY`. No
//! encryption: anything that can read the file could also run this binary.

use crate::error::{Error, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Protocol {
    /// Odoo 19+: `POST /json/2/<model>/<method>` with a bearer API key.
    Json2,
    /// Odoo 8-18 (deprecated in 19): `POST /jsonrpc` with `execute_kw`.
    JsonRpc,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Json2 => "json2",
            Protocol::JsonRpc => "jsonrpc",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "json2" => Some(Protocol::Json2),
            "jsonrpc" => Some(Protocol::JsonRpc),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub url: String,
    pub db: Option<String>,
    pub login: String,
    pub uid: i64,
    pub partner_id: Option<i64>,
    pub protocol: Protocol,
    pub version: String,
    pub major: i64,
    pub api_key: String,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into()))
}

pub fn config_dir() -> PathBuf {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p).join("odoo-cli"),
        _ => home().join(".config/odoo-cli"),
    }
}

pub fn cache_dir() -> PathBuf {
    match std::env::var_os("XDG_CACHE_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p).join("odoo-cli"),
        _ => home().join(".cache/odoo-cli"),
    }
}

fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn key_path(profile: &str) -> PathBuf {
    config_dir().join(format!("{profile}.key"))
}

fn read_config() -> Result<Map<String, Value>> {
    match fs::read_to_string(config_path()) {
        Ok(s) => match serde_json::from_str::<Value>(&s) {
            Ok(Value::Object(m)) => Ok(m),
            _ => Err(Error::config(format!("{} is not a JSON object", config_path().display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(Error::config(format!("cannot read {}: {e}", config_path().display()))),
    }
}

fn write_private(path: &PathBuf, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir).map_err(|e| Error::config(format!("cannot create {}: {e}", dir.display())))?;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    let tmp = path.with_extension("tmp");
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| Error::config(format!("cannot write {}: {e}", tmp.display())))?;
    f.write_all(data).map_err(|e| Error::config(e.to_string()))?;
    drop(f);
    fs::rename(&tmp, path).map_err(|e| Error::config(e.to_string()))
}

/// Which profile to use: `--profile`, then `ODOO_PROFILE`, then the saved default.
pub fn resolve_name(explicit: Option<&str>) -> Result<String> {
    if let Some(p) = explicit {
        return Ok(p.to_string());
    }
    if let Ok(p) = std::env::var("ODOO_PROFILE") {
        if !p.is_empty() {
            return Ok(p);
        }
    }
    let cfg = read_config()?;
    Ok(cfg.get("default").and_then(Value::as_str).unwrap_or("default").to_string())
}

pub fn load(explicit: Option<&str>) -> Result<Profile> {
    let name = resolve_name(explicit)?;
    let cfg = read_config()?;
    let p = cfg
        .get("profiles")
        .and_then(|p| p.get(&name))
        .ok_or_else(|| {
            Error::config(format!(
                "profile '{name}' is not set up; run: odoo-cli setup --url URL --db DB --login LOGIN"
            ))
        })?;
    let s = |k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
    let api_key = match std::env::var("ODOO_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => fs::read_to_string(key_path(&name))
            .map(|k| k.trim().to_string())
            .map_err(|_| Error::config(format!("no API key: set ODOO_API_KEY or rerun setup for '{name}'")))?,
    };
    Ok(Profile {
        url: s("url").ok_or_else(|| Error::config("profile has no url"))?,
        db: s("db"),
        login: s("login").unwrap_or_default(),
        uid: p.get("uid").and_then(Value::as_i64).unwrap_or(0),
        partner_id: p.get("partner_id").and_then(Value::as_i64),
        protocol: s("protocol").as_deref().and_then(Protocol::parse).unwrap_or(Protocol::Json2),
        version: s("version").unwrap_or_default(),
        major: p.get("major").and_then(Value::as_i64).unwrap_or(0),
        api_key,
        name,
    })
}

pub fn save(p: &Profile, make_default: bool) -> Result<()> {
    let mut cfg = read_config()?;
    let profiles = cfg.entry("profiles").or_insert_with(|| json!({}));
    if !profiles.is_object() {
        *profiles = json!({});
    }
    profiles.as_object_mut().unwrap().insert(
        p.name.clone(),
        json!({
            "url": p.url, "db": p.db, "login": p.login, "uid": p.uid,
            "partner_id": p.partner_id, "protocol": p.protocol.as_str(),
            "version": p.version, "major": p.major,
        }),
    );
    if make_default || !cfg.contains_key("default") {
        cfg.insert("default".into(), Value::String(p.name.clone()));
    }
    let data = serde_json::to_vec_pretty(&Value::Object(cfg)).unwrap();
    write_private(&config_path(), &data)?;
    write_private(&key_path(&p.name), p.api_key.as_bytes())
}
