use bq_web::handle;

fn submit(program: &str, document: &str) -> bq_web::Response {
    fn encode(value: &str) -> String {
        value.bytes().map(|b| format!("%{b:02X}")).collect()
    }
    let body = format!("program={}&document={}", encode(program), encode(document));
    handle(
        "POST",
        "/",
        Some("application/x-www-form-urlencoded; charset=UTF-8"),
        body.as_bytes(),
    )
}

#[test]
fn page_and_examples_are_accessible_without_scripts() {
    for url in [
        "/",
        "/?example=query",
        "/?example=merge",
        "/?example=array",
        "/?example=rollback",
    ] {
        let page = handle("GET", url, None, &[]);
        assert_eq!(page.status, 200);
        assert!(page.html.contains("method=\"post\""));
        assert!(page.html.contains("label for=\"program\""));
        assert!(!page.html.contains("<script"));
    }
}

#[test]
fn success_renders_matches_source_and_ordered_result() {
    let page = submit(
        r#"(query "$.a[*]" (merge 1.2300e4))"#,
        r#"{"z":0,"a":[1,2]}"#,
    );
    assert_eq!(page.status, 200);
    for text in [
        "Program completed",
        "Execution timeline",
        "2 selected nodes",
        "/a/0",
        "/a/1",
        "Transformation source",
        "Document before &amp; after",
        "1.2300e4",
        "COMMITTED",
    ] {
        assert!(page.html.contains(text), "{text}");
    }
    let result = page.html.split("result-code\">").nth(1).unwrap();
    assert!(result.find("&quot;z&quot;").unwrap() < result.find("&quot;a&quot;").unwrap());
}

#[test]
fn failures_show_rollback_and_last_valid_transform() {
    let page = submit(
        r#"(query "$..*" (merge (object (b . 1))))"#,
        r#"{"a":{"b":{}}}"#,
    );
    assert_eq!(page.status, 422);
    for text in [
        "Program rolled back",
        "Target last valid before this transformation",
        "RESTORED",
        "/a/b",
    ] {
        assert!(page.html.contains(text), "{text}");
    }
    assert_eq!(submit("(merge)", "{}").status, 422);
    assert_eq!(submit("(merge 1)", "invalid").status, 422);
}

#[test]
fn user_input_is_escaped_everywhere() {
    let page = submit(r#"(merge "</textarea><script>alert('x')</script>")"#, "{}");
    assert_eq!(page.status, 200);
    assert!(!page.html.contains("<script>"));
    assert!(page.html.contains("&lt;script&gt;"));
    let page = submit("</textarea><script>bad</script>", "{}");
    assert!(!page.html.contains("<script>"));
}

#[test]
fn request_validation() {
    assert_eq!(handle("GET", "/missing", None, &[]).status, 404);
    assert_eq!(handle("PUT", "/", None, &[]).status, 405);
    assert_eq!(
        handle("POST", "/", Some("application/json"), b"{}").status,
        415
    );
    for body in [
        b"program=%GG&document=null".as_slice(),
        b"program=%FF&document=null",
        b"program=%&document=null",
        b"program=&program=&document=null",
        b"program=",
    ] {
        assert_eq!(
            handle("POST", "/", Some("application/x-www-form-urlencoded"), body).status,
            400
        );
    }
    assert_eq!(
        handle(
            "POST",
            "/",
            Some("application/x-www-form-urlencoded"),
            &vec![0; bq_web::MAX_BODY_BYTES + 1]
        )
        .status,
        413
    );
}

#[test]
fn empty_program_and_removed_document_are_distinct_from_null() {
    assert!(submit("", "null").html.contains("empty program"));
    assert!(
        submit(r#"(patch remove (path ""))"#, "null")
            .html
            .contains("Document root removed")
    );
}
