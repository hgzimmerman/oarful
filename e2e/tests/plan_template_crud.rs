use lineup_e2e::plan;
use lineup_e2e::TestInstance;

/// Create a template and verify it appears in the list.
#[tokio::test]
async fn create_and_list() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;

    let text = plan::page_text(&client).await;
    assert!(
        text.contains("0 templates"),
        "expected 0 templates initially"
    );

    plan::create_template(&client).await;

    // Go back to list
    plan::click_button(&client, "← Templates").await;

    let text = plan::page_text(&client).await;
    assert!(
        text.contains("1 template"),
        "expected 1 template after creation"
    );
    assert!(
        text.contains("new-template"),
        "expected default name 'new-template'"
    );

    client.close().await.unwrap();
}

/// Rename a template and verify the name persists.
#[tokio::test]
async fn rename_template() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;

    // Update the name via the meta form
    lineup_e2e::set_input_value(&client, "#tmpl-name", "My Warmup Plan").await;
    plan::click_button(&client, "Save details").await;

    let text = plan::page_text(&client).await;
    assert!(text.contains("My Warmup Plan"), "expected renamed template");

    client.close().await.unwrap();
}

/// Duplicate a template and verify the copy appears.
#[tokio::test]
async fn duplicate_template() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;

    // Click Duplicate (at the bottom of the detail page)
    plan::click_button(&client, "Duplicate").await;

    // Go back to list to verify
    plan::click_button(&client, "← Templates").await;

    let text = plan::page_text(&client).await;
    assert!(
        text.contains("2 templates"),
        "expected 2 templates after duplication"
    );

    client.close().await.unwrap();
}

/// Delete a template and verify it's removed.
#[tokio::test]
async fn delete_template() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;

    // Click Delete — dismiss the confirm dialog via JS override
    client
        .execute("window.confirm = function() { return true; }", vec![])
        .await
        .unwrap();
    plan::click_button(&client, "Delete").await;

    let text = plan::page_text(&client).await;
    assert!(
        text.contains("0 templates") || text.contains("No templates yet"),
        "expected 0 templates after deletion"
    );

    client.close().await.unwrap();
}
