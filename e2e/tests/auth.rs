use fantoccini::Locator;
use lineup_e2e::{MailMessage, TestInstance};

/// Log out, request a magic link, capture it via ChannelMailer,
/// navigate to it, and verify we land back on an authenticated page.
#[tokio::test]
async fn magic_link_login() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let client = instance.connect().await;

    // Log out by submitting the logout form.
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

    // Step 1: Enter the demo user's email.
    let email_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#email"))
        .await
        .expect("expected email input on login page");
    email_input.send_keys("demo@localhost").await.unwrap();

    // Submit the email form.
    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;

    // Step 2: We should see the password step. Request a magic link instead.
    // Wait for the password step to render.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#password"))
        .await
        .expect("expected password input on step 2");

    // Submit the magic link form (hidden email field + POST /login/magic).
    client
        .execute(
            &format!(
                r#"
                var form = document.createElement('form');
                form.method = 'POST';
                form.action = '{}/login/magic';
                var input = document.createElement('input');
                input.name = 'email';
                input.value = 'demo@localhost';
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

    // Wait for the "check your email" confirmation page.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let source = client.source().await.unwrap();
    assert!(
        source.contains("demo@localhost") || source.contains("check your email") || source.contains("sign-in link"),
        "expected magic link confirmation page"
    );

    // Capture the magic link URL from the ChannelMailer.
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), mail_rx.recv())
        .await
        .expect("should receive mail within 5s")
        .expect("mail channel should not be closed");

    let magic_url = match msg {
        MailMessage::MagicLogin { clubs, .. } => {
            assert!(!clubs.is_empty(), "expected at least one club link");
            clubs[0].1.clone()
        }
        other => panic!("expected MagicLogin, got {other:?}"),
    };

    // The magic URL is a relative path like /auth/magic/slug/token.
    // Navigate to it.
    client
        .goto(&format!("{}{}", instance.base_url(), magic_url))
        .await
        .unwrap();

    // Should redirect to /practices after successful magic link auth.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let url = client.current_url().await.unwrap();
    assert!(
        url.path().starts_with("/practices"),
        "expected redirect to /practices after magic link, got {}",
        url.path()
    );

    // Verify we're authenticated — page should contain team content.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("Demo Rowing Club") || source.contains("Practices"),
        "expected authenticated content after magic link login"
    );

    client.close().await.unwrap();
}
