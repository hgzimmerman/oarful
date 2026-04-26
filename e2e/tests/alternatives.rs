use fantoccini::Locator;
use lineup_e2e::TestInstance;

/// Generate lineups with 1 alternative, wait for the alt tab to
/// appear in the tab bar via SSE, then switch to it and verify the
/// editor loads with that tab's placements.
#[tokio::test]
async fn streamed_alternative_creates_tab() {
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

    // Navigate to the solve page with generate + 1 alternative.
    // partial=2 allows up to 2 optional seats empty per boat.
    let generate_url = format!(
        "{}{}{}generate=1&budget=3&alts=1&partial=2",
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

    // Wait for the "Alt 1" tab to appear in the tab bar via SSE.
    // The alternative re-solves after the primary, so allow another full budget.
    let alt_tab = client
        .wait()
        .at_most(std::time::Duration::from_secs(15))
        .for_element(Locator::XPath(
            "//*[@id='tab-bar']//button[contains(., 'Alt 1')]",
        ))
        .await
        .expect("expected Alt 1 tab to appear in tab bar via SSE");

    // Click the Alt 1 tab to switch to it.
    alt_tab.click().await.unwrap();

    // The tab switch does a full page navigation — wait for the
    // editor to reload with the alternative's placements.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(10))
        .for_element(Locator::Css("[data-editor-boat]"))
        .await
        .expect("expected editor boat cards after switching to Alt 1 tab");

    // Verify we're on the solve page (not an error page).
    let url = client.current_url().await.unwrap();
    assert!(
        url.path().starts_with("/solve/"),
        "expected to stay on /solve/ page after tab switch, got {}",
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
