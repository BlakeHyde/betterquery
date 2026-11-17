//! Shared ordered values, programs, and document identities.

use std::ops::Range;

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

/// A complete sequence of transformations, in execution order.
pub type Program = Vec<Form>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    /// Byte range in the original UTF-8 source, including parentheses.
    pub span: Range<usize>,
    pub kind: FormKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    Patch(Patch),
    Merge(Value),
    Query { query: String, body: Box<Form> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Patch {
    Add { path: String, value: Value },
    Remove { path: String },
    Replace { path: String, value: Value },
    Move { path: String, from: String },
    Copy { path: String, from: String },
    Test { path: String, value: Value },
}

/// A path consists of decoded JSON Pointer tokens.
pub type Path = Vec<String>;
pub type NodeId = u64;

/// A document node has an identity independent of array indices or object keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
}

impl Node {
    pub fn value(&self) -> Value {
        match &self.kind {
            NodeKind::Null => Value::Null,
            NodeKind::Bool(v) => Value::Bool(*v),
            NodeKind::Number(v) => Value::Number(v.clone()),
            NodeKind::String(v) => Value::String(v.clone()),
            NodeKind::Array(v) => Value::Array(v.iter().map(Node::value).collect()),
            NodeKind::Object(v) => {
                Value::Object(v.iter().map(|(k, v)| (k.clone(), v.value())).collect())
            }
        }
    }

    pub fn at(&self, path: &[String]) -> Option<&Node> {
        let Some((key, rest)) = path.split_first() else {
            return Some(self);
        };
        match &self.kind {
            NodeKind::Array(v) => v.get(array_index(key)?),
            NodeKind::Object(v) => v.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }?
        .at(rest)
    }

    pub fn at_mut(&mut self, path: &[String]) -> Option<&mut Node> {
        let Some((key, rest)) = path.split_first() else {
            return Some(self);
        };
        match &mut self.kind {
            NodeKind::Array(v) => v.get_mut(array_index(key)?),
            NodeKind::Object(v) => v.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }?
        .at_mut(rest)
    }

    /// Parent before children, in document member order.
    pub fn walk(&self, path: &mut Path, visit: &mut impl FnMut(&Path, &Node)) {
        visit(path, self);
        match &self.kind {
            NodeKind::Array(values) => {
                for (i, node) in values.iter().enumerate() {
                    path.push(i.to_string());
                    node.walk(path, visit);
                    path.pop();
                }
            }
            NodeKind::Object(values) => {
                for (key, node) in values {
                    path.push(key.clone());
                    node.walk(path, visit);
                    path.pop();
                }
            }
            _ => {}
        }
    }
}

/// `None` distinguishes a removed root from the JSON value null.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub root: Option<Node>,
    next_id: NodeId,
}

impl Document {
    pub fn new(value: Value) -> Self {
        let mut document = Self {
            root: None,
            next_id: 0,
        };
        document.root = Some(document.allocate(&value));
        document
    }

    /// Allocate fresh identities, including for copies and replacements.
    pub fn allocate(&mut self, value: &Value) -> Node {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("document identity space exhausted");
        let kind = match value {
            Value::Null => NodeKind::Null,
            Value::Bool(v) => NodeKind::Bool(*v),
            Value::Number(v) => NodeKind::Number(v.clone()),
            Value::String(v) => NodeKind::String(v.clone()),
            Value::Array(v) => NodeKind::Array(v.iter().map(|v| self.allocate(v)).collect()),
            Value::Object(v) => NodeKind::Object(
                v.iter()
                    .map(|(k, v)| (k.clone(), self.allocate(v)))
                    .collect(),
            ),
        };
        Node { id, kind }
    }

    pub fn value(&self) -> Option<Value> {
        self.root.as_ref().map(Node::value)
    }
    pub fn at(&self, path: &[String]) -> Option<&Node> {
        self.root.as_ref()?.at(path)
    }
    pub fn at_mut(&mut self, path: &[String]) -> Option<&mut Node> {
        self.root.as_mut()?.at_mut(path)
    }

    pub fn location(&self, id: NodeId) -> Option<Path> {
        let mut found = None;
        if let Some(root) = &self.root {
            root.walk(&mut Vec::new(), &mut |path, node| {
                if node.id == id {
                    found = Some(path.clone());
                }
            });
        }
        found
    }

    pub fn locations(&self) -> Vec<(NodeId, Path)> {
        let mut locations = Vec::new();
        if let Some(root) = &self.root {
            root.walk(&mut Vec::new(), &mut |path, node| {
                locations.push((node.id, path.clone()))
            });
        }
        locations
    }
}

pub fn array_index(token: &str) -> Option<usize> {
    if token.is_empty()
        || (token.len() > 1 && token.starts_with('0'))
        || !token.bytes().all(|b| b.is_ascii_digit())
    {
        None
    } else {
        token.parse().ok()
    }
}

pub fn pointer(path: &[String]) -> String {
    path.iter()
        .map(|p| format!("/{}", p.replace('~', "~0").replace('/', "~1")))
        .collect()
}

pub fn parse_pointer(text: &str) -> Result<Path, ExecutionError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    if !text.starts_with('/') {
        return Err(ExecutionError::new(
            "JSON Pointer must start with '/'",
            Vec::new(),
        ));
    }
    text[1..]
        .split('/')
        .map(|token| {
            let mut decoded = String::new();
            let mut chars = token.chars();
            while let Some(ch) = chars.next() {
                decoded.push(if ch == '~' {
                    match chars.next() {
                        Some('0') => '~',
                        Some('1') => '/',
                        _ => {
                            return Err(ExecutionError::new(
                                "invalid JSON Pointer escape",
                                Vec::new(),
                            ));
                        }
                    }
                } else {
                    ch
                });
            }
            Ok(decoded)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionError {
    pub message: String,
    pub path: Path,
    pub span: Option<Range<usize>>,
    /// Source span of the transform that invalidated a frozen target.
    pub invalidated_by: Option<Range<usize>>,
}
impl ExecutionError {
    pub fn new(message: impl Into<String>, path: Path) -> Self {
        Self {
            message: message.into(),
            path,
            span: None,
            invalidated_by: None,
        }
    }
}
impl std::fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {:?}", self.message, pointer(&self.path))?;
        if let Some(span) = &self.span {
            write!(f, " (form at bytes {}..{})", span.start, span.end)?;
        }
        if let Some(span) = &self.invalidated_by {
            write!(
                f,
                "; target last valid before form at bytes {}..{}",
                span.start, span.end
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for ExecutionError {}

/// JSON equality ignores object order and compares numbers mathematically.
pub fn equivalent(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => decimal(a) == decimal(b),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(k, v)| {
                    b.iter()
                        .find(|(key, _)| key == k)
                        .is_some_and(|(_, other)| equivalent(v, other))
                })
        }
        _ => left == right,
    }
}

fn decimal(text: &str) -> (bool, String, num_bigint::BigInt) {
    use num_bigint::BigInt;
    let negative = text.starts_with('-');
    let text = text.trim_start_matches('-');
    let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
    let mut exponent: BigInt = exponent.parse().expect("valid JSON number exponent");
    let fraction = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    exponent -= BigInt::from(fraction);
    let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    digits = digits.trim_start_matches('0').to_owned();
    if digits.is_empty() {
        return (false, String::from("0"), BigInt::from(0));
    }
    let zeros = digits.len() - digits.trim_end_matches('0').len();
    digits.truncate(digits.len() - zeros);
    exponent += BigInt::from(zeros);
    (negative, digits, exponent)
}

/// Compare valid JSON numbers exactly, without expanding their exponents.
pub fn compare_numbers(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (an, a, ae) = decimal(left);
    let (bn, b, be) = decimal(right);
    if a == "0" && b == "0" {
        return Ordering::Equal;
    }
    if a == "0" {
        return if bn {
            Ordering::Greater
        } else {
            Ordering::Less
        };
    }
    if b == "0" {
        return if an {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    if an != bn {
        return if an {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let order = (ae + num_bigint::BigInt::from(a.len()))
        .cmp(&(be + num_bigint::BigInt::from(b.len())))
        .then_with(|| {
            let length = a.len().max(b.len());
            a.bytes()
                .chain(std::iter::repeat(b'0'))
                .take(length)
                .cmp(b.bytes().chain(std::iter::repeat(b'0')).take(length))
        });
    if an { order.reverse() } else { order }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_numeric_equality() {
        for (a, b) in [
            ("1", "1.00"),
            ("-0e9999", "0"),
            ("12300e-2", "123"),
            ("1e99999999999999999999999", "10e99999999999999999999998"),
        ] {
            assert!(equivalent(
                &Value::Number(a.into()),
                &Value::Number(b.into())
            ));
        }
        assert!(!equivalent(
            &Value::Number("9007199254740992".into()),
            &Value::Number("9007199254740993".into())
        ));
    }
    #[test]
    fn pointers_and_indices() {
        assert_eq!(parse_pointer("/a~1b/~0/").unwrap(), ["a/b", "~", ""]);
        assert!(parse_pointer("/~2").is_err());
        assert_eq!(array_index("01"), None);
        assert_eq!(array_index("-1"), None);
    }
}
