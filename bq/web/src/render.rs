use bq_common::{Patch, Value, pointer};
use bq_timeline::{Event, EventKind, InputError, Timeline};
use std::fmt::Write;

type Evaluation = Result<Timeline, InputError>;

pub fn render(program: &str, document: &str, evaluation: Option<&Evaluation>) -> String {
    let mut html = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>BetterQuery · Document explorer</title><style>",
    );
    html.push_str(include_str!("style.css"));
    html.push_str("</style></head><body><header><a class=\"brand\" href=\"/\"><span class=\"mark\">bq</span>BetterQuery</a><span class=\"header-note\">DOCUMENT EXPLORER</span><span class=\"local\"><i></i>Local workspace</span></header><main><div class=\"intro\"><div><p class=\"eyebrow\">QUERY · PATCH · MERGE</p><h1>See every transformation.</h1><p class=\"subtitle\">Apply a program to your document. Explore the matches and follow each change.</p></div><span class=\"version\">S-expression language<span>JSON in. Clarity out.</span></span></div><nav aria-label=\"Examples\"><span>Try an example</span><a href=\"/?example=query\">Remove matching keys</a><a href=\"/?example=merge\">Merge an object</a><a href=\"/?example=array\">Stable array targets</a><a href=\"/?example=rollback\">Explore a rollback</a></nav><form method=\"post\" action=\"/\"><div class=\"editors\"><section class=\"panel\"><div class=\"panel-head\"><label for=\"document\"><span class=\"step\">01</span>Input document</label><span class=\"language\">JSON</span></div><textarea id=\"document\" name=\"document\" spellcheck=\"false\" required aria-describedby=\"document-hint\">");
    escape(&mut html, document);
    html.push_str("</textarea><p class=\"hint\" id=\"document-hint\">Object order and exact number values are preserved.</p></section><section class=\"panel\"><div class=\"panel-head\"><label for=\"program\"><span class=\"step\">02</span>Transformation program</label><span class=\"language\">BP</span></div><textarea id=\"program\" name=\"program\" spellcheck=\"false\" aria-describedby=\"program-hint\">");
    escape(&mut html, program);
    html.push_str("</textarea><p class=\"hint\" id=\"program-hint\">Forms run in order. The whole program succeeds or rolls back.</p></section></div><div class=\"run-bar\"><span>One document. A complete trail of changes.</span><button type=\"submit\">Run program <span aria-hidden=\"true\">→</span></button></div></form>");
    match evaluation {
        None => html.push_str("<section class=\"empty-state\"><span class=\"empty-symbol\" aria-hidden=\"true\">↳</span><h2>Your timeline starts here</h2><p>Run the program to see selected nodes, intermediate documents, and the final result.</p></section>"),
        Some(Err(error)) => {
            html.push_str("<section class=\"notice failure\" role=\"alert\"><h2>Check your input</h2><p>");
            escape(&mut html,&error.to_string());
            html.push_str("</p><p>No transformations were applied.</p></section>");
        }
        Some(Ok(timeline)) => render_timeline(&mut html,timeline,program),
    }
    html.push_str("<footer><span>BetterQuery</span><span>JSON Patch · JSON Merge Patch · JSONPath</span></footer></main></body></html>");
    html
}

fn render_timeline(html: &mut String, timeline: &Timeline, source: &str) {
    let (class, title) = if timeline.succeeded() {
        ("success", "Program completed")
    } else {
        ("failure", "Program rolled back")
    };
    write!(
        html,
        "<section class=\"notice {class}\" role=\"status\"><h2>{title}</h2><p>"
    )
    .unwrap();
    if let Some(error) = &timeline.error {
        escape(html, &error.to_string());
    } else {
        html.push_str("All transformations succeeded. Explore the execution tree below.");
    }
    html.push_str("</p>");
    if let Some(span) = timeline
        .error
        .as_ref()
        .and_then(|e| e.invalidated_by.as_ref())
    {
        html.push_str("<p>Target last valid before this transformation:</p><pre>");
        escape(
            html,
            source.get(span.clone()).unwrap_or("Source unavailable"),
        );
        html.push_str("</pre>");
    }
    html.push_str("</section><div class=\"output-grid\"><section class=\"execution\"><div class=\"section-heading\"><h2>Execution timeline</h2><span>");
    write!(html, "{} events", timeline.events.len()).unwrap();
    html.push_str("</span></div>");
    if timeline.events.is_empty() {
        html.push_str("<p class=\"hint\">The empty program leaves the document unchanged.</p>");
    }
    render_events(html, &timeline.events, None, source);
    html.push_str("</section><aside class=\"result panel\"><div class=\"panel-head\"><h2>Result document</h2><span class=\"language\">");
    html.push_str(if timeline.succeeded() {
        "COMMITTED"
    } else {
        "RESTORED"
    });
    html.push_str("</span></div><pre class=\"result-code\">");
    escape(html, &pretty(timeline.result.as_ref()));
    html.push_str("</pre></aside></div>");
}

fn render_events(html: &mut String, events: &[Event], parent: Option<usize>, source: &str) {
    html.push_str("<ol class=\"events\">");
    for event in events.iter().filter(|e| e.parent == parent) {
        let label = match &event.kind {
            EventKind::Query { .. } => "Query".to_owned(),
            EventKind::Merge(_) => "Merge".to_owned(),
            EventKind::Rollback(_) => "Rollback".to_owned(),
            EventKind::Patch(patch) => format!(
                "Patch · {}",
                match patch {
                    Patch::Add { .. } => "add",
                    Patch::Remove { .. } => "remove",
                    Patch::Replace { .. } => "replace",
                    Patch::Move { .. } => "move",
                    Patch::Copy { .. } => "copy",
                    Patch::Test { .. } => "test",
                }
            ),
        };
        write!(html,"<li id=\"event-{}\"><article class=\"event {}\"><div class=\"event-heading\"><span class=\"event-number\">{:02}</span><h3>",event.id,if event.error.is_some(){"event-failure"}else{""},event.id+1).unwrap();
        escape(html, &label);
        html.push_str("</h3><code class=\"target\">");
        let target = pointer(&event.target);
        escape(html, if target.is_empty() { "(root)" } else { &target });
        html.push_str("</code></div>");
        if let EventKind::Query {
            expression,
            matches,
        } = &event.kind
        {
            html.push_str("<pre class=\"query-code\">");
            escape(html, expression);
            html.push_str("</pre><details class=\"matches\" open><summary>");
            write!(
                html,
                "{} selected {}",
                matches.len(),
                if matches.len() == 1 { "node" } else { "nodes" }
            )
            .unwrap();
            html.push_str("</summary><ul>");
            for selected in matches {
                html.push_str("<li><code>");
                let path = pointer(&selected.path);
                escape(html, if path.is_empty() { "(root)" } else { &path });
                html.push_str("</code><pre>");
                escape(html, &pretty(Some(&selected.value)));
                html.push_str("</pre></li>");
            }
            html.push_str("</ul></details>");
        }
        if let Some(error) = &event.error {
            html.push_str("<p class=\"event-error\">");
            escape(html, &error.to_string());
            html.push_str("</p>");
        }
        html.push_str("<details class=\"source\"><summary>Transformation source</summary><pre>");
        escape(
            html,
            source
                .get(event.span.clone())
                .unwrap_or("Source unavailable"),
        );
        html.push_str("</pre></details><details class=\"snapshots\"><summary>Document before &amp; after</summary><div class=\"snapshot-grid\"><div><h4>Before</h4><pre>");
        escape(html, &pretty(event.before.as_ref()));
        html.push_str("</pre></div><div><h4>After</h4><pre>");
        escape(html, &pretty(event.after.as_ref()));
        html.push_str("</pre></div></div></details></article>");
        if events.iter().any(|e| e.parent == Some(event.id)) {
            render_events(html, events, Some(event.id), source);
        }
        html.push_str("</li>");
    }
    html.push_str("</ol>");
}

fn escape(html: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '"' => html.push_str("&quot;"),
            '\'' => html.push_str("&#39;"),
            _ => html.push(ch),
        }
    }
}

/// Pretty-print without losing member order or number spelling.
fn pretty(value: Option<&Value>) -> String {
    fn emit(value: &Value, level: usize, out: &mut String) {
        match value {
            Value::Array(values) if !values.is_empty() => {
                out.push_str("[\n");
                for (i, value) in values.iter().enumerate() {
                    out.push_str(&"  ".repeat(level + 1));
                    emit(value, level + 1, out);
                    out.push_str(if i + 1 == values.len() { "\n" } else { ",\n" });
                }
                out.push_str(&"  ".repeat(level));
                out.push(']');
            }
            Value::Object(values) if !values.is_empty() => {
                out.push_str("{\n");
                for (i, (key, value)) in values.iter().enumerate() {
                    out.push_str(&"  ".repeat(level + 1));
                    out.push_str(
                        &bq_document_parser::to_string(&Value::String(key.clone())).unwrap(),
                    );
                    out.push_str(": ");
                    emit(value, level + 1, out);
                    out.push_str(if i + 1 == values.len() { "\n" } else { ",\n" });
                }
                out.push_str(&"  ".repeat(level));
                out.push('}');
            }
            _ => out.push_str(
                &bq_document_parser::to_string(value)
                    .unwrap_or_else(|error| format!("Invalid value: {error}")),
            ),
        }
    }
    let Some(value) = value else {
        return "Document root removed".into();
    };
    let mut out = String::new();
    emit(value, 0, &mut out);
    out
}
