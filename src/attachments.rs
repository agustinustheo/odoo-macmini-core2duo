//! Attachments on messages and records. Files are written to disk and the CLI
//! prints their paths, so an agent can open an image with its file reader
//! instead of pulling megabytes of base64 through its context.

use crate::client::Client;
use crate::config::cache_dir;
use crate::error::{Error, Result};
use crate::schema::Schema;
use base64::Engine;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const META: &[&str] = &["id", "name", "mimetype", "file_size", "res_model", "res_id", "type", "url"];

/// Odoo 20 dropped `datas`; file content goes through `raw`, which reads back
/// as `{"filename", "content": <base64>, "size"}`. Older versions use `datas`
/// (a base64 string). Pick whichever field this server has.
fn content_field(client: &Client) -> Result<&'static str> {
    let s = Schema::load(client, "ir.attachment", false)?;
    Ok(if s.has("datas") { "datas" } else { "raw" })
}

/// Base64 text from either shape of the content field.
fn content_b64(v: &Value) -> Option<&str> {
    v.as_str().or_else(|| v.get("content").and_then(Value::as_str))
}

pub fn default_dir(profile: &str) -> PathBuf {
    cache_dir().join(profile).join("attachments")
}

/// Attachment ids referenced inline in an HTML body: `/web/image/123...` and
/// `/web/content/123...` (pasted images are stored as attachments).
pub fn inline_ids(html: &str) -> Vec<i64> {
    let mut out = Vec::new();
    for marker in ["/web/image/", "/web/content/"] {
        let mut rest = html;
        while let Some(pos) = rest.find(marker) {
            rest = &rest[pos + marker.len()..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            // `/web/image/res.partner/3/avatar` style URLs are not attachment ids.
            let next = rest[digits.len()..].chars().next();
            if !digits.is_empty() && matches!(next, None | Some('-' | '/' | '?' | '"' | '\'' | ' ')) {
                if let Ok(id) = digits.parse() {
                    out.push(id);
                }
            }
        }
    }
    out
}

/// Metadata for attachment ids, in the order given. Ids the user cannot read
/// are reported, not fatal.
pub fn metadata(client: &Client, ids: &[i64]) -> Result<Vec<Value>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let mut k = Map::new();
    k.insert("fields".into(), json!(META));
    let rows = client.call("ir.attachment", "read", Some(ids), k)?;
    Ok(rows.as_array().cloned().unwrap_or_default())
}

/// A safe file name: `<id>-<name>`, with an extension from the mimetype when
/// the name has none, so image viewers recognise the file.
pub fn file_name(id: i64, name: &str, mimetype: &str) -> String {
    let mut clean: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    clean = clean.trim_matches(|c| c == '.' || c == '_').chars().take(120).collect();
    if clean.is_empty() {
        clean = "file".into();
    }
    if !clean.contains('.') {
        if let Some(ext) = ext_for(mimetype) {
            clean.push('.');
            clean.push_str(ext);
        }
    }
    format!("{id}-{clean}")
}

fn ext_for(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/heic" => "heic",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "text/csv" => "csv",
        _ => return None,
    })
}

/// Download attachments to `dir`. Skips the transfer when the file is already
/// there with the right size (attachments are immutable in practice).
/// Returns metadata rows with `path` added, or `data_base64` when `inline_b64`.
pub fn download(client: &Client, rows: Vec<Value>, dir: &Path, inline_b64: bool, max_bytes: u64) -> Result<Vec<Value>> {
    fs::create_dir_all(dir).map_err(|e| Error::usage(format!("cannot create {}: {e}", dir.display())))?;
    let field = if rows.is_empty() { "datas" } else { content_field(client)? };
    let mut out = Vec::with_capacity(rows.len());
    for mut row in rows {
        let id = row["id"].as_i64().unwrap_or(0);
        let size = row["file_size"].as_u64().unwrap_or(0);
        if row["type"].as_str() == Some("url") {
            row["skipped"] = json!("link attachment, open its url instead");
            out.push(row);
            continue;
        }
        if size > max_bytes {
            row["skipped"] = json!(format!("{size} bytes is over --max-bytes {max_bytes}"));
            out.push(row);
            continue;
        }
        let name = file_name(id, row["name"].as_str().unwrap_or(""), row["mimetype"].as_str().unwrap_or(""));
        let path = dir.join(&name);
        let cached = !inline_b64 && size > 0 && fs::metadata(&path).is_ok_and(|m| m.len() == size);
        if !cached {
            let mut k = Map::new();
            k.insert("fields".into(), json!([field]));
            let data = client.call("ir.attachment", "read", Some(&[id]), k)?;
            let b64 = data.get(0).and_then(|r| content_b64(&r[field])).unwrap_or("");
            if inline_b64 {
                row["data_base64"] = json!(b64);
                out.push(row);
                continue;
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.as_bytes())
                .map_err(|e| Error::odoo(format!("attachment {id}: bad base64 from server: {e}"), Value::Null))?;
            let tmp = path.with_extension("part");
            fs::write(&tmp, &bytes)
                .and_then(|_| fs::rename(&tmp, &path))
                .map_err(|e| Error::usage(format!("writing {}: {e}", path.display())))?;
        }
        row["path"] = json!(path.display().to_string());
        row["cached"] = json!(cached);
        out.push(row);
    }
    Ok(out)
}

/// Create an attachment from a local file. With `res_model = "mail.compose.message"`
/// and `res_id = 0` it is unlinked, so `message_post(attachment_ids=...)` can
/// attach it to the new message.
pub fn upload(client: &Client, path: &str, res_model: &str, res_id: i64) -> Result<i64> {
    let bytes = fs::read(path).map_err(|e| Error::usage(format!("reading {path}: {e}")))?;
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
    let mut vals = json!({
        "name": name,
        "res_model": res_model,
        "res_id": res_id,
    });
    vals[content_field(client)?] = json!(base64::engine::general_purpose::STANDARD.encode(&bytes));
    let mut k = Map::new();
    k.insert("vals_list".into(), json!([vals]));
    let res = client.call("ir.attachment", "create", None, k)?;
    res.as_i64()
        .or_else(|| res.get(0).and_then(Value::as_i64))
        .ok_or_else(|| Error::odoo(format!("uploading {path} returned no id"), json!({ "result": res })))
}

/// Attachment ids for a page of messages: `attachment_ids` plus inline images,
/// de-duplicated, keeping first-seen order.
pub fn ids_in_messages(messages: &[Value]) -> Vec<i64> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for m in messages {
        let listed = m["attachment_ids"].as_array().into_iter().flatten().filter_map(Value::as_i64);
        let inline = m["body"].as_str().map(inline_ids).unwrap_or_default();
        for id in listed.chain(inline) {
            if seen.insert(id) {
                out.push(id);
            }
        }
    }
    out
}

/// Image format from magic bytes (never from the field name).
pub fn sniff(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    Some(match bytes {
        [0x89, b'P', b'N', b'G', ..] => ("image/png", "png"),
        [0xFF, 0xD8, 0xFF, ..] => ("image/jpeg", "jpg"),
        [b'G', b'I', b'F', b'8', ..] => ("image/gif", "gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => ("image/webp", "webp"),
        [b'%', b'P', b'D', b'F', ..] => ("application/pdf", "pdf"),
        b if b.starts_with(b"<?xml") || b.starts_with(b"<svg") => ("image/svg+xml", "svg"),
        _ => return None,
    })
}

/// Save a binary/image field of a record to disk.
pub fn save_field(client: &Client, model: &str, id: i64, field: &str, dir: &Path) -> Result<Value> {
    let mut k = Map::new();
    k.insert("fields".into(), json!([field]));
    let rows = client.call(model, "read", Some(&[id]), k)?;
    let row = rows.get(0).ok_or_else(|| Error::odoo(format!("{model} {id} not found"), Value::Null))?;
    let Some(b64) = content_b64(&row[field]) else {
        return Ok(json!({ "model": model, "id": id, "field": field, "empty": true }));
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| Error::odoo(format!("{model} {id} {field}: bad base64: {e}"), Value::Null))?;
    let (mime, ext) = sniff(&bytes).unwrap_or(("application/octet-stream", "bin"));
    fs::create_dir_all(dir).map_err(|e| Error::usage(format!("cannot create {}: {e}", dir.display())))?;
    let path = dir.join(format!("{}-{id}-{field}.{ext}", model.replace('.', "_")));
    fs::write(&path, &bytes).map_err(|e| Error::usage(format!("writing {}: {e}", path.display())))?;
    Ok(json!({ "model": model, "id": id, "field": field, "mimetype": mime, "file_size": bytes.len(), "path": path.display().to_string() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_inline_image_ids() {
        let html = r#"<p>see <img src="/web/image/412-9a1b2c3d/screenshot.png?access_token=x"> and
            <a href="/web/content/77?download=true">file</a>
            <img src="/web/image/res.partner/3/avatar_128"></p>"#;
        assert_eq!(inline_ids(html), vec![412, 77]);
    }

    #[test]
    fn sniffs_formats() {
        assert_eq!(sniff(b"\x89PNG\r\n").unwrap().1, "png");
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0").unwrap().1, "jpg");
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8").unwrap().1, "webp");
        assert_eq!(sniff(b"%PDF-1.7").unwrap().1, "pdf");
        assert!(sniff(b"hello").is_none());
    }

    #[test]
    fn reads_both_content_shapes() {
        assert_eq!(content_b64(&json!("aGk=")), Some("aGk="));
        assert_eq!(content_b64(&json!({ "filename": "a.png", "content": "aGk=", "size": 2 })), Some("aGk="));
        assert_eq!(content_b64(&json!(false)), None);
    }

    #[test]
    fn ignores_bodies_without_images() {
        assert!(inline_ids("<p>plain text 123</p>").is_empty());
    }

    #[test]
    fn file_names_are_safe_and_typed() {
        assert_eq!(file_name(5, "image.png", "image/png"), "5-image.png");
        assert_eq!(file_name(6, "../../etc/passwd", "text/plain"), "6-etc_passwd.txt");
        assert_eq!(file_name(7, "Screenshot 2026-11-06", "image/jpeg"), "7-Screenshot_2026-11-06.jpg");
        assert_eq!(file_name(8, "", "application/octet-stream"), "8-file");
    }

    #[test]
    fn message_ids_are_deduplicated() {
        let msgs = vec![
            json!({ "attachment_ids": [3, 4], "body": "<img src=\"/web/image/4\">" }),
            json!({ "attachment_ids": [], "body": "<img src='/web/image/9-abc'>" }),
        ];
        assert_eq!(ids_in_messages(&msgs), vec![3, 4, 9]);
    }
}
