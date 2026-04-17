use lineup_e2e::{MailMessage, TestInstance};
use reqwest::redirect::Policy;

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(Policy::limited(10))
        .build()
        .unwrap()
}

async fn setup_demo(base: &str, client: &reqwest::Client) {
    let resp = client.post(format!("{base}/demo")).send().await.unwrap();
    assert!(
        resp.url().to_string().contains("/practices"),
        "should redirect to practices after demo creation"
    );
}

/// Request a magic link for the given email using the `sender` client,
/// then log in by following the link with the `receiver` client.
///
/// Two separate clients are needed because the magic-link handler skips
/// JWT issuance when the caller already holds a valid JWT cookie (the
/// PD session from /demo). Using a fresh client ensures the magic link
/// actually creates a new session for the target user.
async fn login_via_magic_link(
    base: &str,
    sender: &reqwest::Client,
    receiver: &reqwest::Client,
    mail_rx: &mut tokio::sync::mpsc::UnboundedReceiver<MailMessage>,
    email: &str,
) {
    // Request magic link (any authenticated client can trigger this).
    sender
        .post(format!("{base}/login/magic"))
        .form(&[("email", email)])
        .send()
        .await
        .unwrap();

    // Capture the magic link from the channel mailer.
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), mail_rx.recv())
        .await
        .expect("should receive magic login mail")
        .expect("channel open");
    let magic_url = match msg {
        MailMessage::MagicLogin { clubs, .. } => {
            assert!(!clubs.is_empty(), "expected at least one club link");
            clubs[0].1.clone()
        }
        other => panic!("expected MagicLogin, got {other:?}"),
    };

    // The magic URL is a relative path like /auth/magic/slug/token.
    // Follow it with the fresh client so a new JWT is issued.
    let full_magic_url = format!("{base}{magic_url}");
    let resp = receiver.get(&full_magic_url).send().await.unwrap();
    assert!(
        resp.status().is_success(),
        "magic link login should succeed, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn availability_page_shows_upcoming_practices() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let base = instance.base_url();
    let pd_client = http_client();
    let alice_client = http_client();

    setup_demo(&base, &pd_client).await;
    login_via_magic_link(
        &base,
        &pd_client,
        &alice_client,
        &mut mail_rx,
        "alice@test.example.com",
    )
    .await;

    let resp = alice_client
        .get(format!("{base}/my/availability"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body = resp.text().await.unwrap();

    // The page should contain status dropdowns for upcoming practices.
    assert!(
        body.contains("name=\"status\""),
        "expected status dropdown on availability page"
    );
    assert!(
        body.contains("<select"),
        "expected <select> elements on availability page"
    );
    assert!(
        body.contains("name=\"practice_id\""),
        "expected practice_id hidden fields on availability page"
    );
}

#[tokio::test]
async fn availability_update_persists() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let base = instance.base_url();
    let pd_client = http_client();
    let alice_client = http_client();

    setup_demo(&base, &pd_client).await;
    login_via_magic_link(
        &base,
        &pd_client,
        &alice_client,
        &mut mail_rx,
        "alice@test.example.com",
    )
    .await;

    // Load the availability page to find a practice_id.
    let body = alice_client
        .get(format!("{base}/my/availability"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // Parse the first practice_id from the HTML.
    let practice_id = body
        .split("name=\"practice_id\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("expected at least one practice_id on availability page");

    // Post an availability update: mark as "No".
    let resp = alice_client
        .post(format!("{base}/my/availability"))
        .form(&[("practice_id", practice_id), ("status", "No")])
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "availability POST should succeed, got {}",
        resp.status()
    );

    // Reload the availability page and verify the change persisted.
    let body = alice_client
        .get(format!("{base}/my/availability"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // The page should reflect the "No" selection. Look for an option
    // element with value "No" that is selected, scoped near our practice_id.
    assert!(
        body.contains("selected") && body.contains("No"),
        "expected 'No' to be selected after updating availability"
    );
}
