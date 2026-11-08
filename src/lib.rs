// SPDX-License-Identifier: Apache-2.0
//! SigmaOS-hwdb — SigmaOS hwdb component inspired by Linux Mint / Omarchy / Arch Linux.
//!
//! udev/sysctl/sysusers/tmpfiles-style rule file: match+action tokens, fail-fast validation, deterministic render.
//! Principles: single responsibility, fail-fast, defensive programming,
//! component isolation, stable API, deterministic tests.
#![forbid(unsafe_code)]

use std::fmt;
/// Errors for `RuleError` operations. Fail-fast: invalid input is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    /// Rule token (key/operator/value) must not be empty
    EmptyToken,
    /// Operator is not in the allowed set
    BadOperator(String),
    /// Rule must contain at least one match or action
    EmptyRule,
    /// Rule value must not be empty
    EmptyValue(String),
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyToken => write!(f, "rule token (key/operator/value) must not be empty"),
            Self::BadOperator(x) => write!(f, "operator is not in the allowed set: {x}"),
            Self::EmptyRule => write!(f, "rule must contain at least one match or action"),
            Self::EmptyValue(x) => write!(f, "rule value must not be empty: {x}"),
        }
    }
}

impl std::error::Error for RuleError {}

/// Allowed match/action operators (udev `==`, sysctl `=`, tmpfiles `d`-style args).
pub const OPERATORS: &[&str] = &["==", "!=", ">=", "<=", ">", "<", "=", "+=", ":=", "!*="];

/// A single `KEY OP VALUE` token inside a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Left-hand key, e.g. `SUBSYSTEM`, `kernel.printk`.
    pub key: String,
    /// Operator, e.g. `==`, `=`.
    pub op: String,
    /// Right-hand value.
    pub value: String,
}

/// One rule line: comma-separated match tokens then action tokens.
#[derive(Debug, Default)]
pub struct Rule {
    /// Conditions that must hold (udev-style `,`-separated matches).
    pub matches: Vec<Token>,
    /// Actions applied when matches hold.
    pub actions: Vec<Token>,
}

/// An ordered rule file (e.g. `/etc/udev/rules.d/99-sigmaos.rules`).
#[derive(Debug, Default)]
pub struct RuleFile {
    rules: Vec<Rule>,
    /// Whether the last rule is still accepting tokens.
    open: bool,
}

impl RuleFile {
    /// Empty rule file.
    pub fn new() -> Self { Self::default() }

    /// Ensure a rule is open to receive tokens.
    fn ensure_open(&mut self) {
        if !self.open {
            self.rules.push(Rule::default());
            self.open = true;
        }
    }

    /// Validate a token triple.
    fn check(key: &str, op: &str, value: &str) -> Result<(), RuleError> {
        if key.trim().is_empty() || op.trim().is_empty() {
            return Err(RuleError::EmptyToken);
        }
        if !OPERATORS.contains(&op) {
            return Err(RuleError::BadOperator(op.to_string()));
        }
        if value.trim().is_empty() {
            return Err(RuleError::EmptyValue(key.to_string()));
        }
        Ok(())
    }

    /// Add a match token to the last rule (or start one).
    pub fn on_match(&mut self, key: &str, op: &str, value: impl Into<String>)
        -> Result<(), RuleError>
    {
        let v = value.into();
        Self::check(key, op, &v)?;
        self.ensure_open();
        self.rules.last_mut().unwrap().matches.push(Token {
            key: key.to_string(), op: op.to_string(), value: v,
        });
        Ok(())
    }

    /// Add an action token to the last rule.
    pub fn do_action(&mut self, key: &str, op: &str, value: impl Into<String>)
        -> Result<(), RuleError>
    {
        let v = value.into();
        Self::check(key, op, &v)?;
        self.ensure_open();
        self.rules.last_mut().unwrap().actions.push(Token {
            key: key.to_string(), op: op.to_string(), value: v,
        });
        Ok(())
    }

    /// Finish the current rule; fails if it has no tokens.
    pub fn end_rule(&mut self) -> Result<(), RuleError> {
        if !self.open { return Err(RuleError::EmptyRule); }
        self.open = false;
        match self.rules.last() {
            Some(r) if r.matches.is_empty() && r.actions.is_empty() => {
                self.rules.pop();
                Err(RuleError::EmptyRule)
            }
            Some(_) => Ok(()),
            None => Err(RuleError::EmptyRule),
        }
    }

    /// Number of rules.
    pub fn len(&self) -> usize { self.rules.len() }

    /// Whether empty.
    pub fn is_empty(&self) -> bool { self.rules.is_empty() }

    /// Render deterministic rule text, one rule per line.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for r in &self.rules {
            let mut parts: Vec<String> = Vec::new();
            for t in &r.matches {
                parts.push(format!("{} {} {}", t.key, t.op, t.value));
            }
            for t in &r.actions {
                parts.push(format!("{} {} {}", t.key, t.op, t.value));
            }
            out.push_str(&parts.join(", "));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udev_style_rule() {
        let mut f = RuleFile::new();
        f.on_match("SUBSYSTEM", "==", "usb").unwrap();
        f.on_match("ATTR{idVendor}", "==", "0781").unwrap();
        f.do_action("MODE", "=", "0660").unwrap();
        f.do_action("GROUP", "=", "disk").unwrap();
        f.end_rule().unwrap();
        let text = f.render();
        assert!(text.contains("SUBSYSTEM == usb"));
        assert!(text.contains("MODE = 0660"));
        assert!(text.contains("GROUP = disk"));
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn bad_operator_fails() {
        let mut f = RuleFile::new();
        assert!(matches!(f.on_match("K", "~~", "v"), Err(RuleError::BadOperator(_))));
    }

    #[test]
    fn empty_value_fails() {
        let mut f = RuleFile::new();
        assert!(matches!(f.do_action("K", "=", " "), Err(RuleError::EmptyValue(_))));
    }

    #[test]
    fn empty_rule_fails() {
        let mut f = RuleFile::new();
        assert!(matches!(f.end_rule(), Err(RuleError::EmptyRule)));
    }

    #[test]
    fn multiple_rules_render_lines() {
        let mut f = RuleFile::new();
        f.on_match("A", "==", "1").unwrap();
        f.end_rule().unwrap();
        f.do_action("B", "=", "2").unwrap();
        f.end_rule().unwrap();
        assert_eq!(f.render().lines().count(), 2);
    }

}

/// Backwards-compatible component handle (kept for existing callers/tests).
#[derive(Debug, Default)]
pub struct Component {
    name: &'static str,
}

impl Component {
    /// Create the component handle.
    pub fn new() -> Self {
        Self { name: "SigmaOS Component" }
    }
}
