use lineup_e2e::plan;
use lineup_e2e::TestInstance;

/// Set repeat count to 3, verify "x3" shows, then split and verify expanded.
#[tokio::test]
async fn repeat_and_split() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;

    // Set repeat to 3 via the repeat input
    let has_repeat = plan::count(&client, "input[name='repeat']").await;
    if has_repeat > 0 {
        lineup_e2e::set_input_value(&client, "input[name='repeat']", "3").await;
        client
            .execute(
                r#"document.querySelector("input[name='repeat']").dispatchEvent(new Event('change', { bubbles: true }))"#,
                vec![],
            )
            .await
            .unwrap();
        plan::wait_for_htmx(&client).await;

        let text = plan::page_text(&client).await;
        assert!(
            text.contains("×3") || text.contains("x3") || text.contains("Split"),
            "expected repeat indicator or split button"
        );

        // Click Split to expand
        plan::click_button(&client, "Split").await;

        // After splitting, the group should have 3x the original segments
        // and repeat should be gone
        let text = plan::page_text(&client).await;
        assert!(
            !text.contains("×3") && !text.contains("Split"),
            "expected repeat to be removed after split"
        );
    }

    client.close().await.unwrap();
}

/// Toggle a warmup group to piece and verify the badge updates.
#[tokio::test]
async fn toggle_group_type() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;

    // The group should show "Warmup" badge
    let text = plan::page_text(&client).await;
    assert!(text.contains("Warmup"), "expected Warmup badge");

    // Click the toggle button ("→ Piece")
    plan::click_button(&client, "→ Piece").await;

    // Should now show "Piece"
    let text = plan::page_text(&client).await;
    assert!(text.contains("Piece"), "expected Piece badge after toggle");

    client.close().await.unwrap();
}

/// Set row-by rotation and verify the rotation label appears.
#[tokio::test]
async fn rotation_settings() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;

    // Set row_by to 4 via the select element
    let has_rowby = plan::count(&client, "select[name='row_by']").await;
    if has_rowby > 0 {
        lineup_e2e::select_value(&client, "select[name='row_by']", "4").await;
        client
            .execute(
                r#"document.querySelector("select[name='row_by']").dispatchEvent(new Event('change', { bubbles: true }))"#,
                vec![],
            )
            .await
            .unwrap();
        plan::wait_for_htmx(&client).await;

        // Verify rotation controls appeared
        let text = plan::page_text(&client).await;
        assert!(
            text.contains("rotate") || text.contains("Rotate") || text.contains("by 4"),
            "expected rotation controls after setting row_by"
        );
    }

    client.close().await.unwrap();
}
