use bq_parser::{FormKind, Patch, Value, parse};

fn merge(source: &str) -> Value {
    let mut forms = parse(source).unwrap();
    assert_eq!(forms.len(), 1);
    match forms.remove(0).kind {
        FormKind::Merge(value) => value,
        other => panic!("expected merge, got {other:?}"),
    }
}

#[test]
fn empty_program_and_form_spans() {
    assert!(parse(" \n\t\r").unwrap().is_empty());
    let source = "  (merge NULL)\n(patch remove(path \"\")) ";
    let forms = parse(source).unwrap();
    assert_eq!(&source[forms[0].span.clone()], "(merge NULL)");
    assert_eq!(&source[forms[1].span.clone()], "(patch remove(path \"\"))");
}

#[test]
fn all_patch_operations_and_field_orders() {
    let forms = parse(
        r#"
        (PATCH add (value 1) (path "/a"))
        (patch REMOVE (path ""))
        (Patch Replace (path "/a") (value false))
        (patch move (from "/a") (path "/b"))
        (patch COPY (path "/c") (from "/b"))
        (patch test (value NULL) (path "/c"))
    "#,
    )
    .unwrap();
    let patches: Vec<_> = forms
        .into_iter()
        .map(|f| match f.kind {
            FormKind::Patch(patch) => patch,
            _ => panic!(),
        })
        .collect();
    assert_eq!(
        patches,
        vec![
            Patch::Add {
                path: "/a".into(),
                value: Value::Number("1".into())
            },
            Patch::Remove { path: "".into() },
            Patch::Replace {
                path: "/a".into(),
                value: Value::Bool(false)
            },
            Patch::Move {
                path: "/b".into(),
                from: "/a".into()
            },
            Patch::Copy {
                path: "/c".into(),
                from: "/b".into()
            },
            Patch::Test {
                path: "/c".into(),
                value: Value::Null
            },
        ]
    );
    for op in ["add", "replace", "test"] {
        for fields in ["(path \"\") (value 2)", "(value 2) (path \"\")"] {
            parse(&format!("(patch {op} {fields})")).unwrap();
        }
    }
    for op in ["move", "copy"] {
        for fields in ["(path \"\") (from \"/a\")", "(from \"/a\") (path \"\")"] {
            parse(&format!("(patch {op} {fields})")).unwrap();
        }
    }
}

#[test]
fn constructors_preserve_keys_order_and_unicode() {
    assert_eq!(
        merge(
            r#"(mErGe (ObJeCt
        (NULL . (array TRUE false null))
        (Case_Sensitive-2 . "\uD834\uDD1E")
        ("" . "é") ("spaced key" . (object))))"#
        ),
        Value::Object(vec![
            (
                "NULL".into(),
                Value::Array(vec![Value::Bool(true), Value::Bool(false), Value::Null])
            ),
            ("Case_Sensitive-2".into(), Value::String("𝄞".into())),
            ("".into(), Value::String("é".into())),
            ("spaced key".into(), Value::Object(vec![])),
        ])
    );
}

#[test]
fn numbers_are_unbounded_and_lossless_in_both_modes() {
    for number in [
        "0",
        "-0",
        "-123",
        "1.2300",
        "1E+999999999999999999999",
        "1e-999999",
        "99999999999999999999999999999999999999",
    ] {
        let expected = Value::Number(number.into());
        assert_eq!(merge(&format!("(merge {number})")), expected);
        assert_eq!(merge(&format!("(merge (JSON {number} END))")), expected);
    }
}

#[test]
fn embedded_json_is_available_in_every_value_position() {
    let source = r#"(merge (object
        (a . (JSON {"z": [1, true, null, "END)\""], "a": {}} END))
        (b . (array (JSON 42 END) (JSON "END" END)))) )"#;
    parse(source).unwrap();
    parse(r#"(patch add (path "") (value (JSON [false] END)))"#).unwrap();
    assert_eq!(
        merge(r#"(merge (JSON {"b":1,"a":2} END))"#),
        Value::Object(vec![
            ("b".into(), Value::Number("1".into())),
            ("a".into(), Value::Number("2".into()))
        ])
    );
}

#[test]
fn nested_queries_and_rfc9535_selectors() {
    for query in [
        "$",
        "$..[?(@.sharedKey == true)]",
        "$['a','b']",
        "$[1:5:2]",
        "$..*",
        "$[?length(@.a) > 2]",
        "$[?match(@.name, 'a.*')]",
    ] {
        let quoted = serde_json::to_string(query).unwrap();
        parse(&format!("(query {quoted} (query \"$\" (merge null)))")).unwrap();
    }
    for query in ["", "@.a", "$.a[", "$[?@.a + 1]", "$[?unknown(@)]"] {
        let quoted = serde_json::to_string(query).unwrap();
        assert!(
            parse(&format!("(query {quoted} (merge null))")).is_err(),
            "{query}"
        );
    }
}

#[test]
fn pointers_validate_decoded_text_not_target_existence() {
    for pointer in [
        "",
        "/",
        "/a~1b/~0/0/-",
        "/~01",
        "/é",
        "/999999999999999999999",
        "/00",
    ] {
        let quoted = serde_json::to_string(pointer).unwrap();
        parse(&format!("(patch remove (path {quoted}))")).unwrap();
    }
    parse(r#"(patch remove (path "\u002fa"))"#).unwrap();
    for pointer in ["a", "#/a", "/~", "/~2", "/a~x"] {
        let quoted = serde_json::to_string(pointer).unwrap();
        assert!(parse(&format!("(patch remove (path {quoted}))")).is_err());
        assert!(parse(&format!("(patch copy (path \"\") (from {quoted}))")).is_err());
    }
}

#[test]
fn duplicate_decoded_keys_rejected_in_both_modes() {
    for source in [
        r#"(merge (object (a . 1) (a . 2)))"#,
        r#"(merge (object (a . 1) ("\u0061" . 2)))"#,
        r#"(merge (JSON {"a":1,"\u0061":2} END))"#,
        r#"(merge (JSON {"x":{"a":1,"a":2}} END))"#,
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains("duplicate object key"), "{error}");
    }
    parse(r#"(merge (object (a . 1) (A . 2)))"#).unwrap();
}

#[test]
fn malformed_syntax_is_rejected() {
    for source in [
        "garbage",
        "()",
        "(patches)",
        "(merge)",
        "(merge null null)",
        "(merge null))",
        "(merge null",
        "(merge TRUEfalse)",
        "(merge null1)",
        "(merge (array 1true))",
        "(merge (array 1-2))",
        "(merge (array 01))",
        "(merge 'x)",
        "; comment",
        "(merge (object (a 1)))",
        "(merge (object (1 . 2)))",
        "(merge (object (a.b . 2)))",
        "(merge (patch remove (path \"\")))",
        "(merge (query \"$\" (merge null)))",
        "(query \"$\")",
        "(query \"$\" (merge 1) (merge 2))",
        "(query \"$\" 1)",
        "(patch nope)",
        "(patch add (path \"\"))",
        "(patch add (value 1))",
        "(patch remove (path \"\") (value 1))",
        "(patch move (path \"\"))",
        "(patch copy (path \"\") (from \"\") (from \"\"))",
        "(patch test (path \"\") (path \"\") (value 1))",
        "(patch remove (PATHS \"\"))",
        "(merge (array",
        "(merge (object",
        "(merge (JSON 1 END)",
    ] {
        assert!(parse(source).is_err(), "accepted {source}");
    }
}

#[test]
fn malformed_json_numbers_and_strings_rejected() {
    for value in [
        "01", "-01", "+1", ".5", "1.", "1e", "1e+", "--1", "NaN", "Infinity",
    ] {
        for source in [
            format!("(merge {value})"),
            format!("(merge (JSON {value} END))"),
        ] {
            assert!(parse(&source).is_err(), "accepted {source}");
        }
    }
    for string in [
        r#""\uD800""#,
        r#""\uDC00""#,
        r#""\uD800\u0041""#,
        r#""\x20""#,
        r#""\uZZZZ""#,
        "\"raw\nnewline\"",
        "\"\0\"",
        "\"unterminated",
    ] {
        for source in [
            format!("(merge {string})"),
            format!("(merge (JSON {string} END))"),
        ] {
            assert!(parse(&source).is_err(), "accepted {source:?}");
        }
    }
    for value in [
        "TRUE",
        "Null",
        "[1,]",
        r#"{"a":1,}"#,
        "[1 2]",
        "{} {}",
        "1 2",
        "(array)",
        r#"{"a": NULL}"#,
        "[1,\u{a0}2]",
    ] {
        let source = format!("(merge (JSON {value} END))");
        assert!(parse(&source).is_err(), "accepted {source}");
    }
}

#[test]
fn errors_report_utf8_safe_locations() {
    let source = "(merge \"é\")\n  !";
    let error = parse(source).unwrap_err();
    assert_eq!(error.offset, source.find('!').unwrap());
    assert_eq!((error.line, error.column), (2, 3));
    assert!(error.to_string().starts_with("2:3:"));
    assert!(parse("(merge \"\\é\")").is_err());
}

#[test]
fn every_truncated_prefix_returns_without_panicking() {
    for source in [
        r#"(query "$..[?(@.a == true)]" (patch add (path "/é") (value (object (key . "𝄞\n\uD834\uDD1E")))))"#,
        r#"(merge (JSON {"é": [1.2e-30, null, "END\\\""]} END))"#,
    ] {
        parse(source).unwrap();
        for (end, _) in source.char_indices() {
            let _ = parse(&source[..end]);
        }
    }
}

#[test]
fn string_escapes_decode_identically_in_both_modes() {
    let string = r#""\"\\\/\b\f\n\r\t\u0041""#;
    let expected = Value::String("\"\\/\u{8}\u{c}\n\r\tA".into());
    assert_eq!(merge(&format!("(merge {string})")), expected);
    assert_eq!(merge(&format!("(merge (JSON {string} END))")), expected);
}

#[test]
fn query_values_share_the_json_ir_and_round_trip() {
    let source = r#"(merge (object (z . 1.2300E+99999) (a . (array "𝄞" NULL))))"#;
    let value: bq_json::Value = merge(source);
    let json = bq_json::to_string(&value).unwrap();
    assert_eq!(json, r#"{"z":1.2300E+99999,"a":["𝄞",null]}"#);
    assert_eq!(bq_json::parse(&json).unwrap(), value);
    assert_eq!(merge(&format!("(merge (JSON {json} END))")), value);
}

#[test]
fn excessive_nesting_is_a_recoverable_error() {
    for source in [
        format!("(merge {}null{})", "(array ".repeat(150), ")".repeat(150)),
        format!(
            "{}(merge null){}",
            "(query \"$\" ".repeat(150),
            ")".repeat(150)
        ),
        format!("(merge (JSON {}0{} END))", "[".repeat(150), "]".repeat(150)),
    ] {
        assert!(
            parse(&source)
                .unwrap_err()
                .message
                .contains("nesting limit")
        );
    }
}
