use fantoccini::Locator;
use lineup_e2e::{MailMessage, TestInstance};

/// A rower in a committed lineup changes their availability to "No".
/// Verify the amber warning banner appears linking to the affected
/// lineup.
#[tokio::test]
async fn availability_change_warns_about_committed_lineup() {
    let (instance, mut mail_rx) = TestInstance::start_with_mail().await;
    let client = instance.connect().await;

    // As PD: invite alice@test.example.com as Member.
    // The demo fixture gives Alice this email, and she's in the Monday
    // committed lineup (stroke seat in Athena).
    client
        .execute(
            &format!(
                r#"
                var form = document.createElement('form');
                form.method = 'POST';
                form.action = '{}/users/invite';
                [['email','alice@test.example.com'],['name','Alice'],['role','Member']].forEach(function(pair) {{
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
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Capture invite email.
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), mail_rx.recv())
        .await
        .expect("should receive invite mail")
        .expect("channel open");
    let invite_url = match msg {
        MailMessage::Invite { invite_url, .. } => invite_url,
        other => panic!("expected Invite, got {other:?}"),
    };

    // Accept the invite — set password.
    client
        .goto(&format!("{}{}", instance.base_url(), invite_url))
        .await
        .unwrap();
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("input[name='password']"))
        .await
        .expect("expected password form on invite page");
    lineup_e2e::set_input_value(&client, "input[name='password']", "testpass123!").await;
    lineup_e2e::set_input_value(&client, "input[name='password_confirm']", "testpass123!").await;
    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Log in as Alice.
    let email_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#email"))
        .await
        .expect("expected login email input");
    email_input.send_keys("alice@test.example.com").await.unwrap();
    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;

    let password_input = client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#password"))
        .await
        .expect("expected password input");
    password_input.send_keys("testpass123!").await.unwrap();
    lineup_e2e::scroll_and_click(&client, "button[type='submit']").await;
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
