//! E2E tests for the self-service club signup flow.
//!
//! These tests use reqwest directly against the in-process Axum server
//! (no browser needed — we're testing form submission and redirects).

use lineup_e2e::TestInstance;
use reqwest::StatusCode;

/// Build a reqwest client that follows redirects and stores cookies.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap()
}

#[tokio::test]
async fn signup_creates_club_and_redirects() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    let resp = client
        .post(format!("{base}/signup"))
        .form(&[
            ("club_name", "Test Rowing Club"),
            ("first_name", "Jane"),
            ("last_name", "Doe"),
            ("email", "jane-signup-ok@example.com"),
            ("password", "securepassword1"),
            ("password_confirm", "securepassword1"),
        ])
        .send()
        .await
        .unwrap();

    // The client follows the redirect, so we should land on /practices.
    let final_url = resp.url().to_string();
    assert!(
        final_url.contains("/practices"),
        "should redirect to /practices, got {final_url}"
    );
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify the JWT cookie works — GET /practices should return 200.
    let practices_resp = client
        .get(format!("{base}/practices"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        practices_resp.status(),
        StatusCode::OK,
        "authenticated user should be able to access /practices"
    );
}

#[tokio::test]
async fn signup_rejects_short_password() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    let resp = client
        .post(format!("{base}/signup"))
        .form(&[
            ("club_name", "Short Pass Club"),
            ("first_name", "Bob"),
            ("last_name", "Smith"),
            ("email", "bob-short-pw@example.com"),
            ("password", "short"),
            ("password_confirm", "short"),
        ])
        .send()
        .await
        .unwrap();

    let final_url = resp.url().to_string();
    assert!(
        !final_url.contains("/practices"),
        "should NOT redirect to /practices on validation error, got {final_url}"
    );

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("Password must be at least 8 characters"),
        "should contain password length error, got snippet: {}",
        &body[..body.len().min(500)]
    );
}

#[tokio::test]
async fn signup_rejects_mismatched_passwords() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    let resp = client
        .post(format!("{base}/signup"))
        .form(&[
            ("club_name", "Mismatch Club"),
            ("first_name", "Carol"),
            ("last_name", "Tester"),
            ("email", "carol-mismatch@example.com"),
            ("password", "password123"),
            ("password_confirm", "different456"),
        ])
        .send()
        .await
        .unwrap();

    let final_url = resp.url().to_string();
    assert!(
        !final_url.contains("/practices"),
        "should NOT redirect to /practices on validation error, got {final_url}"
    );

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("Passwords do not match"),
        "should contain password mismatch error, got snippet: {}",
        &body[..body.len().min(500)]
    );
}
