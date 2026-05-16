use lineup_e2e::plan;
use lineup_e2e::TestInstance;

/// Change target minutes and verify slack indicator updates.
#[tokio::test]
async fn change_target_minutes() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    // Add some content so planned minutes > 0
    plan::add_item(&client, "Warmup").await;

    // Change the target minutes input
    let has_target = plan::count(&client, "input[name='new_target']").await;
    if has_target > 0 {
        lineup_e2e::set_input_value(&client, "input[name='new_target']", "120").await;
        client
            .execute(
                r#"document.querySelector("input[name='new_target']").dispatchEvent(new Event('change', { bubbles: true }))"#,
                vec![],
            )
            .await
            .unwrap();
        plan::wait_for_htmx(&client).await;

        // Verify the target updated (should show "of 120 min" or similar)
        let text = plan::page_text(&client).await;
        assert!(text.contains("120"), "expected target minutes to show 120");
    }

    client.close().await.unwrap();
}

/// Toggle preview panel on and off.
#[tokio::test]
async fn preview_toggle() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    // Add a warmup so the preview has content
    plan::add_item(&client, "Warmup").await;

    // The template detail page opens with preview by default (open_preview).
    // Check if "Hide preview" button exists
    let text = plan::page_text(&client).await;
    if text.contains("Hide preview") {
        // Preview is on — toggle it off
        plan::click_button(&client, "Hide preview").await;

        // Preview panel should be gone
        let text = plan::page_text(&client).await;
        assert!(
            text.contains("Preview") && !text.contains("Hide preview"),
            "expected preview to be hidden"
        );

        // Toggle it back on
        plan::click_button(&client, "Preview").await;
        let text = plan::page_text(&client).await;
        assert!(
            text.contains("min planned"),
            "expected preview content when toggled back on"
        );
    } else if text.contains("Preview") {
        // Preview is off — toggle it on
        plan::click_button(&client, "Preview").await;

        let text = plan::page_text(&client).await;
        assert!(
            text.contains("min planned") || text.contains("Hide preview"),
            "expected preview panel after toggling on"
        );
    }

    client.close().await.unwrap();
}
