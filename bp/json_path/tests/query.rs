use bp_common::{Document, Value, pointer};
use bp_document_parser::parse;
use bp_json_path::{query, select};

#[test]
fn ordering_deduplication_and_pointer_escaping() {
    let doc = Document::new(parse(r#"{"z":{"a/b":1,"~":2},"a":[3,4]}"#).unwrap());
    let matches = select(&doc, &[], "$['a','z','z']").unwrap();
    assert_eq!(
        matches.iter().map(|m| pointer(&m.path)).collect::<Vec<_>>(),
        ["/z", "/a"]
    );
    let matches = select(&doc, &[], "$..*").unwrap();
    assert_eq!(
        matches.iter().map(|m| pointer(&m.path)).collect::<Vec<_>>(),
        ["/z", "/z/a~1b", "/z/~0", "/a", "/a/0", "/a/1"]
    );
}

#[test]
fn rfc_selectors_and_slices() {
    let doc = parse("[0,1,2,3,4,5,6]").unwrap();
    for (source, expected) in [
        ("$", "[[0,1,2,3,4,5,6]]"),
        ("$[2,-1,2]", "[2,6]"),
        ("$[1:6:2]", "[1,3,5]"),
        ("$[::-1]", "[0,1,2,3,4,5,6]"),
        ("$[5:1:-2]", "[3,5]"),
        ("$[:-3]", "[0,1,2,3]"),
        ("$[-3:]", "[4,5,6]"),
        ("$[::0]", "[]"),
        ("$[99]", "[]"),
        ("$[-99]", "[]"),
        ("$[-99:99]", "[0,1,2,3,4,5,6]"),
        ("$[99:-99:-1]", "[0,1,2,3,4,5,6]"),
    ] {
        assert_eq!(
            Value::Array(query(&doc, source).unwrap()),
            parse(expected).unwrap(),
            "{source}"
        );
    }
    assert!(query(&parse("[]").unwrap(), "$[::-1]").unwrap().is_empty());
}

#[test]
fn filters_and_standard_functions() {
    let doc = parse(
        r#"[
        {"name":"Abc","n":1,"values":[1,2],"flag":false},
        {"name":"bc","n":2,"values":[3]},
        {"name":"𝄞","n":3,"values":[]},
        {"name":null,"n":4}
    ]"#,
    )
    .unwrap();
    for (source, expected) in [
        ("$[?@.n >= 2 && @.n < 4].n", "[2,3]"),
        ("$[?(@.n == 1 || @.n == 4)].n", "[1,4]"),
        ("$[?!(@.n == 1)].n", "[2,3,4]"),
        ("$[?@.flag].n", "[1]"),
        ("$[?!@.flag].n", "[2,3,4]"),
        ("$[?length(@.values) == 2].n", "[1]"),
        ("$[?count(@.values[*]) == 1].n", "[2]"),
        ("$[?value(@.values[*]) == 3].n", "[2]"),
        ("$[?length(@.name) == 1].n", "[3]"),
        ("$[?match(@.name, '[A-Z].*')].n", "[1]"),
        ("$[?search(@.name, 'bc')].n", "[1,2]"),
        ("$[?!search(@.name, 'bc')].n", "[3,4]"),
        ("$[?@.values[-1] == 3].n", "[2]"),
        ("$[?@.missing == @.absent].n", "[1,2,3,4]"),
        ("$[?@.missing == null].n", "[]"),
        ("$[?@.values > @.values].n", "[]"),
        ("$[?@.name <= null].n", "[4]"),
    ] {
        assert_eq!(
            Value::Array(query(&doc, source).unwrap()),
            parse(expected).unwrap(),
            "{source}"
        );
    }
}

#[test]
fn numbers_and_compound_values_are_compared_exactly() {
    let doc = parse(
        r#"[
      {"n":9007199254740992,"v":[1.0,{"a":2,"b":3}]},
      {"n":9007199254740993,"v":[1,{"b":3,"a":2.0}]},
      {"n":1e9999999999999999999999,"v":null},
      {"n":0.00001,"v":null}
    ]"#,
    )
    .unwrap();
    for (source, expected) in [
        ("$[?@.n == 9007199254740993].n", "[9007199254740993]"),
        ("$[?@.n > 9007199254740993].n", "[1e9999999999999999999999]"),
        ("$[?@.v == $[0].v].n", "[9007199254740992,9007199254740993]"),
        ("$[?@.n < 1].n", "[0.00001]"),
    ] {
        assert_eq!(
            Value::Array(query(&doc, source).unwrap()),
            parse(expected).unwrap(),
            "{source}"
        );
    }
}

#[test]
fn strings_and_subtree_roots() {
    let doc =
        Document::new(parse(r#"{"items":[{"v":1},{"v":2}],"a'b":3,"a\"b":4,"𝄞":5}"#).unwrap());
    for (source, expected) in [
        (r#"$['a\'b']"#, "3"),
        (r#"$['a"b']"#, "4"),
        (r#"$['\uD834\uDD1E']"#, "5"),
        ("$.𝄞", "5"),
    ] {
        assert_eq!(
            select(&doc, &[], source).unwrap()[0].value,
            parse(expected).unwrap()
        );
    }
    let selected = select(&doc, &["items".into()], "$[?@.v == $[1].v]").unwrap();
    assert_eq!(selected[0].path, ["items", "1"]);
    assert!(select(&doc, &["missing".into()], "$").is_err());
    assert!(select(&doc, &[], "$[?length(@)]").is_err());
}

#[test]
fn count_retains_duplicate_nodes_inside_filters() {
    let doc = parse(r#"[{"a":1}]"#).unwrap();
    assert_eq!(
        query(&doc, "$[?count(@['a','a']) == 2]").unwrap(),
        query(&doc, "$[*]").unwrap()
    );
}
