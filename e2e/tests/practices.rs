use fantoccini::Locator;
use lineup_e2e::TestInstance;

/// Coach creates a new practice date and it appears in the schedule.
#[tokio::test]
async fn create_practice() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    // We're on the practices page (Schedule tab). Set a date 30 days
    // from now to avoid colliding with demo fixture dates.
    let future_date = (chrono::Utc::now().date_naive() + chrono::TimeDelta::try_days(30).unwrap())
        .format("%Y-%m-%d")
        .to_string();
    let display_date = (chrono::Utc::now().date_naive() + chrono::TimeDelta::try_days(30).unwrap())
        .format("%Y-%m-%d")
        .to_string();

    // Fill in the date input via JS (date inputs are hard to interact with natively).
    lineup_e2e::set_input_value(&client, "input#date", &future_date).await;

    // Submit the create practice form.
    lineup_e2e::scroll_and_click(&client, "form[action='/practices'] button[type='submit']").await;

    // Wait for the page to re-render with the new practice.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // The new practice should appear in the schedule list.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::XPath(&format!(
            "//*[contains(text(), '{}')]",
            display_date
        )))
        .await
        .expect("expected newly created practice date in schedule");

    // It should link to the solve page.
    let source = client.source().await.unwrap();
    assert!(
        source.contains("/solve/"),
        "expected solve link for the new practice"
    );

    client.close().await.unwrap();
}
