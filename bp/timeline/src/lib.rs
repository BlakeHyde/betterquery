//! Atomic program execution with a tree of query selections and transformations.
use bp_common::{Document, ExecutionError, Form, FormKind, NodeId, Patch, Path, Program, Value};
use bp_json_path::Match;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Query {
        expression: String,
        matches: Vec<Match>,
    },
    Patch(Patch),
    Merge(Value),
    Rollback(ExecutionError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: usize,
    /// Parent query event; nested executions form a tree.
    pub parent: Option<usize>,
    pub span: Range<usize>,
    pub target: Path,
    pub kind: EventKind,
    /// Whole-document snapshots; None means the document root was removed.
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub error: Option<ExecutionError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    pub initial: Option<Value>,
    /// The committed result. On failure this is identical to initial.
    pub result: Option<Value>,
    pub events: Vec<Event>,
    pub error: Option<ExecutionError>,
}

impl Timeline {
    pub fn succeeded(&self) -> bool {
        self.error.is_none()
    }
}

/// The caller's document is committed only after every top-level form succeeds.
/// Failed runs retain their speculative events, followed by an explicit rollback.
pub fn run(document: &mut Document, program: &Program) -> Timeline {
    let initial = document.value();
    let mut engine = Engine {
        document: document.clone(),
        events: Vec::new(),
        invalidated: HashMap::new(),
    };
    let mut error = None;
    for form in program {
        if let Err(failure) = engine.form(form, None, None, 0) {
            let id = engine.events.len();
            engine.events.push(Event {
                id,
                parent: None,
                span: form.span.clone(),
                target: failure.path.clone(),
                kind: EventKind::Rollback(failure.clone()),
                before: engine.document.value(),
                after: initial.clone(),
                error: Some(failure.clone()),
            });
            error = Some(failure);
            break;
        }
    }
    if error.is_none() {
        *document = engine.document;
    }
    Timeline {
        initial,
        result: document.value(),
        events: engine.events,
        error,
    }
}

#[derive(Debug)]
pub enum InputError {
    Program(bp_parser::Error),
    Document(bp_document_parser::Error),
}
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Program(e) => write!(f, "Program: {e}"),
            Self::Document(e) => write!(f, "Document: {e}"),
        }
    }
}
impl std::error::Error for InputError {}

pub fn evaluate(program: &str, document: &str) -> Result<Timeline, InputError> {
    let program = bp_parser::parse(program).map_err(InputError::Program)?;
    let document = bp_document_parser::parse(document).map_err(InputError::Document)?;
    Ok(run(&mut Document::new(document), &program))
}

struct Engine {
    document: Document,
    events: Vec<Event>,
    invalidated: HashMap<NodeId, (Path, Range<usize>)>,
}
impl Engine {
    fn form(
        &mut self,
        form: &Form,
        selected: Option<&Match>,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<(), ExecutionError> {
        if depth >= 128 {
            return Err(ExecutionError::new(
                "execution nesting exceeds 128",
                Vec::new(),
            ));
        }
        let target = match selected {
            None => Vec::new(),
            Some(selected) => self.document.location(selected.id).ok_or_else(|| {
                let mut error = ExecutionError::new(
                    "frozen query target was removed or replaced",
                    selected.path.clone(),
                );
                error.span = Some(form.span.clone());
                if let Some((path, span)) = self.invalidated.get(&selected.id) {
                    error.path = path.clone();
                    error.invalidated_by = Some(span.clone());
                }
                error
            })?,
        };
        let before = self.document.value();
        let id = self.events.len();
        let kind = match &form.kind {
            FormKind::Query { query, .. } => EventKind::Query {
                expression: query.clone(),
                matches: Vec::new(),
            },
            FormKind::Patch(patch) => EventKind::Patch(patch.clone()),
            FormKind::Merge(value) => EventKind::Merge(value.clone()),
        };
        self.events.push(Event {
            id,
            parent,
            span: form.span.clone(),
            target: target.clone(),
            kind,
            before: before.clone(),
            after: before,
            error: None,
        });
        let result = match &form.kind {
            FormKind::Query { query, body } => {
                match bp_json_path::select(&self.document, &target, query) {
                    Err(error) => Err(error),
                    Ok(matches) => {
                        self.events[id].kind = EventKind::Query {
                            expression: query.clone(),
                            matches: matches.clone(),
                        };
                        let mut result = Ok(());
                        for selected in matches {
                            if let Err(error) =
                                self.form(body, Some(&selected), Some(id), depth + 1)
                            {
                                result = Err(error);
                                break;
                            }
                        }
                        result
                    }
                }
            }
            FormKind::Patch(patch) => self.transform(form, |document| {
                bp_json_patch::apply_at(document, &target, patch)
            }),
            FormKind::Merge(value) => self.transform(form, |document| {
                bp_json_merge_patch::apply_at(document, &target, value)
            }),
        };
        self.events[id].after = self.document.value();
        if let Err(mut error) = result {
            if error.span.is_none() {
                error.span = Some(form.span.clone());
            }
            self.events[id].error = Some(error.clone());
            Err(error)
        } else {
            Ok(())
        }
    }

    fn transform(
        &mut self,
        form: &Form,
        apply: impl FnOnce(&mut Document) -> Result<(), ExecutionError>,
    ) -> Result<(), ExecutionError> {
        let locations = self.document.locations();
        apply(&mut self.document)?;
        if let Some(value) = self.document.value() {
            bp_document_parser::to_string(&value)
                .map_err(|error| ExecutionError::new(error.message, error.path))?;
        }
        let remaining: HashSet<_> = self
            .document
            .locations()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        for (id, path) in locations {
            if !remaining.contains(&id) {
                self.invalidated.insert(id, (path, form.span.clone()));
            }
        }
        Ok(())
    }
}
