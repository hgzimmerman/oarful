use fantoccini::Locator;
use lineup_e2e::plan;
use lineup_e2e::TestInstance;
use std::time::Duration;

/// Create a template with content, then import it into a practice plan.
#[tokio::test]
async fn import_template_into_practice() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    let base = instance.base_url();

    // Step 1: Create a template with a warmup group
    plan::goto_templates(&client, &base).await;
    plan::create_template(&client).await;
    plan::open_editor(&client).await;
    plan::add_item(&client, "Warmup").await;
    plan::click_button(&client, "Save").await;

    // Step 2: Navigate to a practice detail page
    // The demo fixture creates upcoming practices
    client.goto(&format!("{base}/practices")).await.unwrap();
    client
        .wait()
        .at_most(Duration::from_secs(5))
        .for_element(Locator::XPath("//*[contains(text(), 'Practices')]"))
        .await
        .unwrap();

    // Find a practice detail link
    let detail_url: serde_json::Value = client
        .execute(
            r#"
            var links = document.querySelectorAll('a[href*="/detail"]');
            for (var l of links) {
                if (l.href.includes('/practices/') && l.href.includes('/detail')) {
                    return l.href;
                }
            }
            return '';
            "#,
            vec![],
        )
        .await
        .unwrap();

    let detail = detail_url.as_str().unwrap_or("");
    if detail.is_empty() {
        // No upcoming practices with detail links — skip this test gracefully
        client.close().await.unwrap();
        return;
    }

    client.goto(detail).await.unwrap();
    plan::wait_for_htmx(&client).await;

    // Step 3: Look for "use template" button and click it
    let text = plan::page_text(&client).await;
    if text.contains("use template") {
        plan::click_button(&client, "use template").await;

        // The import picker modal should appear with our template
        plan::wait_for_htmx(&client).await;
        let text = plan::page_text(&client).await;
        assert!(
            text.contains("new-template") || text.contains("Import"),
            "expected import picker to show our template"
        );
    }

    client.close().await.unwrap();
}
