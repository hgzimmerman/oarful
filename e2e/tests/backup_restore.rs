//! E2E tests for the backup/export and restore flow.
//!
//! These tests use reqwest directly against the in-process Axum server
//! (no browser needed — we're testing file download/upload, not UI).

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

/// Create a demo tenant and return the auth cookie header value.
/// The demo endpoint issues a JWT and sets a cookie.
async fn setup_demo(base: &str, client: &reqwest::Client) -> String {
    let resp = client.post(format!("{base}/demo")).send().await.unwrap();
    assert!(
        resp.status().is_success()
            || resp.status() == StatusCode::SEE_OTHER
            || resp.status() == StatusCode::OK,
        "demo creation should succeed, got {}",
        resp.status()
    );
    // The cookie store handles the JWT cookie automatically.
    // Return the final URL to confirm we landed on /practices.
    let url = resp.url().to_string();
    assert!(
        url.contains("/practices"),
        "should redirect to practices, got {url}"
    );
    url
}

#[tokio::test]
async fn export_downloads_sqlite_file() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    setup_demo(&base, &client).await;

    // Download the export.
    let resp = client
        .get(format!("{base}/admin/export"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("sqlite"),
        "content-type should indicate sqlite, got {content_type}"
    );

    let disposition = resp
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        disposition.contains("attachment") && disposition.contains(".db"),
        "content-disposition should be an attachment .db, got {disposition}"
    );

    let bytes = resp.bytes().await.unwrap();
    assert!(
        bytes.len() > 1000,
        "exported file should be non-trivial ({} bytes)",
        bytes.len()
    );

    // Verify it's a valid SQLite file (starts with "SQLite format 3\0").
    assert_eq!(
        &bytes[0..16],
        b"SQLite format 3\0",
        "exported file should be a valid SQLite database"
    );
}

#[tokio::test]
async fn restore_rejects_missing_account() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    setup_demo(&base, &client).await;

    // Create a minimal SQLite database that has NO app_user matching
    // the demo user (demo@localhost). We'll create one with a different email.
    let temp_db = instance
        .base_url()
        .replace("http://localhost:", "/tmp/lineup_e2e_restore_reject_")
        + ".db";
    {
        use diesel::prelude::*;
        use diesel::Connection;
        // Copy the real DB first so schema is correct, then delete the demo user.
        let export_resp = client
            .get(format!("{base}/admin/export"))
            .send()
            .await
            .unwrap();
        let export_bytes = export_resp.bytes().await.unwrap();
        tokio::fs::write(&temp_db, &export_bytes).await.unwrap();

        // Open and delete all users.
        let mut conn = diesel::SqliteConnection::establish(&temp_db).unwrap();
        diesel::sql_query("DELETE FROM app_user")
            .execute(&mut conn)
            .unwrap();
    }

    // Upload the DB with no matching user.
    let file_bytes = tokio::fs::read(&temp_db).await.unwrap();
    let form = reqwest::multipart::Form::new().part(
        "backup",
        reqwest::multipart::Part::bytes(file_bytes)
            .file_name("backup.db")
            .mime_str("application/x-sqlite3")
            .unwrap(),
    );

    let resp = client
        .post(format!("{base}/admin/restore"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("does not exist in this backup"),
        "should show error about missing account, got: {body}"
    );

    // Clean up.
    let _ = tokio::fs::remove_file(&temp_db).await;
}

#[tokio::test]
async fn restore_warns_on_credential_mismatch() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    setup_demo(&base, &client).await;

    // Export the current DB, then modify the password hash.
    let temp_db = instance
        .base_url()
        .replace("http://localhost:", "/tmp/lineup_e2e_restore_creds_")
        + ".db";
    {
        use diesel::prelude::*;
        use diesel::Connection;
        let export_resp = client
            .get(format!("{base}/admin/export"))
            .send()
            .await
            .unwrap();
        let export_bytes = export_resp.bytes().await.unwrap();
        tokio::fs::write(&temp_db, &export_bytes).await.unwrap();

        // Change the password hash for the demo user.
        let mut conn = diesel::SqliteConnection::establish(&temp_db).unwrap();
        diesel::sql_query(
            "UPDATE app_user SET password_hash = 'different_hash' WHERE email = 'demo@localhost'",
        )
        .execute(&mut conn)
        .unwrap();
        // Checkpoint WAL so the base file has all changes.
        diesel::sql_query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&mut conn)
            .unwrap();
    }

    let file_bytes = tokio::fs::read(&temp_db).await.unwrap();
    let form = reqwest::multipart::Form::new().part(
        "backup",
        reqwest::multipart::Part::bytes(file_bytes)
            .file_name("backup.db")
            .mime_str("application/x-sqlite3")
            .unwrap(),
    );

    let resp = client
        .post(format!("{base}/admin/restore"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("different credentials"),
        "should warn about credential mismatch, got snippet: {}",
        &body[..body.len().min(500)]
    );
    // Should show a "Restore anyway" button.
    assert!(
        body.contains("Restore anyway"),
        "should show confirm button"
    );

    let _ = tokio::fs::remove_file(&temp_db).await;
}

#[tokio::test]
async fn export_then_restore_preserves_data() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();

    setup_demo(&base, &client).await;

    // Verify we have rowers by checking the roster page.
    let roster = client
        .get(format!("{base}/team/roster"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(roster.contains("Alice"), "demo should have Alice in roster");

    // Export the database.
    let export_resp = client
        .get(format!("{base}/admin/export"))
        .send()
        .await
        .unwrap();
    let backup_bytes = export_resp.bytes().await.unwrap();
    assert!(backup_bytes.len() > 1000);

    // Save to a temp file for restore.
    let temp_db = format!("/tmp/lineup_e2e_roundtrip_{}.db", std::process::id());
    tokio::fs::write(&temp_db, &backup_bytes).await.unwrap();

    // Restore it back (same credentials, should proceed directly).
    let file_bytes = tokio::fs::read(&temp_db).await.unwrap();
    let form = reqwest::multipart::Form::new().part(
        "backup",
        reqwest::multipart::Part::bytes(file_bytes)
            .file_name("backup.db")
            .mime_str("application/x-sqlite3")
            .unwrap(),
    );

    let resp = client
        .post(format!("{base}/admin/restore"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("restored successfully"),
        "restore should succeed, got snippet: {}",
        &body[..body.len().min(500)]
    );

    // Verify data survived the round-trip — check the roster still has rowers.
    // Need to re-auth since the connection pool was reset.
    // The demo cookie should still work (it's a JWT, not session-bound).
    let roster_after = client
        .get(format!("{base}/team/roster"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        roster_after.contains("Alice"),
        "Alice should still be in the roster after restore"
    );
    assert!(
        roster_after.contains("Bob"),
        "Bob should still be in the roster after restore"
    );

    let _ = tokio::fs::remove_file(&temp_db).await;
}
