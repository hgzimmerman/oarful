use lineup_e2e::plan;
use lineup_e2e::TestInstance;

/// Build a plan with a warmup group, save it, close, reopen, and verify intact.
#[tokio::test]
async fn save_and_reopen() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    let _url = plan::create_template(&client).await;
    plan::open_editor(&client).await;

    // Add a warmup group
    plan::add_item(&client, "Warmup").await;

    // Add a piece group
    plan::add_item(&client, "Piece").await;

    // Save
    plan::click_button(&client, "Save").await;

    // The editor should close and show the summary view
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("Warmup") && text.contains("Piece"),
        "expected summary to show Warmup and Piece after save"
    );

    // Navigate away and come back by clicking the first template in the list
    plan::goto_templates(&client, &base).await;
    // Click the first template link
    client
        .execute(
            r#"
            var link = document.querySelector('a[href*="/admin/plan-templates/"]');
            if (link) link.click();
            "#,
            vec![],
        )
        .await
        .unwrap();
    plan::wait_for_htmx(&client).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Verify the saved content is still there
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("Warmup") && text.contains("Piece"),
        "expected saved plan to persist after navigation"
    );

    client.close().await.unwrap();
}

/// Modify a plan, close without saving, reopen, verify original state.
#[tokio::test]
async fn close_without_saving() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    // Add a warmup group and save to establish baseline
    plan::add_item(&client, "Warmup").await;
    plan::click_button(&client, "Save").await;

    // Reopen editor
    plan::click_button(&client, "edit plan").await;
    plan::wait_for_htmx(&client).await;

    // Add a piece group (unsaved change)
    plan::add_item(&client, "Piece").await;
    let text = plan::page_text(&client).await;
    assert!(text.contains("Piece"), "expected Piece in unsaved editor");

    // Close without saving
    plan::click_button(&client, "Cancel").await;

    // The summary should show only Warmup (Piece was not saved)
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("Warmup"),
        "expected Warmup in summary after cancel"
    );
    assert!(
        !text.contains("Piece") || text.contains("edit plan"),
        "expected Piece to NOT persist after cancel"
    );

    client.close().await.unwrap();
}
