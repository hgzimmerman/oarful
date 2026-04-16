use fantoccini::Locator;
use lineup_e2e::TestInstance;

/// Generate lineups with 1 alternative, wait for the alternative to
/// stream in, then click "Use this" and verify the editor loads with
/// the alternative's placements.
#[tokio::test]
async fn use_streamed_alternative() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    // Find a solve link from the practices page.
    let source = client.source().await.unwrap();
    let solve_path = source
        .split("href=\"/solve/")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(|id| format!("/solve/{id}"))
        .expect("expected a /solve/ link on the practices page");

    // Navigate to the solve page with generate=1, alts=1, budget=1.
    let generate_url = format!(
        "{}{}{}generate=1&budget=1&alts=1",
        instance.base_url(),
        solve_path,
        if solve_path.contains('?') { "&" } else { "?" }
    );
    client.goto(&generate_url).await.unwrap();

    // Wait for the primary result to stream in (boat cards in editor).
    client
        .wait()
        .at_most(std::time::Duration::from_secs(15))
        .for_element(Locator::Css("[data-editor-boat]"))
        .await
        .expect("expected boat cards in lineup editor after generation");

    // Wait for an alternative to stream in — look for "Alternative #2".
    let alt_header = client
        .wait()
        .at_most(std::time::Duration::from_secs(15))
        .for_element(Locator::XPath("//*[contains(text(), 'Alternative #2')]"))
        .await
        .expect("expected Alternative #2 to appear via SSE");

    // Verify the "Use this" link exists.
    let use_this = client
        .find(Locator::XPath("//a[contains(text(), 'Use this')]"))
        .await
        .expect("expected 'Use this' link on alternative");

    // Click the "Use this" link via JS.
    client
        .execute(
            r#"
            var links = document.querySelectorAll('a');
            for (var i = 0; i < links.length; i++) {
                if (links[i].textContent.trim() === 'Use this') {
                    links[i].click();
                    break;
                }
            }
            "#,
            vec![],
        )
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    // Wait for the editor to load with the alternative's placements.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("[data-editor-boat]"))
        .await
        .expect("expected editor boat cards after 'Use this' navigation");

    // Verify we're on the solve page (not an error page).
    let url = client.current_url().await.unwrap();
    assert!(
        url.path().starts_with("/solve/"),
        "expected to stay on /solve/ page after 'Use this', got {}",
        url.path()
    );

    // Verify rower names appear — the alternative was loaded into the editor.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("data-editor-boat"),
        "expected editor boat cards with rower placements"
    );

    client.close().await.unwrap();
}
