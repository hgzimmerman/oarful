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

/// Promote the demo tenant so the stale alert poller processes it
/// (demo tenants are skipped by default).
async fn promote_demo_tenant(instance: &TestInstance) {
    use diesel::prelude::*;
    let mut conn = diesel::SqliteConnection::establish(&instance.master_db_path()).unwrap();
    diesel::sql_query("UPDATE tenant SET billing_status = 'grandfathered', demo_expires_at = NULL")
        .execute(&mut conn)
        .unwrap();
    drop(conn);
    instance.refresh_tenant_configs().await;
}

/// Flip a committed rower's availability to "No" via the coach attendance
/// toggle, then call the stale alert poller directly. Verify that a
/// StaleAlert email is sent via the ChannelMailer.
#[tokio::test]
async fn stale_alert_email_sent_on_availability_change() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let base = instance.base_url();
    let pd_client = http_client();

    setup_demo(&base, &pd_client).await;
    promote_demo_tenant(&instance).await;

    // Get a committed practice from the history page.
    let history_html = pd_client
        .get(format!("{base}/practices"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let practice_id: i32 = history_html
        .split("href=\"/practices/")
        .filter_map(|s| {
            let id = s.split('/').next()?;
            if s.contains("/detail") && id.chars().all(|c| c.is_ascii_digit()) && !id.is_empty() {
                id.parse().ok()
            } else {
                None
            }
        })
        .next()
        .expect("expected a committed practice detail link");

    // Get a rower ID from the committed lineup.
    let detail_html = pd_client
        .get(format!("{base}/practices/{practice_id}/detail"))
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
        .expect("expected a no-show checkbox with rower id");

    // Flip rower availability to "No" via coach attendance toggle.
    let toggle_resp = pd_client
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

    // Drain any pending mail messages (magic login etc.) before polling.
    while mail_rx.try_recv().is_ok() {}

    // Trigger the stale alert poller directly via the AppState.
    lineup_server::stale_alert::poll_stale_alerts(&instance.app_state).await;

    // Check for a StaleAlert message in the channel.
    let msg = tokio::time::timeout(std::time::Duration::from_secs(2), mail_rx.recv())
        .await
        .expect("should receive stale alert email")
        .expect("channel open");

    match msg {
        MailMessage::StaleAlert {
            subject, sections, ..
        } => {
            assert!(
                subject.contains("Lineup changes"),
                "subject should contain 'Lineup changes', got: {subject}"
            );
            assert!(
                !sections.is_empty(),
                "should have at least one team section"
            );
            let total_rowers: usize = sections
                .iter()
                .flat_map(|s| &s.practices)
                .map(|p| p.unavailable_rowers.len())
                .sum();
            assert!(
                total_rowers > 0,
                "should have at least one unavailable rower"
            );
        }
        other => panic!("expected StaleAlert, got {other:?}"),
    }
}
