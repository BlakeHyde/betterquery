//! Ordered JSON intermediate representation with lossless number spelling.
//!
//! Parsing rejects duplicate decoded keys, invalid Unicode, and non-JSON syntax.
//! Serialization preserves object order and exact number text, but not source
//! whitespace or string escape spelling. No Unicode normalization is performed.
//!
//! ```
//! use bp_json::{parse, to_string};
//!
//! let source = r#"{"z":1.2300E+999999999999999999999,"a":[true,"𝄞"]}"#;
//! let value = parse(source)?;
//! assert_eq!(to_string(&value)?, source);
//! assert_eq!(parse(&to_string(&value)?)?, value);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::{collections::HashSet, fmt};

/// JSON data in document order. Equality compares structure and exact number text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    /// Exact JSON number spelling, without range restrictions or rounding.
    Number(String),
    String(String),
    Array(Vec<Value>),
    /// Ordered members. Parsing and serialization reject duplicate decoded keys.
    Object(Vec<(String, Value)>),
}

/// A JSON syntax error, with UTF-8 byte offset and one-based character location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub offset: usize,
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

/// Parse exactly one JSON value, surrounded only by JSON whitespace.
/// Nesting is limited to 128 value levels to bound stack usage.
pub fn parse(source: &str) -> Result<Value, Error> {
    let mut parser = Parser { source, pos: 0 };
    let value = parser.value(0)?;
    parser.ws();
    if parser.pos != source.len() {
        return parser.fail("unexpected content after JSON value");
    }
    Ok(value)
}

/// Serialization failure for an invalid programmatically constructed value.
/// `path` contains decoded object keys or decimal array indices from the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializeError {
    pub path: Vec<String>,
    pub message: String,
}
impl fmt::Display for SerializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at {:?}: {}", self.path, self.message)
    }
}
impl std::error::Error for SerializeError {}

/// Serialize compact JSON, preserving member order and number spelling.
/// Invalid numbers, duplicate keys, and excessive nesting are errors. Every
/// successful result can be parsed back into an identical `Value`.
pub fn to_string(value: &Value) -> Result<String, SerializeError> {
    fn emit(value: &Value, out: &mut String, path: &mut Vec<String>) -> Result<(), SerializeError> {
        let error = |message: &str| SerializeError {
            path: path.clone(),
            message: message.into(),
        };
        if path.len() >= 128 {
            return Err(error("nesting limit (128) exceeded"));
        }
        match value {
            Value::Null => out.push_str("null"),
            Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
            Value::Number(text) => {
                if !matches!(parse(text), Ok(Value::Number(ref n)) if n == text) {
                    return Err(error("invalid JSON number"));
                }
                out.push_str(text);
            }
            Value::String(text) => out.push_str(
                &serde_json::to_string(text).expect("string serialization is infallible"),
            ),
            Value::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    path.push(index.to_string());
                    emit(value, out, path)?;
                    path.pop();
                }
                out.push(']');
            }
            Value::Object(members) => {
                let mut keys = HashSet::new();
                out.push('{');
                for (index, (key, value)) in members.iter().enumerate() {
                    path.push(key.clone());
                    if !keys.insert(key) {
                        return Err(SerializeError {
                            path: path.clone(),
                            message: "duplicate object key".into(),
                        });
                    }
                    if index != 0 {
                        out.push(',');
                    }
                    out.push_str(
                        &serde_json::to_string(key).expect("string serialization is infallible"),
                    );
                    out.push(':');
                    emit(value, out, path)?;
                    path.pop();
                }
                out.push('}');
            }
        }
        Ok(())
    }
    let mut output = String::new();
    emit(value, &mut output, &mut Vec::new())?;
    Ok(output)
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
}
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
    fn fail<T>(&self, message: impl Into<String>) -> Result<T, Error> {
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
    fn expect(&mut self, byte: u8) -> Result<(), Error> {
        self.ws();
        if self.eat(byte) {
            Ok(())
        } else {
            self.fail(format!("expected '{}'", byte as char))
        }
    }
    fn string(&mut self) -> Result<String, Error> {
        self.ws();
        let start = self.pos;
        if !self.eat(b'"') {
            return self.fail("expected a JSON string");
        }
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
    fn digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - start
    }
    fn number(&mut self) -> Result<Value, Error> {
        let start = self.pos;
        self.eat(b'-');
        if !self.eat(b'0') {
            if !matches!(self.peek(), Some(b'1'..=b'9')) {
                return self.fail("expected a digit");
            }
            self.digits();
        }
        if self.eat(b'.') && self.digits() == 0 {
            return self.fail("expected fraction digits");
        }
        if self.eat(b'e') || self.eat(b'E') {
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if self.digits() == 0 {
                return self.fail("expected exponent digits");
            }
        }
        Ok(Value::Number(self.source[start..self.pos].into()))
    }
    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        self.ws();
        if depth >= 128 {
            return self.fail("nesting limit (128) exceeded");
        }
        match self.peek() {
            Some(b'"') => self.string().map(Value::String),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b'n' | b't' | b'f') => {
                let (word, value) = match self.peek().unwrap() {
                    b'n' => ("null", Value::Null),
                    b't' => ("true", Value::Bool(true)),
                    _ => ("false", Value::Bool(false)),
                };
                if !self.source[self.pos..].starts_with(word) {
                    return self.fail("invalid JSON literal");
                }
                self.pos += word.len();
                Ok(value)
            }
            Some(b'[') => {
                self.pos += 1;
                let mut values = Vec::new();
                self.ws();
                if !self.eat(b']') {
                    loop {
                        values.push(self.value(depth + 1)?);
                        self.ws();
                        if self.eat(b']') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                Ok(Value::Array(values))
            }
            Some(b'{') => {
                self.pos += 1;
                let mut members = Vec::new();
                let mut keys = HashSet::new();
                self.ws();
                if !self.eat(b'}') {
                    loop {
                        self.ws();
                        let start = self.pos;
                        let key = self.string()?;
                        if !keys.insert(key.clone()) {
                            return Err(self.error(start, format!("duplicate object key {key:?}")));
                        }
                        self.expect(b':')?;
                        members.push((key, self.value(depth + 1)?));
                        self.ws();
                        if self.eat(b'}') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                Ok(Value::Object(members))
            }
            _ => self.fail("expected a JSON value"),
        }
    }
}
