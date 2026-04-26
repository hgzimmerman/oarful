use fantoccini::Locator;
use lineup_e2e::{MailMessage, TestInstance};
use reqwest::StatusCode;

/// A rower in a committed lineup changes their availability to "No".
/// Verify the amber warning banner appears linking to the affected
/// lineup.
#[tokio::test]
async fn availability_change_warns_about_committed_lineup() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let client = instance.connect().await;

    // Log out from the demo PD user.
    client
        .execute(
            &format!(
                r#"await fetch("{}/logout", {{ method: "POST", redirect: "follow" }})"#,
                instance.base_url()
            ),
            vec![],
        )
        .await
        .unwrap();
    client
        .goto(&format!("{}/login", instance.base_url()))
        .await
        .unwrap();

    // Request a magic link for Alice (her app_user was created by the
    // demo fixture, linked to her rower).
    client
        .execute(
            &format!(
                r#"
                var form = document.createElement('form');
                form.method = 'POST';
                form.action = '{}/login/magic';
                var input = document.createElement('input');
                input.name = 'email';
                input.value = 'alice@test.example.com';
                form.appendChild(input);
                document.body.appendChild(form);
                form.submit();
                "#,
                instance.base_url()
            ),
            vec![],
        )
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Capture the magic link from the ChannelMailer.
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

    // Navigate to the magic link to log in as Alice.
    client.goto(&magic_url).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Navigate to availability page.
    client
        .goto(&format!("{}/my/availability", instance.base_url()))
        .await
        .unwrap();

    // Should see upcoming practices with status dropdowns.
    // The Monday practice has a committed lineup with Alice in it —
    // it should show a "View lineup" badge.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("select[name='status']"))
        .await
        .expect("expected availability status dropdown");

    // Find the row with "View lineup" (committed practice) and change
    // its status to "No".
    client
        .execute(
            r#"
            var rows = document.querySelectorAll('tr');
            for (var i = 0; i < rows.length; i++) {
                var row = rows[i];
                if (row.innerHTML.indexOf('View lineup') !== -1) {
                    var sel = row.querySelector('select[name="status"]');
                    if (sel) {
                        sel.value = 'No';
                        sel.dispatchEvent(new Event('change', { bubbles: true }));
                        // Submit the form in this row.
                        var form = row.querySelector('form');
                        if (form) form.requestSubmit();
                    }
                    break;
                }
            }
            "#,
            vec![],
        )
        .await
        .unwrap();

    // Wait for the page to re-render with the stale warning.
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Verify the amber warning banner appears.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css(".bg-amber-50"))
        .await
        .expect("expected amber warning banner after changing availability on committed lineup");

    // Verify the warning mentions the committed lineup.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("committed lineup"),
        "expected warning to mention 'committed lineup'"
    );

    client.close().await.unwrap();
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::limited(10))
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

/// The demo fixture seeds committed lineups for Monday where all placed
/// rowers are available. The nav badge should start empty, then show a
/// count after we flip a committed rower's availability to "No".
#[tokio::test]
async fn nav_stale_badge_reflects_availability_changes() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();
    setup_demo(&base, &client).await;

    // Badge should be empty — all committed rowers are available.
    let resp = client
        .get(format!("{base}/nav/stale-badge"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.text().await.unwrap();
    assert!(
        body.is_empty(),
        "badge should be empty when no lineups are stale, got: {body}"
    );

    // Get committed practice IDs from the history page.
    let history_html = client
        .get(format!("{base}/history"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let history_id = history_html
        .split("href=\"/history/")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("expected at least one committed practice on history page");
    let practice_id: i32 = history_id.parse().expect("practice id should be numeric");

    // Get a rower ID from the committed lineup via the no-show checkbox
    // values on the history detail page: name="no_show" value="{rower_id}".
    let detail_html = client
        .get(format!("{base}/history/{practice_id}"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let rower_id: i32 = detail_html
        .split("name=\"no_show\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse().ok())
        .expect("expected a no-show checkbox with rower id in history detail");

    // Flip that rower's availability to "No" via the coach attendance toggle.
    let toggle_resp = client
        .post(format!("{base}/team/attendance/toggle"))
        .form(&[
            ("rower_id", rower_id.to_string()),
            ("practice_id", practice_id.to_string()),
            ("status", "No".to_string()),
        ])
        .send()
        .await
        .unwrap();
    assert!(
        toggle_resp.status().is_success(),
        "attendance toggle should succeed, got {}",
        toggle_resp.status()
    );

    // Badge should now show a count.
    let resp = client
        .get(format!("{base}/nav/stale-badge"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.text().await.unwrap();
    assert!(
        !body.is_empty(),
        "badge should be non-empty after making a committed lineup stale"
    );
    assert!(
        body.contains("bg-amber-500"),
        "badge should have amber styling, got: {body}"
    );
}
