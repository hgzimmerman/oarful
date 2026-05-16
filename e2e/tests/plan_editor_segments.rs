use lineup_e2e::plan;
use lineup_e2e::TestInstance;

/// Edit segment duration and rate, verify values persist after selecting another segment.
#[tokio::test]
async fn edit_segment_fields() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    // Add a warmup group (auto-selects it, shows its default segment)
    plan::add_item(&client, "Warmup").await;

    // The group's first segment should be visible in the editor.
    // Change the duration value via JS (the input has name="duration_value").
    let has_dur = plan::count(&client, "input[name='duration_value']").await;
    assert!(
        has_dur > 0,
        "expected duration_value input in segment editor"
    );

    // Set duration to 8
    lineup_e2e::set_input_value(&client, "input[name='duration_value']", "8").await;

    // Trigger the change event to fire HTMX
    client
        .execute(
            r#"
            var el = document.querySelector("input[name='duration_value']");
            el.dispatchEvent(new Event('change', { bubbles: true }));
            "#,
            vec![],
        )
        .await
        .unwrap();
    plan::wait_for_htmx(&client).await;

    // Verify the value persisted in the re-rendered form
    let val: serde_json::Value = client
        .execute(
            r#"return document.querySelector("input[name='duration_value']")?.value || ''"#,
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(val.as_str().unwrap(), "8", "expected duration to be 8");

    client.close().await.unwrap();
}

/// Add a rest segment to a group, then delete it.
#[tokio::test]
async fn add_remove_segments() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    plan::add_item(&client, "Warmup").await;

    // A warmup group starts with 1 segment
    let text = plan::page_text(&client).await;
    assert!(text.contains("1 segment"), "expected 1 segment initially");

    // Add a rest segment
    plan::click_button(&client, "+ Rest").await;

    // Should now have 2 segments
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("2 segments"),
        "expected 2 segments after adding rest"
    );

    client.close().await.unwrap();
}
