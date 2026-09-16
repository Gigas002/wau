//! Shells out to the system `gh` CLI for GitHub API requests, instead of
//! `wau` negotiating or storing its own GitHub credentials — auth is
//! whatever `gh auth login` has already configured on the host.

#[cfg(test)]
mod tests;

#[cfg(not(test))]
use crate::results::InternalError;
use crate::{
    http::{HttpClient, HttpResponse},
    results::AnyOutcome,
};

/// Fetches `url` (with extra request `headers`) the way [`HttpClient::get`]
/// used to. Overridable only in tests (mockito needs a real HTTP client to
/// mock against — `gh` itself can't be pointed at a local server), so the
/// real flow always shells out to `gh api`.
pub(super) async fn get(
    http: &HttpClient,
    url: &str,
    headers: &[(&str, &str)],
) -> AnyOutcome<HttpResponse> {
    #[cfg(test)]
    return Ok(http.get(url, headers).await?);
    #[cfg(not(test))]
    {
        let _ = http;
        run_gh_api(url, headers).await
    }
}

/// Runs `gh api <url> -i [-H k:v ...]`. `-i` makes `gh` print the response's
/// status line and headers ahead of the body, which [`parse_response`] then
/// splits back into an [`HttpResponse`] shaped the same as the direct-HTTP
/// path always returned — `gh` exits non-zero on non-2xx responses, but
/// still writes the full response to stdout with `-i`, so the exit code
/// itself is ignored; callers inspect `status` like they always have.
#[cfg(not(test))]
async fn run_gh_api(url: &str, headers: &[(&str, &str)]) -> AnyOutcome<HttpResponse> {
    let mut cmd = tokio::process::Command::new("gh");
    cmd.arg("api").arg(url).arg("-i");
    for (k, v) in headers {
        cmd.arg("-H").arg(format!("{k}: {v}"));
    }

    let output = cmd.output().await.map_err(InternalError::new)?;

    parse_response(&output.stdout).ok_or_else(|| {
        InternalError::new(format!(
            "unparseable `gh api` response for {url}: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into()
    })
}

/// Parses `gh api -i`'s output: a status line, headers, a blank line, then
/// the raw body. `gh` prints the status line terminated by a bare `\n` but
/// every header line (and the header/body separator) with `\r\n` — `str`'s
/// `lines()` handles both uniformly, since it strips an optional trailing
/// `\r` regardless.
fn parse_response(raw: &[u8]) -> Option<HttpResponse> {
    const SEP: &[u8] = b"\r\n\r\n";
    let split_at = raw.windows(SEP.len()).position(|w| w == SEP)?;
    let head = String::from_utf8_lossy(&raw[..split_at]).into_owned();
    let body = raw[split_at + SEP.len()..].to_vec();

    let mut lines = head.lines();
    let status: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers = lines
        .filter_map(|line| {
            let (k, v) = line.split_once(':')?;
            Some((k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect();

    Some(HttpResponse {
        status,
        headers,
        body,
    })
}
