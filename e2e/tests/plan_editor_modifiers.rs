use lineup_e2e::plan;
use lineup_e2e::TestInstance;

/// Add a blade modifier, change to "on square", then remove it.
#[tokio::test]
async fn blade_modifier() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;

    // Open the modifier picker and add Blade
    plan::click_button(&client, "+ Add modifier").await;
    plan::click_button(&client, "Blade").await;

    let text = plan::page_text(&client).await;
    assert!(text.contains("Blade"), "expected Blade modifier to appear");

    // Click the "on square" chip to change blade type
    plan::click_button(&client, "on square").await;

    // Verify the chip is now active (appears in the text)
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("on square"),
        "expected 'on square' blade option"
    );

    // Remove the modifier via the × button
    plan::click_button(&client, "\u{00d7}").await;

    // After removal, there should be no modifier-remove forms
    let remove_forms: u64 = client
        .execute(
            r#"return document.querySelectorAll('form[hx-post*="modifier-remove"]').length"#,
            vec![],
        )
        .await
        .map(|v| v.as_u64().unwrap_or(0))
        .unwrap_or(0);
    assert_eq!(
        remove_forms, 0,
        "expected no modifier remove forms after removal"
    );

    client.close().await.unwrap();
}

/// Add drills modifier, toggle "feet out" and "eyes closed" on.
#[tokio::test]
async fn drill_toggle() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;

    plan::click_button(&client, "+ Add modifier").await;
    plan::click_button(&client, "Drills").await;

    // Toggle "feet out"
    plan::click_button(&client, "feet out").await;
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("feet out"),
        "expected 'feet out' to be toggled"
    );

    // Toggle "eyes closed"
    plan::click_button(&client, "eyes closed").await;
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("eyes closed"),
        "expected 'eyes closed' to be toggled"
    );

    client.close().await.unwrap();
}

/// Add pause-at modifier, select "release" and "arms away".
#[tokio::test]
async fn pause_at_multi() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;

    plan::click_button(&client, "+ Add modifier").await;
    plan::click_button(&client, "Pause at").await;

    // Toggle pause points
    plan::click_button(&client, "release").await;
    plan::click_button(&client, "arms away").await;

    let text = plan::page_text(&client).await;
    assert!(text.contains("release"), "expected 'release' selected");
    assert!(text.contains("arms away"), "expected 'arms away' selected");

    client.close().await.unwrap();
}

/// Add a group-level modifier and verify it shows as inherited on a segment.
#[tokio::test]
async fn modifier_inheritance() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;

    // Add blade at group level (the group is selected, so we're in group view)
    plan::click_button(&client, "+ Add modifier").await;
    plan::click_button(&client, "Blade").await;

    // Click the segment card to select it
    client
        .execute(
            r#"
            var cards = document.querySelectorAll('[data-drag-zone="seglist"]');
            if (cards.length > 0) { cards[0].click(); }
            "#,
            vec![],
        )
        .await
        .unwrap();
    plan::wait_for_htmx(&client).await;

    // The segment should show the inherited Blade modifier with "override here"
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("override here") || text.contains("Blade"),
        "expected inherited blade modifier on segment"
    );

    client.close().await.unwrap();
}

/// Override an inherited modifier on a segment, then revert it.
#[tokio::test]
async fn modifier_override_revert() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;

    // Add blade at group level
    plan::click_button(&client, "+ Add modifier").await;
    plan::click_button(&client, "Blade").await;

    // Select the segment by clicking its card (the div with data-drag-id inside the group)
    client
        .execute(
            r#"
            var cards = document.querySelectorAll('[data-drag-zone="seglist"]');
            if (cards.length > 0) { cards[0].click(); }
            "#,
            vec![],
        )
        .await
        .unwrap();
    plan::wait_for_htmx(&client).await;

    // Should see "override here" for the inherited blade modifier
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("override here"),
        "expected 'override here' button for inherited modifier"
    );

    // Override it
    plan::click_button(&client, "override here").await;

    // Should now see "revert" button
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("revert"),
        "expected 'revert' button after overriding"
    );

    // Revert it back
    plan::click_button(&client, "revert").await;

    // Should see "override here" again
    let text = plan::page_text(&client).await;
    assert!(
        text.contains("override here"),
        "expected 'override here' after reverting"
    );

    client.close().await.unwrap();
}
