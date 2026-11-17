//! Atomic RFC 6902 operations over ordered documents.
use bp_common::{
    Document, ExecutionError, Node, NodeKind, Patch, Path, array_index, equivalent, parse_pointer,
};

pub fn apply(document: &mut Document, patch: &Patch) -> Result<(), ExecutionError> {
    apply_at(document, &[], patch)
}

/// Apply pointers relative to a selected subtree. On error the document is unchanged.
pub fn apply_at(
    document: &mut Document,
    base: &[String],
    patch: &Patch,
) -> Result<(), ExecutionError> {
    let mut working = document.clone();
    execute(&mut working, base, patch)?;
    *document = working;
    Ok(())
}

fn resolve(base: &[String], pointer: &str) -> Result<Path, ExecutionError> {
    let mut path = base.to_vec();
    path.extend(parse_pointer(pointer)?);
    Ok(path)
}

fn execute(document: &mut Document, base: &[String], patch: &Patch) -> Result<(), ExecutionError> {
    match patch {
        Patch::Add { path, value } => {
            let path = resolve(base, path)?;
            let node = document.allocate(value);
            add(document, &path, node)
        }
        Patch::Remove { path } => remove(document, &resolve(base, path)?).map(|_| ()),
        Patch::Replace { path, value } => {
            let path = resolve(base, path)?;
            let node = document.allocate(value);
            *document.at_mut(&path).ok_or_else(|| missing(&path))? = node;
            Ok(())
        }
        Patch::Copy { path, from } => {
            let from = resolve(base, from)?;
            let value = document.at(&from).ok_or_else(|| missing(&from))?.value();
            let node = document.allocate(&value);
            add(document, &resolve(base, path)?, node)
        }
        Patch::Move { path, from } => {
            let path = resolve(base, path)?;
            let from = resolve(base, from)?;
            if path == from {
                document.at(&from).ok_or_else(|| missing(&from))?;
                return Ok(());
            }
            if path.len() > from.len() && path.starts_with(&from) {
                return Err(ExecutionError::new(
                    "cannot move a node into its descendant",
                    path,
                ));
            }
            let node = remove(document, &from)?;
            add(document, &path, node)
        }
        Patch::Test { path, value } => {
            let path = resolve(base, path)?;
            let actual = document.at(&path).ok_or_else(|| missing(&path))?;
            if equivalent(&actual.value(), value) {
                Ok(())
            } else {
                Err(ExecutionError::new("JSON Patch test failed", path))
            }
        }
    }
}

fn missing(path: &[String]) -> ExecutionError {
    ExecutionError::new("target does not exist", path.to_vec())
}

fn add(document: &mut Document, path: &[String], node: Node) -> Result<(), ExecutionError> {
    let Some((key, parent)) = path.split_last() else {
        document.root = Some(node);
        return Ok(());
    };
    let parent = document.at_mut(parent).ok_or_else(|| missing(path))?;
    match &mut parent.kind {
        NodeKind::Object(members) => {
            if let Some((_, value)) = members.iter_mut().find(|(k, _)| k == key) {
                *value = node;
            } else {
                members.push((key.clone(), node));
            }
        }
        NodeKind::Array(values) => {
            let index = if key == "-" {
                values.len()
            } else {
                array_index(key)
                    .filter(|&i| i <= values.len())
                    .ok_or_else(|| missing(path))?
            };
            values.insert(index, node);
        }
        _ => {
            return Err(ExecutionError::new(
                "target parent is not a container",
                path.to_vec(),
            ));
        }
    }
    Ok(())
}

fn remove(document: &mut Document, path: &[String]) -> Result<Node, ExecutionError> {
    let Some((key, parent)) = path.split_last() else {
        return document.root.take().ok_or_else(|| missing(path));
    };
    let parent = document.at_mut(parent).ok_or_else(|| missing(path))?;
    match &mut parent.kind {
        NodeKind::Object(members) => {
            let index = members
                .iter()
                .position(|(k, _)| k == key)
                .ok_or_else(|| missing(path))?;
            Ok(members.remove(index).1)
        }
        NodeKind::Array(values) => {
            let index = array_index(key)
                .filter(|&i| i < values.len())
                .ok_or_else(|| missing(path))?;
            Ok(values.remove(index))
        }
        _ => Err(ExecutionError::new(
            "target parent is not a container",
            path.to_vec(),
        )),
    }
}
