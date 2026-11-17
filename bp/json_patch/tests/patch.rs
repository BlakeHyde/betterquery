use bp_common::{Document, FormKind};
use bp_document_parser::parse;

fn run(source: &str, program: &str) -> Document {
    let mut document = Document::new(parse(source).unwrap());
    for form in bp_parser::parse(program).unwrap() {
        let FormKind::Patch(patch) = form.kind else {
            panic!()
        };
        bp_json_patch::apply(&mut document, &patch).unwrap();
    }
    document
}

#[test]
fn all_operations_and_order() {
    let doc = run(
        r#"{"a":[1,2,3],"b":{}}"#,
        r#"
        (patch add (path "/a/1") (value 4))
        (patch remove (path "/a/0"))
        (patch replace (path "/a/1") (value 5))
        (patch move (from "/a/0") (path "/a/2"))
        (patch copy (from "/a") (path "/b/c"))
        (patch test (path "/b/c") (value (array 5.0 3e0 4)))
        (patch add (path "/a/-") (value 6))
    "#,
    );
    assert_eq!(
        doc.value().unwrap(),
        parse(r#"{"a":[5,3,4,6],"b":{"c":[5,3,4]}}"#).unwrap()
    );
}

#[test]
fn failures_are_atomic() {
    for program in [
        r#"(patch move (from "/a") (path "/a/child"))"#,
        r#"(patch move (from "/a") (path "/missing/child"))"#,
        r#"(patch remove (path "/b/01"))"#,
        r#"(patch remove (path "/b/-"))"#,
        r#"(patch add (path "/b/9") (value 1))"#,
        r#"(patch replace (path "/missing") (value 1))"#,
        r#"(patch test (path "/a") (value 2))"#,
    ] {
        let mut doc = Document::new(parse(r#"{"a":{},"b":[1,2]}"#).unwrap());
        let before = doc.clone();
        let FormKind::Patch(patch) = &bp_parser::parse(program).unwrap()[0].kind else {
            panic!()
        };
        assert!(bp_json_patch::apply(&mut doc, patch).is_err(), "{program}");
        assert_eq!(doc, before);
    }
}

#[test]
fn root_removal_and_recreation() {
    let doc = run(
        "null",
        r#"(patch remove (path "")) (patch add (path "") (value 42))"#,
    );
    assert_eq!(doc.value(), Some(parse("42").unwrap()));
    assert!(run("null", r#"(patch remove (path ""))"#).root.is_none());
}

#[test]
fn pointer_escaping_and_object_equality() {
    run(
        r#"{"a/b":{"~":1},"x":{"a":1,"b":2}}"#,
        r#"
       (patch test (path "/a~1b/~0") (value 1.000))
       (patch test (path "/x") (value (object (b . 2) (a . 1))))
    "#,
    );
}

#[test]
fn move_keeps_identity_copy_and_replace_do_not() {
    let mut doc = Document::new(parse(r#"{"a":1}"#).unwrap());
    let id = doc.at(&["a".into()]).unwrap().id;
    for program in [
        r#"(patch move (from "/a") (path "/b"))"#,
        r#"(patch copy (from "/b") (path "/c"))"#,
    ] {
        let FormKind::Patch(patch) = &bp_parser::parse(program).unwrap()[0].kind else {
            panic!()
        };
        bp_json_patch::apply(&mut doc, patch).unwrap();
    }
    assert_eq!(doc.at(&["b".into()]).unwrap().id, id);
    assert_ne!(doc.at(&["c".into()]).unwrap().id, id);
}

#[test]
fn moving_to_same_location_preserves_order_and_identity() {
    let mut doc = Document::new(parse(r#"{"a":1,"b":2}"#).unwrap());
    let before = doc.clone();
    let FormKind::Patch(patch) =
        &bp_parser::parse(r#"(patch move (from "/a") (path "/a"))"#).unwrap()[0].kind
    else {
        panic!()
    };
    bp_json_patch::apply(&mut doc, patch).unwrap();
    assert_eq!(doc, before);
}
