//! Validate RFC 9485's grammar and map its Unicode patterns to Rust regex.
//! In particular, ^ and $ are literals, and dot excludes both CR and LF.
use regex::Regex;

pub(super) fn compile(source: &str, whole: bool) -> Option<Regex> {
    let mut parser = Parser {
        chars: source.chars().collect(),
        pos: 0,
        depth: 0,
    };
    let translated = parser.expression()?;
    if parser.pos != parser.chars.len() {
        return None;
    }
    let translated = if whole {
        format!("\\A(?:{translated})\\z")
    } else {
        translated
    };
    Regex::new(&translated).ok()
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}
struct Unit {
    pattern: String,
    character: Option<char>,
}
impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(ch) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expression(&mut self) -> Option<String> {
        self.depth += 1;
        if self.depth > 128 {
            return None;
        }
        let mut out = String::new();
        while self.peek().is_some_and(|ch| ch != ')') {
            if self.eat('|') {
                out.push('|');
                continue;
            }
            out.push_str(&self.atom()?);
            match self.peek() {
                Some('*' | '+' | '?') => {
                    out.push(self.peek()?);
                    self.pos += 1;
                }
                Some('{') => {
                    self.pos += 1;
                    out.push('{');
                    out.push_str(&self.digits()?);
                    if self.eat(',') {
                        out.push(',');
                        if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                            out.push_str(&self.digits()?);
                        }
                    }
                    if !self.eat('}') {
                        return None;
                    }
                    out.push('}');
                }
                _ => {}
            }
        }
        self.depth -= 1;
        Some(out)
    }
    fn digits(&mut self) -> Option<String> {
        let start = self.pos;
        while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.pos += 1;
        }
        (self.pos > start).then(|| self.chars[start..self.pos].iter().collect())
    }
    fn atom(&mut self) -> Option<String> {
        let ch = self.peek()?;
        match ch {
            '(' => {
                self.pos += 1;
                let expression = self.expression()?;
                if !self.eat(')') {
                    return None;
                }
                Some(format!("(?:{expression})"))
            }
            '[' => self.class(),
            '\\' => self.escape().map(|unit| unit.pattern),
            '.' => {
                self.pos += 1;
                Some("[^\\n\\r]".into())
            }
            ')' | '*' | '+' | '?' | ']' | '{' | '}' | '|' => None,
            _ => {
                self.pos += 1;
                Some(regex::escape(&ch.to_string()))
            }
        }
    }
    fn escape(&mut self) -> Option<Unit> {
        if !self.eat('\\') {
            return None;
        }
        let ch = self.peek()?;
        self.pos += 1;
        if matches!(ch, 'p' | 'P') {
            if !self.eat('{') {
                return None;
            }
            let start = self.pos;
            while self.peek().is_some_and(|ch| ch.is_ascii_alphabetic()) {
                self.pos += 1;
            }
            let category: String = self.chars[start..self.pos].iter().collect();
            if !self.eat('}')
                || !matches!(
                    category.as_str(),
                    "L" | "Ll"
                        | "Lm"
                        | "Lo"
                        | "Lt"
                        | "Lu"
                        | "M"
                        | "Mc"
                        | "Me"
                        | "Mn"
                        | "N"
                        | "Nd"
                        | "Nl"
                        | "No"
                        | "P"
                        | "Pc"
                        | "Pd"
                        | "Pe"
                        | "Pf"
                        | "Pi"
                        | "Po"
                        | "Ps"
                        | "Z"
                        | "Zl"
                        | "Zp"
                        | "Zs"
                        | "S"
                        | "Sc"
                        | "Sk"
                        | "Sm"
                        | "So"
                        | "C"
                        | "Cc"
                        | "Cf"
                        | "Cn"
                        | "Co"
                )
            {
                return None;
            }
            return Some(Unit {
                pattern: format!("\\{ch}{{{category}}}"),
                character: None,
            });
        }
        let ch = match ch {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '(' | ')' | '*' | '+' | '-' | '.' | '?' | '[' | '\\' | ']' | '^' | '{' | '|' | '}' => {
                ch
            }
            _ => return None,
        };
        Some(Unit {
            pattern: regex::escape(&ch.to_string()),
            character: Some(ch),
        })
    }
    fn class_unit(&mut self) -> Option<Unit> {
        let ch = self.peek()?;
        if ch == '\\' {
            return self.escape();
        }
        if matches!(ch, '-' | '[' | ']') {
            return None;
        }
        self.pos += 1;
        Some(Unit {
            pattern: regex::escape(&ch.to_string()),
            character: Some(ch),
        })
    }
    fn class(&mut self) -> Option<String> {
        self.pos += 1;
        let mut out = String::from("[");
        if self.eat('^') {
            out.push('^');
        }
        let mut units = 0;
        if self.eat('-') {
            out.push_str("\\-");
            units += 1;
        }
        while self.peek()? != ']' {
            if self.peek() == Some('-') && self.chars.get(self.pos + 1) == Some(&']') {
                self.pos += 1;
                out.push_str("\\-");
                units += 1;
                break;
            }
            let start = self.class_unit()?;
            out.push_str(&start.pattern);
            units += 1;
            if self.peek() == Some('-') && self.chars.get(self.pos + 1) != Some(&']') {
                self.pos += 1;
                let end = self.class_unit()?;
                if start.character? > end.character? {
                    return None;
                }
                out.push('-');
                out.push_str(&end.pattern);
            }
        }
        if units == 0 || !self.eat(']') {
            return None;
        }
        out.push(']');
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::compile;
    #[test]
    fn mapping_and_validation() {
        for (pattern, text, expected) in [
            ("a.b", "a\rb", false),
            ("a.b", "a\nb", false),
            ("a.b", "a💡b", true),
            ("^a$", "^a$", true),
            ("^a$", "a", false),
            ("\\p{L}+", "é𝄞", false),
            ("\\p{L}+", "Éclair", true),
            ("[a-z-[0-9]]", "a", false),
            ("[a-z]+", "abc", true),
            ("[-a]+", "a-a", true),
            ("[a-]+", "a-a", true),
            ("[&~]+", "&&~~", true),
            ("a{2,3}", "aaa", true),
            ("a|bc", "bc", true),
        ] {
            assert_eq!(
                compile(pattern, true).is_some_and(|regex| regex.is_match(text)),
                expected,
                "{pattern}"
            );
        }
        for pattern in [
            "\\d+",
            "(?i)a",
            "a+?",
            "[z-a]",
            "[]",
            "[^]",
            "a**",
            "\\p{Latin}",
            "a{,2}",
        ] {
            assert!(compile(pattern, true).is_none(), "{pattern}");
        }
        assert!(compile("bc", false).unwrap().is_match("abcd"));
        assert!(!compile("bc", true).unwrap().is_match("abcd"));
    }
}
