use lineup_e2e::TestInstance;

/// After logging out, navigating to a protected page should redirect
/// to /login.
#[tokio::test]
async fn logout_redirects_to_login() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    // Log out via POST /logout.
    client
        .execute(
            &format!(
                r#"
                var form = document.createElement('form');
                form.method = 'POST';
                form.action = '{}/logout';
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

    // Try to access a protected page.
    client
        .goto(&format!("{}/practices", instance.base_url()))
        .await
        .unwrap();

    // Should have been redirected to /login.
    let url = client.current_url().await.unwrap();
    assert!(
        url.path().starts_with("/login"),
        "expected redirect to /login after logout, got {}",
        url.path()
    );

    client.close().await.unwrap();
}
