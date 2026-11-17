use bp_common::Value;

#[derive(Debug)]
pub(super) struct Query {
    pub relative: bool,
    pub segments: Vec<Segment>,
}
#[derive(Debug)]
pub(super) struct Segment {
    pub descendant: bool,
    pub selectors: Vec<Selector>,
}
#[derive(Debug)]
pub(super) enum Selector {
    Name(String),
    Wildcard,
    Index(i64),
    Slice(Option<i64>, Option<i64>, i64),
    Filter(Expr),
}
#[derive(Debug)]
pub(super) enum Expr {
    Literal(Value),
    Query(Query),
    Function(String, Vec<Expr>),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Compare(Box<Expr>, String, Box<Expr>),
}

type Result<T> = std::result::Result<T, String>;

pub(super) fn parse(source: &str) -> Result<Query> {
    budget(source)?;
    serde_json_path::JsonPath::parse(source).map_err(|e| format!("invalid JSONPath: {e}"))?;
    let mut parser = Parser {
        source,
        pos: 0,
        depth: 0,
    };
    let query = parser.query()?;
    parser.ws();
    if parser.pos != source.len() {
        return Err("unexpected JSONPath suffix".into());
    }
    Ok(query)
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
    depth: usize,
}
impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }
    fn ws(&mut self) {
        while self
            .peek()
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
        {
            self.pos += 1;
        }
    }
    fn eat(&mut self, token: &str) -> bool {
        self.ws();
        if self.source[self.pos..].starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, token: &str) -> Result<()> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(format!("expected {token:?} at JSONPath byte {}", self.pos))
        }
    }
    fn query(&mut self) -> Result<Query> {
        self.ws();
        let relative = self.eat("@");
        if !relative {
            self.expect("$")?;
        }
        let mut segments = Vec::new();
        loop {
            self.ws();
            let descendant = self.eat("..");
            let dot = !descendant && self.eat(".");
            if !descendant && !dot && self.peek() != Some('[') {
                break;
            }
            let selectors = if self.eat("[") {
                let mut selectors = Vec::new();
                loop {
                    selectors.push(self.selector()?);
                    if self.eat("]") {
                        break;
                    }
                    self.expect(",")?;
                }
                selectors
            } else if self.eat("*") {
                vec![Selector::Wildcard]
            } else {
                vec![Selector::Name(self.name()?)]
            };
            segments.push(Segment {
                descendant,
                selectors,
            });
        }
        Ok(Query { relative, segments })
    }
    fn name(&mut self) -> Result<String> {
        self.ws();
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c as u32 >= 0x80)
        {
            self.pos += self.peek().unwrap().len_utf8();
        }
        if start == self.pos {
            Err(format!("expected name at JSONPath byte {}", self.pos))
        } else {
            Ok(self.source[start..self.pos].into())
        }
    }
    fn string(&mut self) -> Result<String> {
        self.ws();
        let quote = self.peek().ok_or("expected string")?;
        self.pos += 1;
        let mut json = String::from("\"");
        loop {
            let ch = self.peek().ok_or("unterminated JSONPath string")?;
            self.pos += ch.len_utf8();
            if ch == quote {
                break;
            }
            if ch == '\\' {
                let escaped = self.peek().ok_or("unterminated escape")?;
                self.pos += escaped.len_utf8();
                if escaped == '\'' && quote == '\'' {
                    json.push('\'');
                } else {
                    json.push('\\');
                    json.push(escaped);
                }
            } else if ch == '"' {
                json.push_str("\\\"");
            } else {
                json.push(ch);
            }
        }
        json.push('"');
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
    fn integer(&mut self) -> Result<Option<i64>> {
        self.ws();
        if !self.peek().is_some_and(|c| c == '-' || c.is_ascii_digit()) {
            return Ok(None);
        }
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.source[start..self.pos]
            .parse()
            .map(Some)
            .map_err(|_| "invalid array index".into())
    }
    fn selector(&mut self) -> Result<Selector> {
        self.ws();
        if self.eat("*") {
            return Ok(Selector::Wildcard);
        }
        if matches!(self.peek(), Some('\'' | '"')) {
            return self.string().map(Selector::Name);
        }
        if self.eat("?") {
            return self.expr().map(Selector::Filter);
        }
        let start = self.integer()?;
        if !self.eat(":") {
            return start.map(Selector::Index).ok_or("expected selector".into());
        }
        let end = self.integer()?;
        let step = if self.eat(":") {
            self.integer()?.unwrap_or(1)
        } else {
            1
        };
        Ok(Selector::Slice(start, end, step))
    }
    fn expr(&mut self) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 128 {
            return Err("JSONPath expression nesting exceeds 128".into());
        }
        let mut expr = self.and()?;
        while self.eat("||") {
            expr = Expr::Or(Box::new(expr), Box::new(self.and()?));
        }
        self.depth -= 1;
        Ok(expr)
    }
    fn and(&mut self) -> Result<Expr> {
        let mut expr = self.basic()?;
        while self.eat("&&") {
            expr = Expr::And(Box::new(expr), Box::new(self.basic()?));
        }
        Ok(expr)
    }
    fn basic(&mut self) -> Result<Expr> {
        if self.eat("!") {
            return Ok(Expr::Not(Box::new(self.basic()?)));
        }
        if self.eat("(") {
            let expr = self.expr()?;
            self.expect(")")?;
            return Ok(expr);
        }
        let left = self.term()?;
        for op in ["==", "!=", "<=", ">=", "<", ">"] {
            if self.eat(op) {
                return Ok(Expr::Compare(
                    Box::new(left),
                    op.into(),
                    Box::new(self.term()?),
                ));
            }
        }
        Ok(left)
    }
    fn term(&mut self) -> Result<Expr> {
        self.ws();
        match self.peek() {
            Some('$' | '@') => self.query().map(Expr::Query),
            Some('\'' | '"') => self.string().map(|s| Expr::Literal(Value::String(s))),
            Some('-' | '0'..='9') => {
                let start = self.pos;
                while self
                    .peek()
                    .is_some_and(|c| matches!(c, '0'..='9' | '-' | '+' | '.' | 'e' | 'E'))
                {
                    self.pos += 1;
                }
                bp_document_parser::parse(&self.source[start..self.pos])
                    .map(Expr::Literal)
                    .map_err(|e| e.to_string())
            }
            _ => {
                let name = self.name()?;
                match name.as_str() {
                    "true" => Ok(Expr::Literal(Value::Bool(true))),
                    "false" => Ok(Expr::Literal(Value::Bool(false))),
                    "null" => Ok(Expr::Literal(Value::Null)),
                    _ => {
                        self.expect("(")?;
                        let mut args = Vec::new();
                        if !self.eat(")") {
                            loop {
                                args.push(self.expr()?);
                                if self.eat(")") {
                                    break;
                                }
                                self.expect(",")?;
                            }
                        }
                        Ok(Expr::Function(name, args))
                    }
                }
            }
        }
    }
}

// Bound recursion before invoking the syntax/type validator.
fn budget(source: &str) -> Result<()> {
    if source.len() > 65536 {
        return Err("JSONPath exceeds 64 KiB".into());
    }
    let (mut depth, mut operators) = (0usize, 0usize);
    let mut quote = None;
    let mut escaped = false;
    for ch in source.chars() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
        } else {
            match ch {
                '\'' | '"' => quote = Some(ch),
                '[' | '(' => depth += 1,
                ']' | ')' => depth = depth.saturating_sub(1),
                '!' | '&' | '|' => operators += 1,
                _ => {}
            }
            if depth > 128 || operators > 512 {
                return Err("JSONPath complexity limit exceeded".into());
            }
        }
    }
    Ok(())
}
