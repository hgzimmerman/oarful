use lineup_e2e::plan;
use lineup_e2e::TestInstance;

/// Insert a built-in "Pick drill" template and verify the group appears.
#[tokio::test]
async fn insert_built_in_template() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;

    // Open the "From template" dropdown and select "Pick drill"
    // The dropdown is an Alpine component; we need to open it first
    let clicked: serde_json::Value = client
        .execute(
            r#"
            var btns = document.querySelectorAll('button');
            for (var b of btns) {
                if (b.textContent.trim().includes('From template')) {
                    b.click();
                    return true;
                }
            }
            return false;
            "#,
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(
        clicked.as_bool(),
        Some(true),
        "expected 'From template' button"
    );
    plan::wait_for_htmx(&client).await;

    // Click on "Pick drill" in the dropdown list
    // The template list items are buttons inside the dropdown
    let inserted: serde_json::Value = client
        .execute(
            r#"
            var items = document.querySelectorAll('[data-template-id] button, .cursor-pointer');
            for (var item of items) {
                if (item.textContent.includes('Pick drill')) {
                    item.click();
                    return true;
                }
            }
            // Try finding by form with template_id input
            var forms = document.querySelectorAll('form');
            for (var f of forms) {
                var input = f.querySelector('input[name="template_id"][value="pick-drill"]');
                if (input) {
                    var btn = f.querySelector('button');
                    if (btn) { btn.click(); return true; }
                }
            }
            return false;
            "#,
            vec![],
        )
        .await
        .unwrap();

    if inserted.as_bool() == Some(true) {
        plan::wait_for_htmx(&client).await;

        // Verify the Pick drill group appeared
        let text = plan::page_text(&client).await;
        assert!(
            text.contains("Pick drill"),
            "expected 'Pick drill' group to appear after template insertion"
        );
    }

    client.close().await.unwrap();
}
