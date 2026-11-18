use crate::{evaluate, syntax::Expr};
use bq_common::{Node, Value, compare_numbers, equivalent};
use std::cmp::Ordering;

enum ResultValue<'a> {
    Nodes(Vec<&'a Node>),
    Value(Option<Value>),
    Logical(bool),
}
impl ResultValue<'_> {
    fn logical(&self) -> bool {
        match self {
            Self::Nodes(v) => !v.is_empty(),
            Self::Logical(v) => *v,
            Self::Value(_) => false,
        }
    }
    fn value(&self) -> Option<Value> {
        match self {
            Self::Nodes(v) if v.len() == 1 => Some(v[0].value()),
            Self::Value(v) => v.clone(),
            _ => None,
        }
    }
}

pub(super) fn logical(expr: &Expr, current: &Node, root: &Node) -> bool {
    eval(expr, current, root).logical()
}

fn eval<'a>(expr: &Expr, current: &'a Node, root: &'a Node) -> ResultValue<'a> {
    use ResultValue as R;
    match expr {
        Expr::Literal(value) => R::Value(Some(value.clone())),
        Expr::Query(query) => R::Nodes(evaluate(query, current, root)),
        Expr::Not(expr) => R::Logical(!logical(expr, current, root)),
        Expr::And(left, right) => {
            R::Logical(logical(left, current, root) && logical(right, current, root))
        }
        Expr::Or(left, right) => {
            R::Logical(logical(left, current, root) || logical(right, current, root))
        }
        Expr::Compare(left, op, right) => {
            let left = eval(left, current, root).value();
            let right = eval(right, current, root).value();
            let equal = match (&left, &right) {
                (Some(a), Some(b)) => equivalent(a, b),
                (None, None) => true,
                _ => false,
            };
            let order = match (&left, &right) {
                (Some(Value::Number(a)), Some(Value::Number(b))) => Some(compare_numbers(a, b)),
                (Some(Value::String(a)), Some(Value::String(b))) => Some(a.cmp(b)),
                _ => None,
            };
            R::Logical(match op.as_str() {
                "==" => equal,
                "!=" => !equal,
                "<" => order == Some(Ordering::Less),
                ">" => order == Some(Ordering::Greater),
                "<=" => equal || order == Some(Ordering::Less),
                ">=" => equal || order == Some(Ordering::Greater),
                _ => false,
            })
        }
        Expr::Function(name, args) => {
            let args: Vec<_> = args.iter().map(|arg| eval(arg, current, root)).collect();
            match name.as_str() {
                "length" => R::Value(
                    args[0]
                        .value()
                        .and_then(|value| match value {
                            Value::String(s) => Some(s.chars().count()),
                            Value::Array(v) => Some(v.len()),
                            Value::Object(v) => Some(v.len()),
                            _ => None,
                        })
                        .map(|n| Value::Number(n.to_string())),
                ),
                "count" => R::Value(match &args[0] {
                    R::Nodes(nodes) => Some(Value::Number(nodes.len().to_string())),
                    _ => None,
                }),
                "value" => R::Value(args[0].value()),
                "match" | "search" => R::Logical(match (args[0].value(), args[1].value()) {
                    (Some(Value::String(text)), Some(Value::String(pattern))) => {
                        crate::iregexp::compile(&pattern, name == "match")
                            .is_some_and(|regex| regex.is_match(&text))
                    }
                    _ => false,
                }),
                _ => R::Value(None),
            }
        }
    }
}
