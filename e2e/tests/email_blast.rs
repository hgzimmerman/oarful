//! E2E tests for the coach email blast (reminders and lineups).
//!
//! These tests use reqwest directly against the in-process Axum server.
//! Demo tenants have billing_status="demo" which blocks outbound email,
//! so the send tests verify that blocking behavior.

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

/// Create a demo tenant and return the final URL (should be /practices).
async fn setup_demo(base: &str, client: &reqwest::Client) -> String {
    let resp = client.post(format!("{base}/demo")).send().await.unwrap();
    assert!(
        resp.status().is_success()
            || resp.status() == StatusCode::SEE_OTHER
            || resp.status() == StatusCode::OK,
        "demo creation should succeed, got {}",
        resp.status()
    );
    let url = resp.url().to_string();
    assert!(
        url.contains("/practices"),
        "should redirect to practices, got {url}"
    );
    url
}

/// Parse practice IDs from the practices page by finding `/solve/N` links.
fn parse_practice_ids(html: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for segment in html.split("/solve/") {
        // The ID ends at a quote or query string: /solve/3" or /solve/3?...
        let end = segment
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(segment.len());
        let id = &segment[..end];
        if !id.is_empty() {
            ids.push(id.to_string());
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

/// Parse committed practice dates from the history page.
/// The template renders dates as YYYY-MM-DD inside the history rows.
fn parse_history_dates(html: &str) -> Vec<String> {
    let mut dates = Vec::new();
    for segment in html.split("/history/") {
        // Extract the practice ID from /history/N links.
        if let Some(end) = segment.find('"') {
            let _id = &segment[..end];
        }
    }
    // Look for YYYY-MM-DD date patterns in the page content.
    let re_like = |s: &str| -> Vec<String> {
        let mut result = Vec::new();
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len();
        let mut i = 0;
        while i + 10 <= len {
            // Match YYYY-MM-DD pattern.
            if chars[i].is_ascii_digit()
                && chars[i + 1].is_ascii_digit()
                && chars[i + 2].is_ascii_digit()
                && chars[i + 3].is_ascii_digit()
                && chars[i + 4] == '-'
                && chars[i + 5].is_ascii_digit()
                && chars[i + 6].is_ascii_digit()
                && chars[i + 7] == '-'
                && chars[i + 8].is_ascii_digit()
                && chars[i + 9].is_ascii_digit()
            {
                let date: String = chars[i..i + 10].iter().collect();
                result.push(date);
                i += 10;
            } else {
                i += 1;
            }
        }
        result
    };
    dates.extend(re_like(html));
    dates.sort();
    dates.dedup();
    dates
}

#[tokio::test]
async fn reminder_preview_shows_recipients() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    setup_demo(&base, &client).await;

    // Fetch the practices page to find practice IDs.
    let practices_html = client
        .get(format!("{base}/practices"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let practice_ids = parse_practice_ids(&practices_html);
    assert!(
        !practice_ids.is_empty(),
        "demo should have at least one practice"
    );

    let pid = &practice_ids[0];

    // GET the reminder preview with a practice_id.
    let resp = client
        .get(format!(
            "{base}/practices/reminder-preview?practice_ids={pid}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "reminder preview should return 200"
    );

    let body = resp.text().await.unwrap();
    // The preview modal should contain either recipient names or a
    // "no reminders" / "No recipients" indication.
    assert!(
        body.contains("Alice")
            || body.contains("Bob")
            || body.contains("No reminders")
            || body.contains("no reminders")
            || body.contains("No recipients")
            || body.contains("0 recipient"),
        "reminder preview should contain recipient info or no-recipients message, got snippet: {}",
        &body[..body.len().min(500)]
    );
}

#[tokio::test]
async fn lineup_preview_shows_recipients() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    setup_demo(&base, &client).await;

    // Fetch the history page to find a committed practice date.
    let history_html = client
        .get(format!("{base}/history"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let dates = parse_history_dates(&history_html);
    assert!(
        !dates.is_empty(),
        "demo should have at least one committed practice with a date on the history page"
    );

    let date = &dates[0];

    // GET the lineup preview with a committed date.
    let resp = client
        .get(format!(
            "{base}/practices/lineup-preview?dates={date}&scope=placed"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "lineup preview should return 200"
    );

    let body = resp.text().await.unwrap();
    // The preview should contain recipient names or a no-recipients message.
    assert!(
        body.contains("Alice")
            || body.contains("Bob")
            || body.contains("No lineup")
            || body.contains("no lineup")
            || body.contains("No recipients")
            || body.contains("0 recipient"),
        "lineup preview should contain recipient info or no-recipients message, got snippet: {}",
        &body[..body.len().min(500)]
    );
}

#[tokio::test]
async fn send_reminders_blocked_for_demo() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    setup_demo(&base, &client).await;

    // Get a practice ID from the practices page.
    let practices_html = client
        .get(format!("{base}/practices"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let practice_ids = parse_practice_ids(&practices_html);
    assert!(
        !practice_ids.is_empty(),
        "demo should have at least one practice"
    );

    let pid = &practice_ids[0];

    // POST to send reminders — should be blocked for demo tenants.
    let resp = client
        .post(format!("{base}/practices/send-reminders"))
        .form(&[("practice_ids", pid.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("Upgrade to unlock email"),
        "send reminders should be blocked for demo tenant, got: {}",
        &body[..body.len().min(500)]
    );
}

#[tokio::test]
async fn send_lineups_blocked_for_demo() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    setup_demo(&base, &client).await;

    // Fetch the history page to find a committed practice date.
    let history_html = client
        .get(format!("{base}/history"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let dates = parse_history_dates(&history_html);
    assert!(
        !dates.is_empty(),
        "demo should have at least one committed practice date"
    );

    let date = &dates[0];

    // POST to send lineups — should be blocked for demo tenants.
    let resp = client
        .post(format!("{base}/practices/send-lineups"))
        .form(&[("dates", date.as_str()), ("scope", "placed")])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("Upgrade to unlock email"),
        "send lineups should be blocked for demo tenant, got: {}",
        &body[..body.len().min(500)]
    );
}
