use bq_json::{Value, parse, to_string};

fn round_trip(source: &str) -> Value {
    let value = parse(source).unwrap();
    let output = to_string(&value).unwrap();
    assert_eq!(parse(&output).unwrap(), value, "{source}");
    assert_eq!(to_string(&parse(&output).unwrap()).unwrap(), output);
    value
}

#[test]
fn every_value_kind_round_trips() {
    for source in [
        "null",
        "true",
        "false",
        "0",
        "-0",
        "123",
        "-123.45e+6",
        r#""""#,
        r#""hello""#,
        "[]",
        "{}",
        "[1,true,null,{}]",
        r#"{"b": [false, {"z": "𝄞", "": null}], "a": {}}"#,
        " \r\n\t[1, 2, 3] \r\n\t",
    ] {
        round_trip(source);
    }
}

#[test]
fn exact_number_text_has_no_numeric_range_limit() {
    for number in [
        "0",
        "-0",
        "-0.000",
        "1.230000",
        "1E+00042",
        "1e-00042",
        "99999999999999999999999999999999999999999999999999",
        "1E+99999999999999999999999999999999999999",
        "-1e-99999999999999999999999999999999999999",
    ] {
        assert_eq!(round_trip(number), Value::Number(number.into()));
        assert_eq!(to_string(&parse(number).unwrap()).unwrap(), number);
    }
    let giant = format!(
        "{}.{}e+{}",
        "9".repeat(10_000),
        "0".repeat(10_000),
        "9".repeat(10_000)
    );
    assert_eq!(to_string(&parse(&giant).unwrap()).unwrap(), giant);
}

#[test]
fn ordering_is_retained_including_after_editing() {
    let mut value = round_trip(r#"{"z":0,"a":1,"Z":2,"":3}"#);
    let Value::Object(ref mut members) = value else {
        panic!()
    };
    members.push(("new2".into(), Value::Bool(true)));
    members.push(("new1".into(), Value::Null));
    assert_eq!(
        to_string(&value).unwrap(),
        r#"{"z":0,"a":1,"Z":2,"":3,"new2":true,"new1":null}"#
    );
    assert_eq!(parse(&to_string(&value).unwrap()).unwrap(), value);
}

#[test]
fn unicode_and_escapes_round_trip_semantically() {
    assert_eq!(
        round_trip(r#""\"\\\/\b\f\n\r\t\u0000\u0041\uD834\uDD1E""#),
        Value::String("\"\\/\u{8}\u{c}\n\r\t\0A𝄞".into())
    );
    for text in ["é", "e\u{301}", "𝄞", "\u{10ffff}", "\u{2028}", "\u{feff}"] {
        let value = Value::String(text.into());
        assert_eq!(parse(&to_string(&value).unwrap()).unwrap(), value);
    }
    // All raw controls must be escaped by the serializer.
    let controls: String = (0..=31).map(char::from).collect();
    let value = Value::String(controls);
    assert_eq!(parse(&to_string(&value).unwrap()).unwrap(), value);
}

#[test]
fn duplicate_decoded_keys_are_rejected_at_every_depth() {
    for source in [
        r#"{"a":1,"a":2}"#,
        r#"{"a":1,"\u0061":2}"#,
        r#"{"𝄞":1,"\ud834\udd1e":2}"#,
        r#"[{"nested":{"":1,"":2}}]"#,
    ] {
        assert!(
            parse(source)
                .unwrap_err()
                .message
                .contains("duplicate object key")
        );
    }
    round_trip(r#"{"a":1,"A":2,"é":3,"e\u0301":4}"#);
    round_trip(r#"[{"a":1},{"a":2}]"#);
}

#[test]
fn invalid_json_is_rejected() {
    for source in [
        "",
        " ",
        "NULL",
        "True",
        "FALSE",
        "undefined",
        "NaN",
        "Infinity",
        "01",
        "-01",
        "+1",
        ".1",
        "1.",
        "1e",
        "1e+",
        "--1",
        "1 2",
        "truefalse",
        "null1",
        "[01]",
        "[1-2]",
        "[1 2]",
        "[1,]",
        "[,1]",
        "[",
        "{",
        "{a:1}",
        r#"{"a":1,}"#,
        r#"{"a" 1}"#,
        r#"{"a":1 "b":2}"#,
        "{}{}",
        "[]x",
        "// comment\n1",
        "/*comment*/1",
        "\u{feff}null",
        "\u{a0}null",
        "[1,\u{b}2]",
        r#""\uD800""#,
        r#""\uDC00""#,
        r#""\uD800\u0041""#,
        r#""\uD800\uD800""#,
        r#""\uZZZZ""#,
        r#""\x41""#,
        r#""\U00000041""#,
        "\"a\nb\"",
        "\"\0\"",
        "\"unterminated",
    ] {
        assert!(parse(source).is_err(), "accepted {source:?}");
    }
}

#[test]
fn serialization_rejects_invalid_constructed_ir() {
    for text in ["", "NaN", "null", "[]", "01", "1.", " 1", "1 ", "+1", "1 2"] {
        assert!(to_string(&Value::Number(text.into())).is_err(), "{text}");
    }
    let value = Value::Array(vec![Value::Object(vec![
        ("a".into(), Value::Null),
        ("a".into(), Value::Bool(true)),
    ])]);
    let error = to_string(&value).unwrap_err();
    assert_eq!(error.path, ["0", "a"]);
    assert!(error.message.contains("duplicate"));
    let value = Value::Object(vec![(
        "a".into(),
        Value::Array(vec![Value::Number("bad".into())]),
    )]);
    assert_eq!(to_string(&value).unwrap_err().path, ["a", "0"]);
}

#[test]
fn errors_have_utf8_safe_locations() {
    let source = "{\"é\": 1,\n  !}";
    let error = parse(source).unwrap_err();
    assert_eq!(error.offset, source.find('!').unwrap());
    assert_eq!((error.line, error.column), (2, 3));
    assert!(error.to_string().starts_with("2:3:"));
    assert!(parse("\"\\é\"").is_err());
}

#[test]
fn truncation_never_panics() {
    let source = r#"{"é": ["𝄞\uD834\uDD1E\"\\", -12.30e+42, true, false, null, {}]}"#;
    round_trip(source);
    for (end, _) in source.char_indices() {
        let _ = parse(&source[..end]);
    }
}

#[test]
fn nesting_limit_is_symmetric_for_parsing_and_serializing() {
    let source = format!("{}null{}", "[".repeat(127), "]".repeat(127));
    round_trip(&source);
    let mut value = parse(&source).unwrap();
    value = Value::Array(vec![value]);
    assert!(
        to_string(&value)
            .unwrap_err()
            .message
            .contains("nesting limit")
    );
    let source = format!("{}null{}", "[".repeat(128), "]".repeat(128));
    assert!(
        parse(&source)
            .unwrap_err()
            .message
            .contains("nesting limit")
    );
}

#[test]
fn generated_values_round_trip() {
    fn next(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        *seed
    }
    fn generate(seed: &mut u64, depth: usize) -> Value {
        let kind = next(seed) % if depth == 0 { 4 } else { 6 };
        match kind {
            0 => Value::Null,
            1 => Value::Bool(next(seed) & 1 == 0),
            2 => Value::Number(format!("-{}.000E+{}", next(seed), next(seed))),
            3 => Value::String(format!("é𝄞\0\n\"\\{}", next(seed))),
            4 => Value::Array(
                (0..next(seed) % 5)
                    .map(|_| generate(seed, depth - 1))
                    .collect(),
            ),
            _ => Value::Object(
                (0..next(seed) % 5)
                    .map(|i| (format!("key{i}é"), generate(seed, depth - 1)))
                    .collect(),
            ),
        }
    }
    let mut seed = 42;
    for _ in 0..1000 {
        let value = generate(&mut seed, 5);
        let output = to_string(&value).unwrap();
        assert_eq!(parse(&output).unwrap(), value);
    }
}
