use fantoccini::Locator;
use lineup_e2e::TestInstance;

/// Verifies the app boots, demo seeds, and the practices page renders.
#[tokio::test]
async fn practices_page_loads() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::XPath("//*[contains(text(), 'Practices')]"))
        .await
        .expect("expected 'Practices' text on the page");

    client.close().await.unwrap();
}

/// Verifies the history page lists committed practices from the demo fixture.
#[tokio::test]
async fn history_page_loads() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    client
        .goto(&format!("{}/practices", instance.base_url()))
        .await
        .unwrap();

    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::XPath("//*[contains(text(), 'Practices')]"))
        .await
        .expect("expected practices page to load");

    // The demo fixture creates practices; verify the page has practice content.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("/detail") || source.contains("Past practices"),
        "expected practice content on the page"
    );

    client.close().await.unwrap();
}

/// Verifies the roster page renders with demo rowers.
#[tokio::test]
async fn roster_page_loads() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    client
        .goto(&format!("{}/team/roster", instance.base_url()))
        .await
        .unwrap();

    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::XPath("//*[contains(text(), 'Roster')]"))
        .await
        .expect("expected 'Roster' text on the team page");

    let source = client.source().await.unwrap();
    assert!(
        source.contains("Alice") || source.contains("Bob") || source.contains("Carla"),
        "expected demo rower names on the roster page"
    );

    client.close().await.unwrap();
}
