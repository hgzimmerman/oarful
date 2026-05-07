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

async fn login_via_magic_link(
    base: &str,
    sender: &reqwest::Client,
    receiver: &reqwest::Client,
    mail_rx: &mut tokio::sync::mpsc::UnboundedReceiver<MailMessage>,
    email: &str,
) {
    sender
        .post(format!("{base}/login/magic"))
        .form(&[("email", email)])
        .send()
        .await
        .unwrap();

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

    let resp = receiver
        .get(format!("{base}{magic_url}"))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "magic link login should succeed"
    );
}

/// Find the "Demo Rowing Club" team ID from the teams list.
/// The migration seeds a "Default" team (ID 1), then the demo fixture
/// creates "Demo Rowing Club" (ID 2). Rowers are on the demo team.
async fn find_demo_team_id(base: &str, client: &reqwest::Client) -> String {
    let body = client
        .get(format!("{base}/teams"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    for (i, segment) in body.split("href=\"/teams/").enumerate() {
        if i == 0 {
            continue;
        }
        let id = segment.split('"').next().unwrap_or("");
        if segment.contains("Demo Rowing Club") {
            return id.to_string();
        }
    }
    panic!("could not find 'Demo Rowing Club' team on /teams page");
}

/// Update team settings (PD client).
async fn update_team_settings(
    base: &str,
    client: &reqwest::Client,
    team_id: &str,
    bucket_visibility: &str,
    member_raw_metrics: bool,
) {
    let mut params = vec![
        ("name", "Demo Rowing Club"),
        ("bucket_visibility", bucket_visibility),
    ];
    let mrm;
    if member_raw_metrics {
        mrm = "1".to_string();
        params.push(("member_raw_metrics", &mrm));
    }
    let url = format!("{base}/teams/{team_id}");
    let resp = client.post(&url).form(&params).send().await.unwrap();
    assert!(
        resp.status().is_success(),
        "team update should succeed at {url}, got {}",
        resp.status()
    );
}

/// Default demo: bucket_visibility=off, member_raw_metrics=false.
/// Member profile should NOT show bucket labels.
#[tokio::test]
async fn member_profile_hides_buckets_by_default() {
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

    let body = alice_client
        .get(format!("{base}/my/profile"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // Buckets should be hidden (off by default).
    // The subtitle should only show the side, not weight_class/skill/etc.
    // The subtitle is inside a <span class="font-mono-stat text-xs" ...> in the header.
    let subtitle = body
        .split("font-mono-stat text-xs")
        .nth(1)
        .and_then(|s| s.split("</span>").next())
        .unwrap_or("");
    assert!(
        !subtitle.contains("Expert") && !subtitle.contains("Strong"),
        "subtitle should not contain bucket labels when off, got: '{subtitle}'"
    );

    // Side preference should still be visible.
    assert!(
        body.contains("Side"),
        "member should see Side field regardless of bucket_visibility"
    );
}

/// Set bucket_visibility=view → member sees read-only bucket labels.
#[tokio::test]
async fn member_profile_shows_readonly_buckets_when_view() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let base = instance.base_url();
    let pd_client = http_client();
    let alice_client = http_client();

    setup_demo(&base, &pd_client).await;

    let team_id = find_demo_team_id(&base, &pd_client).await;
    update_team_settings(&base, &pd_client, &team_id, "view", false).await;

    login_via_magic_link(
        &base,
        &pd_client,
        &alice_client,
        &mut mail_rx,
        "alice@test.example.com",
    )
    .await;

    let body = alice_client
        .get(format!("{base}/my/profile"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // Buckets should be visible in the subtitle and attributes section.
    assert!(
        body.contains("Weight") && body.contains("Form") && body.contains("Strength"),
        "member should see bucket labels when bucket_visibility=view"
    );
}

/// Set bucket_visibility=edit → member sees editable bucket dropdowns on edit form.
#[tokio::test]
async fn member_profile_shows_editable_buckets_when_edit() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let base = instance.base_url();
    let pd_client = http_client();
    let alice_client = http_client();

    setup_demo(&base, &pd_client).await;

    let team_id = find_demo_team_id(&base, &pd_client).await;
    update_team_settings(&base, &pd_client, &team_id, "edit", false).await;

    login_via_magic_link(
        &base,
        &pd_client,
        &alice_client,
        &mut mail_rx,
        "alice@test.example.com",
    )
    .await;

    let body = alice_client
        .get(format!("{base}/my/profile"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // Parse alice's rower ID from the profile page.
    let rower_id = body
        .split("/rowers/")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .expect("expected rower link in profile page");

    // Fetch the edit-attributes partial.
    let edit_body = alice_client
        .get(format!("{base}/rowers/{rower_id}/edit-attributes"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // In "edit" mode, bucket fields should have <select> dropdowns.
    assert!(
        edit_body.contains("name=\"weight_class\""),
        "member should see weight_class field when bucket_visibility=edit"
    );
    assert!(
        edit_body.contains("name=\"skill\""),
        "member should see skill field when bucket_visibility=edit"
    );
}

/// member_raw_metrics=false → no erg add form on profile.
/// member_raw_metrics=true → erg add form visible.
#[tokio::test]
async fn member_raw_metrics_toggle() {
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

    // Default: member_raw_metrics=false.
    let body = alice_client
        .get(format!("{base}/my/profile"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !body.contains("/my/erg-test"),
        "member should not see erg test add form when member_raw_metrics=false"
    );

    // Enable member_raw_metrics.
    let team_id = find_demo_team_id(&base, &pd_client).await;
    update_team_settings(&base, &pd_client, &team_id, "off", true).await;

    let body = alice_client
        .get(format!("{base}/my/profile"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("/my/erg-test"),
        "member should see erg test add form when member_raw_metrics=true"
    );
}

/// When member_raw_metrics=true, member can add an erg test via POST /my/erg-test.
#[tokio::test]
async fn member_can_add_erg_test() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let base = instance.base_url();
    let pd_client = http_client();
    let alice_client = http_client();

    setup_demo(&base, &pd_client).await;

    let team_id = find_demo_team_id(&base, &pd_client).await;
    update_team_settings(&base, &pd_client, &team_id, "off", true).await;

    login_via_magic_link(
        &base,
        &pd_client,
        &alice_client,
        &mut mail_rx,
        "alice@test.example.com",
    )
    .await;

    let resp = alice_client
        .post(format!("{base}/my/erg-test"))
        .form(&[("distance_m", "2000"), ("time", "7:30.00")])
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "erg test add should succeed, got {}",
        resp.status()
    );

    let body = resp.text().await.unwrap();
    assert!(
        body.contains("2000m") && body.contains("7:30"),
        "erg test response should show the added test"
    );
    // Member should NOT see delete buttons.
    assert!(
        !body.contains("hx-delete"),
        "member should not see erg test delete buttons"
    );
}

/// When member_raw_metrics=false, POST /my/erg-test is rejected.
#[tokio::test]
async fn member_cannot_add_erg_test_when_disabled() {
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
        .post(format!("{base}/my/erg-test"))
        .form(&[("distance_m", "2000"), ("time", "7:30.00")])
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().as_u16() == 400,
        "erg test add should be rejected when member_raw_metrics=false, got {}",
        resp.status()
    );
}
