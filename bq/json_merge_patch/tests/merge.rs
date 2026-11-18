use bq_common::Document;
use bq_document_parser::parse;

#[test]
fn rfc7396_appendix_a() {
    for (original, patch, expected) in [
        (r#"{"a":"b"}"#, r#"{"a":"c"}"#, r#"{"a":"c"}"#),
        (r#"{"a":"b"}"#, r#"{"b":"c"}"#, r#"{"a":"b","b":"c"}"#),
        (r#"{"a":"b"}"#, r#"{"a":null}"#, "{}"),
        (r#"{"a":"b","b":"c"}"#, r#"{"a":null}"#, r#"{"b":"c"}"#),
        (r#"{"a":["b"]}"#, r#"{"a":"c"}"#, r#"{"a":"c"}"#),
        (r#"{"a":"c"}"#, r#"{"a":["b"]}"#, r#"{"a":["b"]}"#),
        (
            r#"{"a":{"b":"c"}}"#,
            r#"{"a":{"b":"d","c":null}}"#,
            r#"{"a":{"b":"d"}}"#,
        ),
        (r#"{"a":[{"b":"c"}]}"#, r#"{"a":[1]}"#, r#"{"a":[1]}"#),
        (r#"["a","b"]"#, r#"["c","d"]"#, r#"["c","d"]"#),
        (r#"{"a":"b"}"#, r#"["c"]"#, r#"["c"]"#),
        (r#"{"a":"foo"}"#, "null", "null"),
        (r#"{"a":"foo"}"#, r#""bar""#, r#""bar""#),
        (r#"{"e":null}"#, r#"{"a":1}"#, r#"{"e":null,"a":1}"#),
        ("[1,2]", r#"{"a":"b","c":null}"#, r#"{"a":"b"}"#),
        ("{}", r#"{"a":{"bb":{"ccc":null}}}"#, r#"{"a":{"bb":{}}}"#),
    ] {
        let mut doc = Document::new(parse(original).unwrap());
        bq_json_merge_patch::apply(&mut doc, &parse(patch).unwrap()).unwrap();
        assert_eq!(doc.value().unwrap(), parse(expected).unwrap());
    }
}

#[test]
fn merge_preserves_untouched_identities_and_member_order() {
    let mut doc = Document::new(parse(r#"{"z":{"keep":1,"replace":2},"a":3}"#).unwrap());
    let keep = doc.at(&["z".into(), "keep".into()]).unwrap().id;
    let replaced = doc.at(&["z".into(), "replace".into()]).unwrap().id;
    bq_json_merge_patch::apply(&mut doc, &parse(r#"{"z":{"replace":2},"new":4}"#).unwrap())
        .unwrap();
    assert_eq!(doc.at(&["z".into(), "keep".into()]).unwrap().id, keep);
    assert_ne!(
        doc.at(&["z".into(), "replace".into()]).unwrap().id,
        replaced
    );
    assert_eq!(
        bq_document_parser::to_string(&doc.value().unwrap()).unwrap(),
        r#"{"z":{"keep":1,"replace":2},"a":3,"new":4}"#
    );
}
