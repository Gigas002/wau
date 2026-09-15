use std::time::Duration;

use super::*;
use cache::Cache;

// ---------------------------------------------------------------------------
// Cache store
// ---------------------------------------------------------------------------

fn sample_response(body: &str) -> CachedResponse {
    CachedResponse {
        status: 200,
        headers: vec![("content-type".to_owned(), "text/plain".to_owned())],
        body: body.as_bytes().to_vec(),
    }
}

#[test]
fn cache_miss_on_unknown_key() {
    let cache = Cache::open_in_memory().unwrap();
    assert!(cache.get("missing").is_none());
}

#[test]
fn cache_put_then_get_round_trips() {
    let cache = Cache::open_in_memory().unwrap();
    let response = sample_response("hello");
    cache.put("key", &response, CacheTtl::Indefinite);

    let hit = cache.get("key").unwrap();
    assert_eq!(hit, response);
}

#[test]
fn cache_never_does_not_store() {
    let cache = Cache::open_in_memory().unwrap();
    cache.put("key", &sample_response("hello"), CacheTtl::Never);
    assert!(cache.get("key").is_none());
}

#[test]
fn cache_entry_expires_after_ttl() {
    let cache = Cache::open_in_memory().unwrap();
    cache.put(
        "key",
        &sample_response("hello"),
        CacheTtl::For(Duration::from_millis(0)),
    );
    // TTL of 0 means "expires now or in the past" — already stale on read.
    std::thread::sleep(Duration::from_millis(5));
    assert!(cache.get("key").is_none());
}

#[test]
fn cache_clear_removes_all_entries() {
    let cache = Cache::open_in_memory().unwrap();
    cache.put("a", &sample_response("1"), CacheTtl::Indefinite);
    cache.put("b", &sample_response("2"), CacheTtl::Indefinite);
    cache.clear().unwrap();
    assert!(cache.get("a").is_none());
    assert!(cache.get("b").is_none());
}

// ---------------------------------------------------------------------------
// Cache key
// ---------------------------------------------------------------------------

#[test]
fn cache_key_is_order_independent_over_headers() {
    let a = cache_key(
        "GET",
        "https://example.invalid/x",
        &[("a", "1"), ("b", "2")],
    );
    let b = cache_key(
        "GET",
        "https://example.invalid/x",
        &[("b", "2"), ("a", "1")],
    );
    assert_eq!(a, b);
}

#[test]
fn cache_key_differs_by_method_url_or_headers() {
    let base = cache_key("GET", "https://example.invalid/x", &[]);
    assert_ne!(base, cache_key("POST", "https://example.invalid/x", &[]));
    assert_ne!(base, cache_key("GET", "https://example.invalid/y", &[]));
    assert_ne!(
        base,
        cache_key("GET", "https://example.invalid/x", &[("a", "1")])
    );
}

// ---------------------------------------------------------------------------
// HttpClient — mocked HTTP
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_returns_body_and_status() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/thing")
        .with_status(200)
        .with_body("payload")
        .create_async()
        .await;

    let client = HttpClient::with_in_memory_cache().unwrap();
    let response = client
        .get(&format!("{}/thing", server.url()), &[], CacheTtl::Never)
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"payload");
    mock.assert_async().await;
}

#[tokio::test]
async fn get_with_indefinite_ttl_serves_second_call_from_cache() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/cached")
        .with_status(200)
        .with_body("first")
        .expect(1)
        .create_async()
        .await;

    let client = HttpClient::with_in_memory_cache().unwrap();
    let url = format!("{}/cached", server.url());

    let first = client.get(&url, &[], CacheTtl::Indefinite).await.unwrap();
    let second = client.get(&url, &[], CacheTtl::Indefinite).await.unwrap();

    assert_eq!(first.body, second.body);
    mock.assert_async().await;
}

#[tokio::test]
async fn get_with_never_ttl_hits_server_every_time() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/uncached")
        .with_status(200)
        .with_body("x")
        .expect(2)
        .create_async()
        .await;

    let client = HttpClient::with_in_memory_cache().unwrap();
    let url = format!("{}/uncached", server.url());

    client.get(&url, &[], CacheTtl::Never).await.unwrap();
    client.get(&url, &[], CacheTtl::Never).await.unwrap();

    mock.assert_async().await;
}

#[tokio::test]
async fn non_cacheable_status_is_not_stored() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/not-found")
        .with_status(404)
        .with_body("nope")
        .expect(2)
        .create_async()
        .await;

    let client = HttpClient::with_in_memory_cache().unwrap();
    let url = format!("{}/not-found", server.url());

    client.get(&url, &[], CacheTtl::Indefinite).await.unwrap();
    client.get(&url, &[], CacheTtl::Indefinite).await.unwrap();

    // 404 isn't in ALLOWED_CACHE_STATUSES, so both calls hit the server.
    mock.assert_async().await;
}

#[tokio::test]
async fn post_sends_body_and_reports_status() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/submit")
        .match_body("payload")
        .with_status(200)
        .with_body("ok")
        .create_async()
        .await;

    let client = HttpClient::with_in_memory_cache().unwrap();
    let response = client
        .post(
            &format!("{}/submit", server.url()),
            &[],
            b"payload".to_vec(),
            CacheTtl::Never,
        )
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    mock.assert_async().await;
}

#[tokio::test]
async fn clear_cache_evicts_stored_responses() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/cached")
        .with_status(200)
        .with_body("first")
        .expect(2)
        .create_async()
        .await;

    let client = HttpClient::with_in_memory_cache().unwrap();
    let url = format!("{}/cached", server.url());

    client.get(&url, &[], CacheTtl::Indefinite).await.unwrap();
    client.clear_cache().await.unwrap();
    client.get(&url, &[], CacheTtl::Indefinite).await.unwrap();

    mock.assert_async().await;
}
