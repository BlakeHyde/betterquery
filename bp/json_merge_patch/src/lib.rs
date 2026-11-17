//! RFC 7396 merge patches, retaining object order and surviving node identities.
use bp_common::{Document, ExecutionError, Node, NodeKind, Value};

pub fn apply(document: &mut Document, patch: &Value) -> Result<(), ExecutionError> {
    apply_at(document, &[], patch)
}

pub fn apply_at(
    document: &mut Document,
    path: &[String],
    patch: &Value,
) -> Result<(), ExecutionError> {
    let mut working = document.clone();
    let mut node = match working.at(path) {
        Some(node) => node.clone(),
        None if path.is_empty() => working.allocate(&Value::Null),
        None => {
            return Err(ExecutionError::new(
                "merge target does not exist",
                path.to_vec(),
            ));
        }
    };
    merge(&mut working, &mut node, patch);
    if path.is_empty() {
        working.root = Some(node);
    } else {
        *working.at_mut(path).expect("merge target remains present") = node;
    }
    *document = working;
    Ok(())
}

fn merge(document: &mut Document, target: &mut Node, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = document.allocate(patch);
        return;
    };
    if !matches!(target.kind, NodeKind::Object(_)) {
        *target = document.allocate(&Value::Object(Vec::new()));
    }
    let NodeKind::Object(members) = &mut target.kind else {
        unreachable!()
    };
    for (key, value) in patch {
        let existing = members.iter().position(|(k, _)| k == key);
        if matches!(value, Value::Null) {
            if let Some(index) = existing {
                members.remove(index);
            }
        } else if let Some(index) = existing {
            merge(document, &mut members[index].1, value);
        } else {
            let mut node = document.allocate(&Value::Null);
            merge(document, &mut node, value);
            members.push((key.clone(), node));
        }
    }
}
