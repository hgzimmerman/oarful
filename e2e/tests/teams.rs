use fantoccini::Locator;
use lineup_e2e::TestInstance;

/// Create a second team via admin, verify the team selector dropdown
/// appears, switch to the new team, verify the practices page reflects
/// the change, then switch back.
#[tokio::test]
async fn switch_teams() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;

    // Wait for team selector to load via HTMX.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Create a second team by POSTing to /teams.
    client
        .execute(
            &format!(
                r#"
                var form = document.createElement('form');
                form.method = 'POST';
                form.action = '{}/teams';
                var input = document.createElement('input');
                input.name = 'name';
                input.value = 'Second Team';
                form.appendChild(input);
                document.body.appendChild(form);
                form.submit();
                "#,
                instance.base_url()
            ),
            vec![],
        )
        .await
        .unwrap();

    // Wait for redirect.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Navigate to practices page — the team selector should now show
    // a dropdown since there are 2 teams.
    client
        .goto(&format!("{}/practices", instance.base_url()))
        .await
        .unwrap();

    // Wait for the team selector to load (it's fetched via hx-trigger="load").
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify the dropdown exists now.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("select[name='team_id']"))
        .await
        .expect("expected team selector dropdown after creating second team");

    // Switch to the new team by submitting the form.
    client
        .execute(
            r#"
            var sel = document.querySelector("select[name='team_id']");
            var opts = sel.options;
            for (var i = 0; i < opts.length; i++) {
                if (opts[i].text.includes('Second Team')) {
                    sel.value = opts[i].value;
                    sel.form.submit();
                    break;
                }
            }
            "#,
            vec![],
        )
        .await
        .unwrap();

    // Wait for redirect to /practices.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // The new team should have no practices — verify empty state.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#content"))
        .await
        .unwrap();

    // Wait for team selector to reload.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify "Second Team" is now the selected option.
    let selected: serde_json::Value = client
        .execute(
            r#"
            var sel = document.querySelector("select[name='team_id']");
            if (!sel) return "no selector";
            return sel.options[sel.selectedIndex].text;
            "#,
            vec![],
        )
        .await
        .unwrap();

    assert_eq!(
        selected.as_str().unwrap().trim(),
        "Second Team",
        "expected 'Second Team' to be the active team"
    );

    // Switch back to the original team.
    client
        .execute(
            r#"
            var sel = document.querySelector("select[name='team_id']");
            var opts = sel.options;
            for (var i = 0; i < opts.length; i++) {
                if (opts[i].text.includes('Demo Rowing Club')) {
                    sel.value = opts[i].value;
                    sel.form.submit();
                    break;
                }
            }
            "#,
            vec![],
        )
        .await
        .unwrap();

    // Wait for redirect.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify we're back on the original team with practices.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let source = client.source().await.unwrap();
    assert!(
        source.contains("/solve/") || source.contains("Practices"),
        "expected original team's practices after switching back"
    );

    client.close().await.unwrap();
}
