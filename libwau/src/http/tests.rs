use super::*;

#[tokio::test]
async fn get_returns_body_and_status() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/thing")
        .with_status(200)
        .with_body("payload")
        .create_async()
        .await;

    let client = HttpClient::new().unwrap();
    let response = client
        .get(&format!("{}/thing", server.url()), &[])
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"payload");
    mock.assert_async().await;
}

#[tokio::test]
async fn get_hits_server_every_time() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("GET", "/uncached")
        .with_status(200)
        .with_body("x")
        .expect(2)
        .create_async()
        .await;

    let client = HttpClient::new().unwrap();
    let url = format!("{}/uncached", server.url());

    client.get(&url, &[]).await.unwrap();
    client.get(&url, &[]).await.unwrap();

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

    let client = HttpClient::new().unwrap();
    let response = client
        .post(&format!("{}/submit", server.url()), &[], b"payload".to_vec())
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    mock.assert_async().await;
}
