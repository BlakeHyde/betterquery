//! Server-rendered BetterQuery explorer; the form works without JavaScript.
mod render;

pub use render::render;

pub const MAX_BODY_BYTES: usize = 1024 * 1024;
pub const SAMPLE_DOCUMENT: &str = "{\n  \"a\": {\"sharedKey\": true, \"uniqueA\": 1},\n  \"b\": {\"sharedKey\": true, \"uniqueB\": 2},\n  \"c\": {\"sharedKey\": false}\n}";
pub const SAMPLE_PROGRAM: &str =
    "(query \"$..[?(@.sharedKey == true)]\"\n  (patch remove (path \"/sharedKey\")))";

pub struct Response {
    pub status: u16,
    pub html: String,
}

/// Route an HTTP request independently of the network transport.
pub fn handle(method: &str, url: &str, content_type: Option<&str>, body: &[u8]) -> Response {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    if path != "/" {
        return plain_error(404, "Page not found");
    }
    match method {
        "GET" => {
            let fields = match decode_form(query.as_bytes()) {
                Ok(fields) => fields,
                Err(error) => return plain_error(400, &error),
            };
            let example = fields
                .iter()
                .find(|(key, _)| key == "example")
                .map(|(_, value)| value.as_str())
                .unwrap_or("query");
            let (program, document) = match example {
                "query" => (SAMPLE_PROGRAM, SAMPLE_DOCUMENT),
                "merge" => (
                    "(merge (object\n  (title . \"Hello!\")\n  (author . (object (familyName . NULL)))\n  (tags . (array \"example\"))))",
                    "{\n  \"title\": \"Goodbye!\",\n  \"author\": {\"givenName\": \"John\", \"familyName\": \"Doe\"},\n  \"tags\": [\"example\", \"sample\"]\n}",
                ),
                "array" => (
                    "(query \"$[1,3]\"\n  (patch remove (path \"\")))",
                    "[\"a\", \"b\", \"c\", \"d\", \"e\"]",
                ),
                "rollback" => (
                    "(query \"$..*\"\n  (merge (object (b . 1))))",
                    "{\"a\": {\"b\": {}}}",
                ),
                _ => return plain_error(404, "Example not found"),
            };
            Response {
                status: 200,
                html: render(program, document, None),
            }
        }
        "POST" => {
            if body.len() > MAX_BODY_BYTES {
                return plain_error(413, "Input exceeds the 1 MiB request limit");
            }
            if content_type
                .and_then(|v| v.split(';').next())
                .map(str::trim)
                != Some("application/x-www-form-urlencoded")
            {
                return plain_error(415, "Expected a form submission");
            }
            let fields = match decode_form(body) {
                Ok(fields) => fields,
                Err(error) => return plain_error(400, &error),
            };
            let value = |name| {
                fields
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.as_str())
            };
            let (Some(program), Some(document)) = (value("program"), value("document")) else {
                return plain_error(400, "Both program and document fields are required");
            };
            let result = bq_timeline::evaluate(program, document);
            let status = match &result {
                Ok(timeline) if timeline.succeeded() => 200,
                _ => 422,
            };
            Response {
                status,
                html: render(program, document, Some(&result)),
            }
        }
        _ => plain_error(405, "Method not allowed"),
    }
}

fn plain_error(status: u16, message: &str) -> Response {
    let result = Err(bq_timeline::InputError::Document(
        bq_document_parser::Error {
            offset: 0,
            line: 1,
            column: 1,
            message: message.into(),
        },
    ));
    Response {
        status,
        html: render("", "", Some(&result)),
    }
}

fn decode_form(body: &[u8]) -> Result<Vec<(String, String)>, String> {
    fn decode(bytes: &[u8]) -> Result<String, String> {
        let mut decoded = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => decoded.push(b' '),
                b'%' => {
                    let hex = bytes.get(i + 1..i + 3).ok_or("incomplete percent escape")?;
                    let number = |b: u8| (b as char).to_digit(16).map(|v| v as u8);
                    let (Some(a), Some(b)) = (number(hex[0]), number(hex[1])) else {
                        return Err("invalid percent escape".into());
                    };
                    decoded.push(a * 16 + b);
                    i += 2;
                }
                b => decoded.push(b),
            }
            i += 1;
        }
        String::from_utf8(decoded).map_err(|_| "form must be valid UTF-8".into())
    }
    let mut fields = Vec::new();
    for field in body.split(|b| *b == b'&').filter(|s| !s.is_empty()) {
        let separator = field.iter().position(|b| *b == b'=').unwrap_or(field.len());
        let key = decode(&field[..separator])?;
        if fields.iter().any(|(existing, _)| existing == &key) {
            return Err(format!("duplicate form field {key:?}"));
        }
        let value = decode(field.get(separator + 1..).unwrap_or_default())?;
        fields.push((key, value));
    }
    Ok(fields)
}
