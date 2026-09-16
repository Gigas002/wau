use std::time::Duration;

use super::*;

fn auth(server: &mockito::ServerGuard) -> GitHubAuth {
    GitHubAuth::new_with_urls(server.url(), server.url())
}

#[tokio::test]
async fn get_codes_returns_parsed_response() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/login/device/code")
        .match_body(mockito::Matcher::JsonString(format!(
            r#"{{"client_id":"{CLIENT_ID}"}}"#
        )))
        .with_status(200)
        .with_body(
            serde_json::json!({
                "device_code": "dev123",
                "user_code": "ABCD-1234",
                "verification_uri": "https://github.com/login/device",
                "expires_in": 900,
                "interval": 5,
            })
            .to_string(),
        )
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let codes = auth(&server).get_codes(&http).await.unwrap();
    assert_eq!(codes.user_code, "ABCD-1234");
    assert_eq!(codes.interval, 5);
}

#[tokio::test]
async fn get_codes_errors_on_bad_status() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/login/device/code")
        .with_status(500)
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let err = auth(&server).get_codes(&http).await.unwrap_err();
    assert!(matches!(err, Failure::Internal(_)));
}

#[tokio::test]
async fn poll_for_access_token_retries_on_authorization_pending_then_succeeds() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/login/oauth/access_token")
        .with_status(200)
        .with_body(serde_json::json!({ "error": "authorization_pending" }).to_string())
        .expect(1)
        .create_async()
        .await;
    server
        .mock("POST", "/login/oauth/access_token")
        .with_status(200)
        .with_body(
            serde_json::json!({ "access_token": "tok-123", "token_type": "bearer", "scope": "" })
                .to_string(),
        )
        .expect(1)
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let token = auth(&server)
        .poll_for_access_token(&http, "dev123", Duration::from_millis(1))
        .await
        .unwrap();
    assert_eq!(token, "tok-123");
}

#[tokio::test]
async fn poll_for_access_token_errors_on_other_error_codes() {
    let mut server = mockito::Server::new_async().await;
    server
        .mock("POST", "/login/oauth/access_token")
        .with_status(200)
        .with_body(serde_json::json!({ "error": "access_denied" }).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    let err = auth(&server)
        .poll_for_access_token(&http, "dev123", Duration::from_millis(1))
        .await
        .unwrap_err();
    assert!(matches!(err, Failure::Internal(_)));
}

#[tokio::test]
async fn get_rate_limit_status_includes_auth_header_when_token_present() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/rate_limit")
        .match_header("authorization", "Bearer tok-123")
        .with_status(200)
        .with_body(serde_json::json!({ "resources": {} }).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    auth(&server)
        .get_rate_limit_status(&http, Some("tok-123"))
        .await
        .unwrap();
    mock.assert_async().await;
}

#[tokio::test]
async fn get_rate_limit_status_omits_auth_header_without_token() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/rate_limit")
        .match_header("authorization", mockito::Matcher::Missing)
        .with_status(200)
        .with_body(serde_json::json!({ "resources": {} }).to_string())
        .create_async()
        .await;

    let http = HttpClient::new().unwrap();
    auth(&server)
        .get_rate_limit_status(&http, None)
        .await
        .unwrap();
    mock.assert_async().await;
}
