//! Field metadata cache. `fields_get` is the slowest call an agent makes over
//! and over, so it is stored on disk per profile and model. Field names are
//! checked against it before any request is sent. A name missing from the
//! cache triggers one refresh, so a stale cache never rejects a real field.

use crate::client::Client;
use crate::config::cache_dir;
use crate::error::{Error, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const TTL: Duration = Duration::from_secs(24 * 3600);
const ATTRS: &[&str] = &["string", "type", "required", "readonly", "relation", "selection", "store", "help"];

pub struct Schema {
    pub model: String,
    pub fields: Map<String, Value>,
    fresh: bool,
}

fn path(profile: &str, model: &str) -> PathBuf {
    cache_dir().join(profile).join(format!("{model}.json"))
}

fn fetch(client: &Client, model: &str) -> Result<Map<String, Value>> {
    let mut kw = Map::new();
    kw.insert("attributes".into(), json!(ATTRS));
    let v = client.call(model, "fields_get", None, kw)?;
    let fields = match v {
        Value::Object(m) => m,
        _ => return Err(Error::odoo(format!("fields_get on {model} returned no fields"), Value::Null)),
    };
    let p = path(&client.profile.name, model);
    if let Some(dir) = p.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(&p, serde_json::to_vec(&fields).unwrap());
    Ok(fields)
}

impl Schema {
    pub fn load(client: &Client, model: &str, refresh: bool) -> Result<Schema> {
        let p = path(&client.profile.name, model);
        if !refresh {
            let young = fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .is_some_and(|age| age < TTL);
            if young {
                if let Ok(Value::Object(m)) = fs::read(&p).map(|b| serde_json::from_slice(&b).unwrap_or(Value::Null)) {
                    return Ok(Schema { model: model.into(), fields: m, fresh: false });
                }
            }
        }
        Ok(Schema { model: model.into(), fields: fetch(client, model)?, fresh: true })
    }

    pub fn has(&self, f: &str) -> bool {
        self.fields.contains_key(f)
    }

    fn refresh(&mut self, client: &Client) -> Result<()> {
        if !self.fresh {
            self.fields = fetch(client, &self.model)?;
            self.fresh = true;
        }
        Ok(())
    }

    /// Ensure every name exists. Dotted paths (`stage_id.name`) are checked on
    /// the first segment only; Odoo checks the rest.
    pub fn check<'a, I: IntoIterator<Item = &'a str>>(&mut self, client: &Client, names: I) -> Result<()> {
        let names: Vec<&str> = names.into_iter().collect();
        let missing = |s: &Schema| -> Vec<String> {
            names
                .iter()
                .map(|n| n.split('.').next().unwrap())
                .filter(|n| !n.is_empty() && *n != "id" && !s.has(n))
                .map(str::to_string)
                .collect()
        };
        let mut bad = missing(self);
        if !bad.is_empty() {
            self.refresh(client)?;
            bad = missing(self);
        }
        if bad.is_empty() {
            return Ok(());
        }
        let suggestions: Map<String, Value> =
            bad.iter().map(|b| (b.clone(), json!(self.suggest(b)))).collect();
        Err(Error::usage(format!("unknown field(s) on {}: {}", self.model, bad.join(", "))).with_detail(json!({
            "model": self.model,
            "unknown_fields": bad,
            "did_you_mean": suggestions,
            "hint": format!("list fields with: odoo-cli fields {} --search TEXT", self.model),
        })))
    }

    /// Warn on stderr (do not fail) when writing fields Odoo marks read-only.
    pub fn warn_readonly<'a, I: IntoIterator<Item = &'a str>>(&self, names: I) {
        let ro: Vec<&str> = names
            .into_iter()
            .filter(|n| self.fields.get(*n).and_then(|f| f.get("readonly")).and_then(Value::as_bool) == Some(true))
            .collect();
        if !ro.is_empty() {
            eprintln!(
                "{}",
                json!({ "warning": format!("read-only in the Odoo UI, the value may be ignored or recomputed: {}", ro.join(", ")) })
            );
        }
    }

    /// Keep only fields that exist; used for built-in default field lists so
    /// shortcuts work across Odoo versions.
    pub fn existing<'a>(&self, wanted: &[&'a str]) -> Vec<&'a str> {
        wanted.iter().copied().filter(|f| *f == "id" || self.has(f)).collect()
    }

    pub fn selection_has(&self, field: &str, value: &str) -> bool {
        self.fields
            .get(field)
            .and_then(|f| f.get("selection"))
            .and_then(Value::as_array)
            .is_some_and(|opts| opts.iter().any(|o| o.get(0).and_then(Value::as_str) == Some(value)))
    }

    fn suggest(&self, bad: &str) -> Vec<String> {
        let b = bad.to_lowercase();
        // Rank: `<bad>_id(s)` first, then prefix, then substring/label, then
        // edit distance. Lower is better.
        let mut scored: Vec<(usize, &String)> = self
            .fields
            .iter()
            .filter_map(|(name, f)| {
                let label = f.get("string").and_then(Value::as_str).unwrap_or("").to_lowercase();
                let d = levenshtein(&b, name);
                let score = if *name == format!("{b}_id") || *name == format!("{b}_ids") {
                    0
                } else if name.starts_with(&b) {
                    1
                } else if name.contains(&b) || (b.len() >= 3 && label.contains(&b)) {
                    2
                } else if d <= 2 + b.len() / 4 {
                    3 + d
                } else {
                    return None;
                };
                Some((score, name))
            })
            .collect();
        scored.sort();
        scored.into_iter().take(5).map(|(_, n)| n.clone()).collect()
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + (ca != *cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Compact view of fields for `odoo-cli fields`: drops empty/false attributes.
pub fn compact(fields: &Map<String, Value>, search: Option<&str>, verbose: bool) -> Map<String, Value> {
    let q = search.map(str::to_lowercase);
    let mut names: Vec<&String> = fields.keys().collect();
    names.sort();
    names
        .into_iter()
        .filter(|n| {
            q.as_ref().is_none_or(|q| {
                n.contains(q.as_str())
                    || fields[*n].get("string").and_then(Value::as_str).is_some_and(|s| s.to_lowercase().contains(q.as_str()))
            })
        })
        .map(|n| {
            let mut out = Map::new();
            if let Value::Object(f) = &fields[n] {
                for (k, v) in f {
                    let skip = matches!(v, Value::Bool(false) | Value::Null)
                        || (k == "help" && !verbose)
                        || (k == "store" && v == &Value::Bool(true));
                    if !skip {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            (n.clone(), Value::Object(out))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(names: &[&str]) -> Schema {
        let fields = names.iter().map(|n| (n.to_string(), json!({ "string": n, "type": "char" }))).collect();
        Schema { model: "x".into(), fields, fresh: true }
    }

    #[test]
    fn suggests_many2one_for_bare_name() {
        let s = schema(&["date_last_stage_update", "personal_stage_id", "stage_id", "name"]);
        assert_eq!(s.suggest("stage")[0], "stage_id");
    }

    #[test]
    fn suggests_typo_fix() {
        let s = schema(&["date_deadline", "name", "priority"]);
        assert_eq!(s.suggest("priorty")[0], "priority");
    }

    #[test]
    fn existing_keeps_only_known_fields() {
        let s = schema(&["user_ids", "name"]);
        assert_eq!(s.existing(&["id", "name", "user_id", "user_ids"]), vec!["id", "name", "user_ids"]);
    }

    #[test]
    fn compact_drops_noise() {
        let mut m = Map::new();
        m.insert("name".into(), json!({ "string": "Name", "type": "char", "required": true, "readonly": false, "store": true, "help": "long" }));
        let c = compact(&m, None, false);
        assert_eq!(c["name"], json!({ "string": "Name", "type": "char", "required": true }));
    }
}
