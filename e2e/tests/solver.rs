use fantoccini::Locator;
use lineup_e2e::TestInstance;

/// Full coach workflow: navigate to an uncommitted practice, generate
/// lineups, commit them, and verify they appear on the history page.
#[tokio::test]
async fn generate_and_commit_lineup() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    // Find a solve link from the practices page to get a practice ID.
    let source = client.source().await.unwrap();
    let solve_path = source
        .split("href=\"/solve/")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(|id| format!("/solve/{id}"))
        .expect("expected a /solve/ link on the practices page");

    // Navigate directly to the solve page (avoid HTMX partial swap issues).
    client
        .goto(&format!("{}{}", instance.base_url(), solve_path))
        .await
        .unwrap();

    // Wait for the knobs form to render.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::XPath("//button[contains(text(), 'Generate')]"))
        .await
        .expect("expected Generate button on solve page");

    // Set time budget to 1s for fast test execution.
    client
        .execute(
            r#"var input = document.querySelector('input[name="budget"]');
               if (input) { input.value = '1'; }"#,
            vec![],
        )
        .await
        .unwrap();

    // Add generate=1 to trigger the solver and submit via navigation.
    let generate_url = format!(
        "{}{}{}generate=1&budget=1&partial=1",
        instance.base_url(),
        solve_path,
        if solve_path.contains('?') { "&" } else { "?" }
    );
    client.goto(&generate_url).await.unwrap();

    // Wait for the solver result — boat cards should appear in the editor.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(15))
        .for_element(Locator::Css("[data-editor-boat]"))
        .await
        .expect("expected boat cards in lineup editor after generation");

    // Verify rower names appear in the editor.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("Alice") || source.contains("Bob") || source.contains("Hana"),
        "expected rower names in generated lineup"
    );

    // Commit the lineup by clicking the emerald button.
    client
        .execute(
            r#"var btn = document.querySelector('.bg-emerald-600');
               if (btn) { btn.scrollIntoView(); btn.click(); }"#,
            vec![],
        )
        .await
        .unwrap();

    // Wait for redirect to history detail.
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    let url = client.current_url().await.unwrap();
    assert!(
        url.path().starts_with("/history/"),
        "expected redirect to /history/ after commit, got {}",
        url.path()
    );

    // Verify committed lineup content.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("committed"),
        "expected 'committed' timestamp on history detail"
    );

    client.close().await.unwrap();
}

/// Verify the demo fixture solves with default knobs (5s budget,
/// strict fill) — this is what a new user sees on first Generate click.
#[tokio::test]
async fn demo_solves_with_default_knobs() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    let source = client.source().await.unwrap();
    let solve_path = source
        .split("href=\"/solve/")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(|id| format!("/solve/{id}"))
        .expect("expected a /solve/ link");

    // Default knobs: 5s budget, partial=0 (strict)
    let url = format!(
        "{}{}{}generate=1",
        instance.base_url(),
        solve_path,
        if solve_path.contains('?') { "&" } else { "?" }
    );
    client.goto(&url).await.unwrap();

    client
        .wait()
        .at_most(std::time::Duration::from_secs(20))
        .for_element(Locator::Css("[data-editor-boat]"))
        .await
        .expect("demo should solve with default knobs (5s budget, strict fill)");

    client.close().await.unwrap();
}
