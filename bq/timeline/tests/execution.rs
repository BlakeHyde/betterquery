use bq_common::Document;
use bq_document_parser::parse;
use bq_timeline::{EventKind, evaluate, run};

fn result(program: &str, document: &str, expected: &str) {
    let timeline = evaluate(program, document).unwrap();
    assert!(timeline.succeeded(), "{:?}", timeline.error);
    assert_eq!(timeline.result.unwrap(), parse(expected).unwrap());
}

#[test]
fn design_examples() {
    result(
        r#"(query "$..[?(@.sharedKey == true)]" (patch remove (path "/sharedKey")))"#,
        r#"{"a":{"sharedKey":true,"uniqueA":1},"b":{"sharedKey":true,"uniqueB":2},"c":{"sharedKey":false}}"#,
        r#"{"a":{"uniqueA":1},"b":{"uniqueB":2},"c":{"sharedKey":false}}"#,
    );
    result(
        r#"(query "$..[?(@.sharedKey == true)]" (merge (object (answer . 42))))"#,
        r#"{"a":{"sharedKey":true},"b":{"sharedKey":false}}"#,
        r#"{"a":{"sharedKey":true,"answer":42},"b":{"sharedKey":false}}"#,
    );
}

#[test]
fn frozen_array_locations_and_sequential_top_level_indices() {
    result(
        r#"(query "$[1,3]" (patch remove (path "")))"#,
        r#"["a","b","c","d","e"]"#,
        r#"["a","c","e"]"#,
    );
    result(
        r#"(patch remove (path "/1")) (patch remove (path "/3"))"#,
        r#"["a","b","c","d","e"]"#,
        r#"["a","c","d"]"#,
    );
    result(
        r#"(query "$[0,1,2]" (patch add (path "") (value 9)))"#,
        "[1,2,3]",
        "[9,1,9,2,9,3]",
    );
}

#[test]
fn deduplicated_query_targets() {
    result(
        r#"(query "$[1,1,1]" (patch remove (path "")))"#,
        "[0,1,2]",
        "[0,2]",
    );
}

#[test]
fn replaced_child_fails_even_when_path_survives() {
    let program = r#"(merge (object (earlier . TRUE))) (query "$..*" (merge (object (b . 1))))"#;
    let original = parse(r#"{"a":{"b":{}}}"#).unwrap();
    let mut doc = Document::new(original.clone());
    let before = doc.clone();
    let timeline = run(&mut doc, &bq_parser::parse(program).unwrap());
    assert_eq!(doc, before);
    assert_eq!(timeline.result, Some(original));
    let error = timeline.error.unwrap();
    assert_eq!(error.path, ["a", "b"]);
    assert_eq!(
        &program[error.invalidated_by.unwrap()],
        "(merge (object (b . 1)))"
    );
    assert!(matches!(
        timeline.events.last().unwrap().kind,
        EventKind::Rollback(_)
    ));
}

#[test]
fn equal_value_replacement_also_invalidates_child() {
    let timeline = evaluate(
        r#"(query "$..*" (merge (object (b . 1))))"#,
        r#"{"a":{"b":1}}"#,
    )
    .unwrap();
    assert!(!timeline.succeeded());
    assert!(timeline.error.unwrap().invalidated_by.is_some());
}

#[test]
fn snapshot_selection_and_nested_roots() {
    result(
        r#"(query "$..[?@.flag]" (merge (object (new . (object (flag . TRUE))))))"#,
        r#"{"a":{"flag":true}}"#,
        r#"{"a":{"flag":true,"new":{"flag":true}}}"#,
    );
    result(
        r#"(query "$.groups[*]" (query "$.items[?@.n == $['wanted']]" (patch replace (path "/n") (value 9))))"#,
        r#"{"groups":[{"wanted":1,"items":[{"n":1},{"n":2}]},{"wanted":2,"items":[{"n":1},{"n":2}]}]}"#,
        r#"{"groups":[{"wanted":1,"items":[{"n":9},{"n":2}]},{"wanted":2,"items":[{"n":1},{"n":9}]}]}"#,
    );
}

#[test]
fn parent_before_children_and_untouched_descendants_remain_valid() {
    result(
        r#"(query "$..*" (merge (object (x . TRUE))))"#,
        r#"{"a":{"b":{}}}"#,
        r#"{"a":{"b":{"x":true},"x":true}}"#,
    );
    let timeline = evaluate(
        r#"(query "$..*" (patch remove (path "")))"#,
        r#"{"a":{"b":1}}"#,
    )
    .unwrap();
    assert!(!timeline.succeeded());
    assert_eq!(timeline.error.unwrap().path, ["a", "b"]);
}

#[test]
fn timeline_tree_snapshots_and_rollback() {
    let timeline = evaluate(
        r#"(query "$.a" (query "$[*]" (merge 2)))"#,
        r#"{"a":[0,1]}"#,
    )
    .unwrap();
    assert_eq!(timeline.events.len(), 4);
    assert_eq!(timeline.events[0].parent, None);
    assert_eq!(timeline.events[1].parent, Some(0));
    assert_eq!(timeline.events[2].parent, Some(1));
    assert_eq!(timeline.events[3].parent, Some(1));
    assert_eq!(timeline.events[0].after, timeline.result);
    let timeline = evaluate(r#"(merge 3) (patch test (path "") (value 4))"#, "1").unwrap();
    assert_eq!(timeline.result, Some(parse("1").unwrap()));
    assert_eq!(timeline.events[0].after, Some(parse("3").unwrap()));
    assert_eq!(
        timeline.events.last().unwrap().after,
        Some(parse("1").unwrap())
    );
}

#[test]
fn empty_program_no_matches_and_removed_root() {
    result("", "null", "null");
    result(
        r#"(query "$.missing" (patch remove (path "/bad")))"#,
        "{}",
        "{}",
    );
    let timeline = evaluate(r#"(patch remove (path ""))"#, "null").unwrap();
    assert!(timeline.succeeded());
    assert_eq!(timeline.result, None);
    result(
        r#"(patch remove (path "")) (merge (object (a . 1)))"#,
        "null",
        r#"{"a":1}"#,
    );
}

#[test]
fn later_queries_see_previous_results_and_errors_have_source() {
    result(
        r#"(merge (object (a . (array 1 2)))) (query "$.a[*]" (merge 3))"#,
        "{}",
        r#"{"a":[3,3]}"#,
    );
    let timeline = evaluate(
        r#"(query "$.a[*]" (patch remove (path "/x")))"#,
        r#"{"a":[{"x":1},{}]}"#,
    )
    .unwrap();
    assert!(!timeline.succeeded());
    assert!(timeline.error.unwrap().span.is_some());
}
