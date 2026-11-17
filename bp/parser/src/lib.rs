//! Parser for BetterQuery. Parsing validates syntax, JSON Pointers, and RFC 9535
//! queries; document-dependent patch constraints belong to execution.
//!
//! Objects retain source order, numbers retain their exact JSON spelling, and
//! duplicate decoded keys and invalid Unicode escapes are rejected in both modes.

use std::{collections::HashSet, fmt};

pub use bp_common::{Form, FormKind, Patch, Program, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// Zero-based UTF-8 byte offset.
    pub offset: usize,
    /// One-based line and Unicode-character column.
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}
impl std::error::Error for Error {}

/// Parse an entire program. Empty programs are valid.
///
/// Nesting is limited to 128 forms/values to bound parser stack usage.
pub fn parse(source: &str) -> Result<Program, Error> {
    let mut parser = Parser { source, pos: 0 };
    let mut program = Vec::new();
    parser.ws();
    while parser.peek().is_some() {
        program.push(parser.form(0)?);
        parser.ws();
    }
    Ok(program)
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
}

type Result<T, E = Error> = std::result::Result<T, E>;

impl Parser<'_> {
    fn error(&self, offset: usize, message: impl Into<String>) -> Error {
        let prefix = &self.source[..offset];
        Error {
            offset,
            line: prefix.bytes().filter(|&b| b == b'\n').count() + 1,
            column: prefix.rsplit('\n').next().unwrap().chars().count() + 1,
            message: message.into(),
        }
    }

    fn fail<T>(&self, message: impl Into<String>) -> Result<T> {
        Err(self.error(self.pos, message))
    }

    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, byte: u8) -> Result<()> {
        self.ws();
        if self.eat(byte) {
            Ok(())
        } else {
            self.fail(format!("expected '{}'", byte as char))
        }
    }

    fn depth(&self, depth: usize) -> Result<()> {
        if depth >= 128 {
            self.fail("nesting limit (128) exceeded")
        } else {
            Ok(())
        }
    }

    fn word(&mut self) -> Result<&str> {
        self.ws();
        let start = self.pos;
        if !matches!(self.peek(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')) {
            return self.fail("expected a word");
        }
        self.pos += 1;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'0'..=b'9' | b'-')
        ) {
            self.pos += 1;
        }
        Ok(&self.source[start..self.pos])
    }

    fn string(&mut self) -> Result<String> {
        self.ws();
        let start = self.pos;
        if !self.eat(b'"') {
            return self.fail("expected a JSON string");
        }
        // Only find the closing quote here; serde_json validates escapes,
        // control characters, and surrogate pairing without normalizing text.
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    self.pos += 1;
                    return serde_json::from_str(&self.source[start..self.pos])
                        .map_err(|e| self.error(start, format!("invalid string: {e}")));
                }
                b'\\' => {
                    self.pos += 1;
                    if let Some(ch) = self.source[self.pos..].chars().next() {
                        self.pos += ch.len_utf8();
                    }
                }
                _ => self.pos += self.source[self.pos..].chars().next().unwrap().len_utf8(),
            }
        }
        self.fail("unterminated string")
    }

    fn number(&mut self, json: bool) -> Result<Value> {
        let start = self.pos;
        self.eat(b'-');
        if !self.eat(b'0') {
            if !matches!(self.peek(), Some(b'1'..=b'9')) {
                return self.fail("expected a digit");
            }
            self.digits();
        }
        if self.eat(b'.') && self.digits() == 0 {
            return self.fail("expected digits after decimal point");
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if self.digits() == 0 {
                return self.fail("expected exponent digits");
            }
        }
        if matches!(self.peek(), Some(b'0'..=b'9'))
            || (!json
                && matches!(
                    self.peek(),
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'-' | b'+')
                ))
        {
            return self.fail("invalid number or missing whitespace after number");
        }
        Ok(Value::Number(self.source[start..self.pos].to_owned()))
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - start
    }

    fn form(&mut self, depth: usize) -> Result<Form> {
        self.depth(depth)?;
        self.ws();
        let start = self.pos;
        self.expect(b'(')?;
        let name = self.word()?.to_ascii_lowercase();
        let kind = match name.as_str() {
            "merge" => FormKind::Merge(self.value(depth + 1, false)?),
            "patch" => FormKind::Patch(self.patch(depth + 1)?),
            "query" => {
                self.ws();
                let query_start = self.pos;
                let query = self.string()?;
                serde_json_path::JsonPath::parse(&query)
                    .map_err(|e| self.error(query_start, format!("invalid JSONPath: {e}")))?;
                let body = Box::new(self.form(depth + 1)?);
                FormKind::Query { query, body }
            }
            _ => return self.fail(format!("unknown form '{name}'")),
        };
        self.expect(b')')?;
        Ok(Form {
            span: start..self.pos,
            kind,
        })
    }

    fn pointer(&mut self) -> Result<String> {
        self.ws();
        let start = self.pos;
        let pointer = self.string()?;
        if !pointer.is_empty() && !pointer.starts_with('/') {
            return Err(self.error(start, "JSON Pointer must be empty or start with '/'"));
        }
        let mut chars = pointer.chars();
        while let Some(ch) = chars.next() {
            if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
                return Err(self.error(start, "JSON Pointer escape must be ~0 or ~1"));
            }
        }
        Ok(pointer)
    }

    fn patch(&mut self, depth: usize) -> Result<Patch> {
        let operation = self.word()?.to_ascii_lowercase();
        if !matches!(
            operation.as_str(),
            "add" | "remove" | "replace" | "move" | "copy" | "test"
        ) {
            return self.fail(format!("unknown patch operation '{operation}'"));
        }
        let (mut path, mut from, mut value) = (None, None, None);
        self.ws();
        while self.peek() != Some(b')') {
            self.expect(b'(')?;
            let field = self.word()?.to_ascii_lowercase();
            match field.as_str() {
                "path" if path.is_none() => path = Some(self.pointer()?),
                "from" if matches!(operation.as_str(), "move" | "copy") && from.is_none() => {
                    from = Some(self.pointer()?);
                }
                "value"
                    if matches!(operation.as_str(), "add" | "replace" | "test")
                        && value.is_none() =>
                {
                    value = Some(self.value(depth, false)?);
                }
                _ => return self.fail(format!("unexpected or duplicate patch field '{field}'")),
            }
            self.expect(b')')?;
            self.ws();
        }
        let path = path.ok_or_else(|| self.error(self.pos, "missing PATH field"))?;
        match operation.as_str() {
            "remove" => Ok(Patch::Remove { path }),
            "move" | "copy" => {
                let from = from.ok_or_else(|| self.error(self.pos, "missing FROM field"))?;
                Ok(if operation == "move" {
                    Patch::Move { path, from }
                } else {
                    Patch::Copy { path, from }
                })
            }
            _ => {
                let value = value.ok_or_else(|| self.error(self.pos, "missing VALUE field"))?;
                Ok(match operation.as_str() {
                    "add" => Patch::Add { path, value },
                    "replace" => Patch::Replace { path, value },
                    _ => Patch::Test { path, value },
                })
            }
        }
    }

    fn value(&mut self, depth: usize, json: bool) -> Result<Value> {
        self.depth(depth)?;
        self.ws();
        match self.peek() {
            Some(b'"') => self.string().map(Value::String),
            Some(b'-' | b'0'..=b'9') => self.number(json),
            Some(b'[') if json => self.json_array(depth),
            Some(b'{') if json => self.object(depth, true),
            Some(b'(') if !json => {
                self.pos += 1;
                let name = self.word()?.to_ascii_lowercase();
                let value = match name.as_str() {
                    "array" => {
                        let mut values = Vec::new();
                        self.ws();
                        while self.peek() != Some(b')') {
                            values.push(self.value(depth + 1, false)?);
                            self.ws();
                        }
                        Value::Array(values)
                    }
                    "object" => self.object(depth, false)?,
                    "json" => {
                        let value = self.value(depth + 1, true)?;
                        let end = self.word()?;
                        if !end.eq_ignore_ascii_case("END") {
                            return self.fail("expected END after one JSON value");
                        }
                        value
                    }
                    _ => return self.fail(format!("unknown value constructor '{name}'")),
                };
                self.expect(b')')?;
                Ok(value)
            }
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => {
                let start = self.pos;
                let word = self.word()?;
                if json && !matches!(word, "true" | "false" | "null") {
                    return Err(self.error(
                        start,
                        "JSON literals must be lowercase true, false, or null",
                    ));
                }
                match word.to_ascii_lowercase().as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    _ => self.fail("expected a value"),
                }
            }
            _ => self.fail("expected a value"),
        }
    }

    fn object(&mut self, depth: usize, json: bool) -> Result<Value> {
        let mut members = Vec::new();
        let mut keys = HashSet::new();
        if json {
            self.expect(b'{')?;
        }
        self.ws();
        let close = if json { b'}' } else { b')' };
        while self.peek() != Some(close) {
            if !json {
                self.expect(b'(')?;
            }
            self.ws();
            let start = self.pos;
            let key = if json || self.peek() == Some(b'"') {
                self.string()?
            } else {
                self.word()?.to_owned()
            };
            if !keys.insert(key.clone()) {
                return Err(self.error(start, format!("duplicate object key {key:?}")));
            }
            self.expect(if json { b':' } else { b'.' })?;
            let value = self.value(depth + 1, json)?;
            members.push((key, value));
            if !json {
                self.expect(b')')?;
            }
            self.ws();
            if json && self.peek() != Some(close) {
                self.expect(b',')?;
                self.ws();
                if self.peek() == Some(close) {
                    return self.fail("trailing comma in JSON object");
                }
            }
        }
        if json {
            self.pos += 1;
        }
        Ok(Value::Object(members))
    }

    fn json_array(&mut self, depth: usize) -> Result<Value> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        self.ws();
        if !self.eat(b']') {
            loop {
                values.push(self.value(depth + 1, true)?);
                self.ws();
                if self.eat(b']') {
                    break;
                }
                self.expect(b',')?;
            }
        }
        Ok(Value::Array(values))
    }
}
