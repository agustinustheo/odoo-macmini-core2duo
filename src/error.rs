//! One error type. Every failure is printed to stderr as a single JSON object
//! and maps to a stable exit code, so calling agents can branch on it.

use serde_json::{json, Value};

#[derive(Debug)]
pub enum Kind {
    /// Odoo refused the call (access, validation, missing record...). Exit 1.
    Odoo,
    /// Bad arguments, unknown field, bad JSON input. Exit 2.
    Usage,
    /// Missing or broken profile / API key / authentication. Exit 3.
    Config,
    /// DNS, TLS, timeout, non-Odoo HTTP response. Exit 4.
    Network,
}

#[derive(Debug)]
pub struct Error {
    pub kind: Kind,
    pub message: String,
    pub detail: Value,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    fn new(kind: Kind, message: impl Into<String>) -> Self {
        Error { kind, message: message.into(), detail: Value::Null }
    }
    pub fn usage(m: impl Into<String>) -> Self {
        Self::new(Kind::Usage, m)
    }
    pub fn config(m: impl Into<String>) -> Self {
        Self::new(Kind::Config, m)
    }
    pub fn network(m: impl Into<String>) -> Self {
        Self::new(Kind::Network, m)
    }
    pub fn odoo(m: impl Into<String>, detail: Value) -> Self {
        Error { kind: Kind::Odoo, message: m.into(), detail }
    }
    pub fn with_detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }

    pub fn exit_code(&self) -> i32 {
        match self.kind {
            Kind::Odoo => 1,
            Kind::Usage => 2,
            Kind::Config => 3,
            Kind::Network => 4,
        }
    }

    pub fn to_json(&self) -> Value {
        let kind = match self.kind {
            Kind::Odoo => "odoo",
            Kind::Usage => "usage",
            Kind::Config => "config",
            Kind::Network => "network",
        };
        let mut e = json!({ "kind": kind, "message": self.message });
        if let Value::Object(d) = &self.detail {
            for (k, v) in d {
                e[k] = v.clone();
            }
        }
        json!({ "error": e })
    }
}
