use fantoccini::Locator;
use lineup_e2e::TestInstance;

/// Invite a member user, accept the invite to set a password, log in
/// as them, and verify they see only member-level nav items (no Team
/// or Admin links).
#[tokio::test]
async fn member_sees_restricted_nav() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    // As PD: create a member invite via POST /users/invite.
    // The response page shows the invite link regardless of whether
    // the email was sent (demo tenants skip email delivery).
    client
        .execute(
            &format!(
                r#"
                var form = document.createElement('form');
                form.method = 'POST';
                form.action = '{}/users/invite';
                [['email','test@example.com'],['name','Test Member'],['role','Member']].forEach(function(pair) {{
                    var input = document.createElement('input');
                    input.name = pair[0];
                    input.value = pair[1];
                    form.appendChild(input);
                }});
                document.body.appendChild(form);
                form.submit();
                "#,
                instance.base_url()
            ),
            vec![],
        )
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Extract the invite URL from the response page.
    let source = client.source().await.unwrap();
    let invite_url = source
        .split("/invite/")
        .nth(1)
        .and_then(|s| s.split('<').next())
        .map(|path| format!("/invite/{path}"))
        .expect("expected invite URL on the result page");

    // Navigate to the invite URL to see the password-set form.
    client
        .goto(&format!("{}{}", instance.base_url(), invite_url))
        .await
        .unwrap();

    // Fill in the password form.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("input[name='password']"))
        .await
        .expect("expected password input on invite acceptance page");

    lineup_e2e::set_input_value(&client, "input[name='password']", "testpassword123").await;
    lineup_e2e::set_input_value(&client, "input[name='password_confirm']", "testpassword123").await;
    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;

    // Should redirect to /login after accepting the invite.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let url = client.current_url().await.unwrap();
    assert!(
        url.path().starts_with("/login"),
        "expected redirect to /login after invite acceptance, got {}",
        url.path()
    );

    // Log in with the new credentials.
    // Step 1: enter email.
    let email_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#email"))
        .await
        .expect("expected email input");
    email_input.send_keys("test@example.com").await.unwrap();
    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;

    // Step 2: enter password.
    let password_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#password"))
        .await
        .expect("expected password input");
    password_input.send_keys("testpassword123").await.unwrap();
    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;

    // Should land on /practices.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let url = client.current_url().await.unwrap();
    assert!(
        url.path().starts_with("/practices"),
        "expected /practices after login, got {}",
        url.path()
    );

    // Verify member-level nav: should see Practices, My
    // but NOT Team or Admin.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("data-nav=\"/practices\""),
        "member should see Practices nav link"
    );
    assert!(
        source.contains("data-nav=\"/my\""),
        "member should see My nav link"
    );
    assert!(
        !source.contains("data-nav=\"/team\""),
        "member should NOT see Team nav link"
    );
    assert!(
        !source.contains("data-nav=\"/admin\""),
        "member should NOT see Admin nav link"
    );

    client.close().await.unwrap();
}
