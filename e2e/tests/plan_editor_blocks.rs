use lineup_e2e::plan;
use lineup_e2e::TestInstance;

/// Add warmup and piece groups, verify they appear in the strip.
#[tokio::test]
async fn add_groups() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    // Default timeline has launch + dock (structural). Add a warmup.
    plan::add_item(&client, "Warmup").await;

    let strip_items = plan::count(&client, "#tl-strip [data-tl-id]").await;
    assert!(
        strip_items >= 3,
        "expected at least 3 strip items (launch + warmup + dock), got {strip_items}"
    );

    // Add a piece
    plan::add_item(&client, "Piece").await;

    let strip_items = plan::count(&client, "#tl-strip [data-tl-id]").await;
    assert!(
        strip_items >= 4,
        "expected at least 4 strip items after adding piece, got {strip_items}"
    );

    client.close().await.unwrap();
}

/// Add rest and turn blocks, verify they appear.
#[tokio::test]
async fn add_blocks() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    plan::add_item(&client, "Rest").await;
    plan::add_item(&client, "Turn").await;

    let text = plan::page_text(&client).await;
    assert!(text.contains("Rest"), "expected Rest block in editor");
    assert!(text.contains("Turn"), "expected Turn block in editor");

    client.close().await.unwrap();
}

/// Delete a group and verify it's removed.
#[tokio::test]
async fn delete_group() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    plan::add_item(&client, "Warmup").await;
    let before = plan::count(&client, "#tl-strip [data-tl-id]").await;

    // The warmup should be selected. Click the Delete button.
    plan::click_button(&client, "Delete").await;

    let after = plan::count(&client, "#tl-strip [data-tl-id]").await;
    assert!(
        after < before,
        "expected fewer strip items after deleting group"
    );

    client.close().await.unwrap();
}

/// Duplicate a group and verify two exist.
#[tokio::test]
async fn duplicate_group() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    plan::add_item(&client, "Warmup").await;
    let before = plan::count(&client, "#tl-strip [data-tl-id]").await;

    // The warmup should be selected. Click Duplicate.
    plan::click_button(&client, "Duplicate").await;

    let after = plan::count(&client, "#tl-strip [data-tl-id]").await;
    assert_eq!(
        after,
        before + 1,
        "expected one more strip item after duplicating group"
    );

    client.close().await.unwrap();
}
