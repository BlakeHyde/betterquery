//! RFC 9535 selection over the ordered, exact-number IR.
//! Results are de-duplicated and returned in document depth-first order.
mod expression;
mod iregexp;
mod syntax;

use bq_common::{Document, ExecutionError, Node, NodeId, NodeKind, Path, Value};
use std::collections::HashSet;
use syntax::{Query, Selector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub id: NodeId,
    /// Absolute path at the instant the query was evaluated.
    pub path: Path,
    pub value: Value,
}

/// Validate query syntax and function types, enforcing the evaluator's limits.
pub fn validate(source: &str) -> Result<(), String> {
    syntax::parse(source).map(|_| ())
}

/// `$` refers to the subtree at `base`. Missing bases are errors; no matches is valid.
pub fn select(
    document: &Document,
    base: &[String],
    source: &str,
) -> Result<Vec<Match>, ExecutionError> {
    let query =
        syntax::parse(source).map_err(|message| ExecutionError::new(message, base.to_vec()))?;
    let root = document
        .at(base)
        .ok_or_else(|| ExecutionError::new("query target does not exist", base.to_vec()))?;
    let selected: HashSet<_> = evaluate(&query, root, root)
        .iter()
        .map(|node| node.id)
        .collect();
    let mut matches = Vec::new();
    root.walk(&mut base.to_vec(), &mut |path, node| {
        if selected.contains(&node.id) {
            matches.push(Match {
                id: node.id,
                path: path.clone(),
                value: node.value(),
            });
        }
    });
    Ok(matches)
}

pub fn query(value: &Value, source: &str) -> Result<Vec<Value>, ExecutionError> {
    Ok(select(&Document::new(value.clone()), &[], source)?
        .into_iter()
        .map(|m| m.value)
        .collect())
}

fn children(node: &Node) -> Vec<&Node> {
    match &node.kind {
        NodeKind::Array(v) => v.iter().collect(),
        NodeKind::Object(v) => v.iter().map(|(_, v)| v).collect(),
        _ => Vec::new(),
    }
}

fn evaluate<'a>(query: &Query, current: &'a Node, root: &'a Node) -> Vec<&'a Node> {
    let mut nodes = vec![if query.relative { current } else { root }];
    for segment in &query.segments {
        let mut next = Vec::new();
        for node in nodes {
            let mut inputs = vec![node];
            if segment.descendant {
                fn descend<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
                    for child in children(node) {
                        out.push(child);
                        descend(child, out);
                    }
                }
                descend(node, &mut inputs);
            }
            for input in inputs {
                for selector in &segment.selectors {
                    match selector {
                        Selector::Name(name) => {
                            if let NodeKind::Object(members) = &input.kind
                                && let Some((_, node)) = members.iter().find(|(k, _)| k == name)
                            {
                                next.push(node);
                            }
                        }
                        Selector::Wildcard => next.extend(children(input)),
                        Selector::Index(index) => {
                            if let NodeKind::Array(values) = &input.kind {
                                let len = values.len() as i64;
                                let index = if *index < 0 { len + index } else { *index };
                                if let Some(node) =
                                    usize::try_from(index).ok().and_then(|i| values.get(i))
                                {
                                    next.push(node);
                                }
                            }
                        }
                        Selector::Slice(start, end, step) => {
                            if let NodeKind::Array(values) = &input.kind {
                                let len = values.len() as i64;
                                if *step == 0 {
                                    continue;
                                }
                                let normalize = |i: i64| if i < 0 { len + i } else { i };
                                let (mut index, end) = if *step > 0 {
                                    (
                                        start.map_or(0, normalize).clamp(0, len),
                                        end.map_or(len, normalize).clamp(0, len),
                                    )
                                } else {
                                    (
                                        start.map_or(len - 1, normalize).clamp(-1, len - 1),
                                        end.map_or(-1, normalize).clamp(-1, len - 1),
                                    )
                                };
                                while if *step > 0 { index < end } else { index > end } {
                                    next.push(&values[index as usize]);
                                    index += step;
                                }
                            }
                        }
                        Selector::Filter(expr) => next.extend(
                            children(input)
                                .into_iter()
                                .filter(|node| expression::logical(expr, node, root)),
                        ),
                    }
                }
            }
        }
        nodes = next;
    }
    nodes
}
