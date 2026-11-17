use std::{io::Read, sync::Arc, thread};
use tiny_http::{Header, Response, Server};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args().skip(1);
    let mut address = String::from("127.0.0.1:3000");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => address = args.next().ok_or("--listen requires an address")?,
            "--help" | "-h" => {
                println!(
                    "BetterQuery document explorer\n\nUsage: bp-web [--listen 127.0.0.1:3000]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let server = Arc::new(Server::http(&address)?);
    println!("BetterQuery explorer: http://{}", server.server_addr());
    let workers: Vec<_> = (0..4).map(|_| {
        let server = server.clone();
        thread::spawn(move || {
            for mut request in server.incoming_requests() {
                let content_type = request.headers().iter().find(|h| h.field.equiv("Content-Type")).map(|h| h.value.as_str().to_owned());
                let mut body = Vec::new();
                let read = request.as_reader().take(bp_web::MAX_BODY_BYTES as u64 + 1).read_to_end(&mut body);
                let response = if read.is_err() {
                    bp_web::Response { status: 400, html: "Could not read request body".into() }
                } else { bp_web::handle(request.method().as_str(),request.url(),content_type.as_deref(),&body) };
                let mut response = Response::from_string(response.html).with_status_code(response.status);
                for (name,value) in [
                    ("Content-Type","text/html; charset=utf-8"),
                    ("Cache-Control","no-store"),
                    ("X-Content-Type-Options","nosniff"),
                    ("Content-Security-Policy","default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'"),
                ] { response.add_header(Header::from_bytes(name,value).unwrap()); }
                if response.status_code().0 == 405 { response.add_header(Header::from_bytes("Allow","GET, POST").unwrap()); }
                if let Err(error) = request.respond(response) { eprintln!("Response failed: {error}"); }
            }
        })
    }).collect();
    for worker in workers {
        worker
            .join()
            .map_err(|_| "HTTP worker stopped unexpectedly")?;
    }
    Ok(())
}
