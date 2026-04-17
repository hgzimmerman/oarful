use fantoccini::Locator;
use lineup_e2e::{MailMessage, TestInstance};

/// Request a password reset, capture the magic link via ChannelMailer,
/// navigate to it, set a new password, then log in with the new password.
#[tokio::test]
async fn password_reset_flow() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let client = instance.connect().await;

    // ── Step 1: Log out ──
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

    // ── Step 2: Navigate to forgot-password page ──
    client
        .goto(&format!(
            "{}/forgot-password?email=demo@localhost",
            instance.base_url()
        ))
        .await
        .unwrap();

    // Verify we're on the forgot-password page.
    let email_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#email"))
        .await
        .expect("expected email input on forgot-password page");

    // Check prefilled email.
    let val = email_input.prop("value").await.unwrap().unwrap_or_default();
    assert_eq!(val, "demo@localhost", "email should be prefilled");

    // Submit the forgot-password form.
    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify we see the confirmation page.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("reset link sent") || source.contains("demo@localhost"),
        "expected forgot-password confirmation page"
    );

    // ── Step 3: Capture the password-reset email ──
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), mail_rx.recv())
        .await
        .expect("should receive mail within 5s")
        .expect("mail channel should not be closed");

    let reset_url = match msg {
        MailMessage::PasswordReset { clubs, .. } => {
            assert!(!clubs.is_empty(), "expected at least one club link");
            clubs[0].1.clone()
        }
        other => panic!("expected PasswordReset, got {other:?}"),
    };

    // ── Step 4: Navigate to the magic link → lands on reset-password form ──
    client
        .goto(&format!("{}{}", instance.base_url(), reset_url))
        .await
        .unwrap();

    // Wait for redirect to /reset-password.
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if let Ok(url) = client.current_url().await {
            if url.path() == "/reset-password" {
                break;
            }
        }
    }

    let url = client.current_url().await.unwrap();
    assert_eq!(
        url.path(),
        "/reset-password",
        "expected redirect to /reset-password"
    );

    // ── Step 5: Set a new password ──
    let pw_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#password"))
        .await
        .expect("expected password input on reset form");
    pw_input.send_keys("newpassword123").await.unwrap();

    let pw_confirm = client
        .find(Locator::Css("#password_confirm"))
        .await
        .expect("expected password_confirm input");
    pw_confirm.send_keys("newpassword123").await.unwrap();

    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;

    // Wait for redirect to /login?reset=1.
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if let Ok(url) = client.current_url().await {
            if url.path() == "/login" {
                break;
            }
        }
    }

    let url = client.current_url().await.unwrap();
    assert_eq!(url.path(), "/login", "expected redirect to /login");

    // Verify the success banner is shown.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("Password updated"),
        "expected success banner on login page after password reset"
    );

    // ── Step 6: Log in with the new password ──
    let _email_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#email"))
        .await
        .expect("expected email input on login page");
    // Clear and set the email (may be prefilled from known_user cookie).
    lineup_e2e::set_input_value(&client, "#email", "demo@localhost").await;

    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;

    // Wait for password step.
    let pw_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#password"))
        .await
        .expect("expected password input on login step 2");
    pw_input.send_keys("newpassword123").await.unwrap();

    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;

    // Wait for redirect to /practices.
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if let Ok(url) = client.current_url().await {
            if url.path() == "/practices" {
                break;
            }
        }
    }

    let url = client.current_url().await.unwrap();
    assert!(
        url.path().starts_with("/practices"),
        "expected redirect to /practices after login with new password, got {}",
        url.path()
    );

    client.close().await.unwrap();
}
