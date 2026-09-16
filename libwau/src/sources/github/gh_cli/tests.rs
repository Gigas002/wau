use super::*;

#[test]
fn parses_status_headers_and_body() {
    let raw = b"HTTP/2.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"a\":1}";
    let resp = parse_response(raw).unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(
        resp.headers,
        vec![("Content-Type".to_owned(), "application/json".to_owned())]
    );
    assert_eq!(resp.body, b"{\"a\":1}");
}

#[test]
fn parses_status_line_terminated_by_bare_lf() {
    // gh prints the status line with a bare `\n`; every header after it
    // (and the header/body separator) uses `\r\n` — exactly what a real
    // `gh api -i` invocation produces.
    let raw = b"HTTP/2.0 404 Not Found\nX-Foo: bar\r\n\r\nnot found";
    let resp = parse_response(raw).unwrap();
    assert_eq!(resp.status, 404);
    assert_eq!(resp.headers, vec![("X-Foo".to_owned(), "bar".to_owned())]);
    assert_eq!(resp.body, b"not found");
}

#[test]
fn binary_body_after_the_boundary_is_preserved_verbatim() {
    let mut raw = b"HTTP/2.0 200 OK\r\n\r\n".to_vec();
    raw.extend_from_slice(&[0u8, 1, 2, 3, 255]);
    let resp = parse_response(&raw).unwrap();
    assert_eq!(resp.body, vec![0u8, 1, 2, 3, 255]);
}

#[test]
fn missing_header_body_separator_returns_none() {
    assert!(parse_response(b"garbage, no CRLFCRLF here").is_none());
}
