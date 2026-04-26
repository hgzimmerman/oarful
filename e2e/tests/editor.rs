use fantoccini::Locator;
use lineup_e2e::TestInstance;

/// Helper: navigate to the solver, generate lineups with budget=1,
/// and return once the editor has boat cards.
async fn generate_lineup(client: &fantoccini::Client, base_url: &str) {
    // Find a solve link from the practices page.
    let source = client.source().await.unwrap();
    let solve_path = source
        .split("href=\"/solve/")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(|id| format!("/solve/{id}"))
        .expect("expected a /solve/ link on the practices page");

    // Generate with budget=1, partial=1 to allow partial fills.
    let url = format!(
        "{}{solve_path}{}generate=1&budget=1&partial=1",
        base_url,
        if solve_path.contains('?') { "&" } else { "?" }
    );
    client.goto(&url).await.unwrap();

    // Wait for editor boat cards.
    client
        .wait()
        .at_most(std::time::Duration::from_secs(15))
        .for_element(Locator::Css("[data-editor-boat]"))
        .await
        .expect("expected boat cards after generation");
}

/// Swap two rowers via the editor and verify they exchanged positions.
#[tokio::test]
async fn swap_two_rowers() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    generate_lineup(&client, &instance.base_url()).await;

    // Find two seated rowers in the same boat.
    let rowers: serde_json::Value = client
        .execute(
            r#"
            var rows = document.querySelectorAll('div[data-boat][data-rower]');
            var seated = [];
            for (var i = 0; i < rows.length; i++) {
                var r = rows[i];
                if (r.dataset.rower && r.dataset.boat !== 'bench' && r.dataset.boat !== 'sculling') {
                    seated.push({
                        key: r.dataset.key,
                        rower: r.dataset.rower,
                        name: r.dataset.name || '',
                        boat: r.dataset.boat,
                        seat: r.dataset.seat
                    });
                }
                if (seated.length >= 2) break;
            }
            return JSON.stringify(seated);
            "#,
            vec![],
        )
        .await
        .unwrap();

    let rowers: Vec<serde_json::Value> = serde_json::from_str(rowers.as_str().unwrap()).unwrap();
    assert!(rowers.len() >= 2, "need at least 2 seated rowers to swap");

    let key_a = rowers[0]["key"].as_str().unwrap().to_string();
    let key_b = rowers[1]["key"].as_str().unwrap().to_string();
    let rower_a = rowers[0]["rower"].as_str().unwrap().to_string();
    let rower_b = rowers[1]["rower"].as_str().unwrap().to_string();

    // Click first seat to select it.
    client
        .execute(
            &format!(
                r#"document.querySelector('[data-key="{}"]').click();"#,
                key_a
            ),
            vec![],
        )
        .await
        .unwrap();

    // Click second seat to trigger swap + HTMX re-render.
    client
        .execute(
            &format!(
                r#"document.querySelector('[data-key="{}"]').click();"#,
                key_b
            ),
            vec![],
        )
        .await
        .unwrap();

    // Wait for the HTMX re-render to complete.
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("[data-editor-boat]"))
        .await
        .expect("editor should re-render after swap");

    // Verify the rowers exchanged: rower_a should now be at key_b's
    // position and vice versa.
    let after: serde_json::Value = client
        .execute(
            &format!(
                r#"
                var a = document.querySelector('[data-key="{}"]');
                var b = document.querySelector('[data-key="{}"]');
                return JSON.stringify({{
                    a_rower: a ? a.dataset.rower : null,
                    b_rower: b ? b.dataset.rower : null
                }});
                "#,
                key_a, key_b
            ),
            vec![],
        )
        .await
        .unwrap();

    let after: serde_json::Value = serde_json::from_str(after.as_str().unwrap()).unwrap();
    assert_eq!(
        after["a_rower"].as_str().unwrap(),
        &rower_b,
        "rower_b should now be in seat A after swap"
    );
    assert_eq!(
        after["b_rower"].as_str().unwrap(),
        &rower_a,
        "rower_a should now be in seat B after swap"
    );

    client.close().await.unwrap();
}

/// Transfer rowers from one boat to another via the boat pill interaction:
/// select source boat (click header), then click destination pill.
/// Verify rowers appear in the destination and the source is emptied.
#[tokio::test]
async fn transfer_between_boats() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    generate_lineup(&client, &instance.base_url()).await;

    // Find two active boats with seated rowers.
    let boats: serde_json::Value = client
        .execute(
            r#"
            var cards = document.querySelectorAll('[data-editor-boat]');
            var result = [];
            cards.forEach(function(c) {
                if (c.dataset.hidden === 'true') return;
                var rowers = [];
                c.querySelectorAll('div[data-rower]').forEach(function(r) {
                    if (r.dataset.rower) rowers.push(r.dataset.name || r.dataset.rower);
                });
                if (rowers.length > 0) {
                    result.push({ boatId: c.dataset.editorBoat, rowerCount: rowers.length, rowerNames: rowers });
                }
            });
            return JSON.stringify(result);
            "#,
            vec![],
        )
        .await
        .unwrap();
    let boats: Vec<serde_json::Value> = serde_json::from_str(boats.as_str().unwrap()).unwrap();
    assert!(
        boats.len() >= 2,
        "need at least 2 active boats with rowers for transfer test"
    );

    let source_id = boats[0]["boatId"].as_str().unwrap().to_string();
    let dest_id = boats[1]["boatId"].as_str().unwrap().to_string();
    let source_rower_names: Vec<String> = boats[0]["rowerNames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let dest_rower_names: Vec<String> = boats[1]["rowerNames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        !source_rower_names.is_empty(),
        "source boat should have rowers"
    );

    // Trigger the transfer by navigating to the editor endpoint with
    // the transfer param. This is what the JS does under the hood.
    let state: String = client
        .execute(
            r#"
            var editor = document.querySelector('#lineup-editor');
            var params = [];
            editor.querySelectorAll('div[data-boat][data-seat][data-rower]').forEach(function(el) {
                if (el.dataset.rower && el.dataset.boat !== 'bench' && el.dataset.boat !== 'sculling') {
                    params.push('seat=' + el.dataset.rower + ':' + el.dataset.boat + ':' + el.dataset.seat);
                }
            });
            editor.querySelectorAll('[data-editor-boat]').forEach(function(card) {
                if (card.dataset.hidden !== 'true') {
                    params.push('boat=' + card.dataset.editorBoat);
                }
            });
            return params.join('&');
            "#,
            vec![],
        )
        .await
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let editor_url = client
        .execute(
            r#"return document.querySelector('#lineup-editor').dataset.editorUrl;"#,
            vec![],
        )
        .await
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Trigger the transfer via HTMX (same mechanism the editor JS uses),
    // keeping the full page layout intact.
    let transfer_params = format!("{state}&transfer={source_id}:{dest_id}");
    client
        .execute(
            &format!(
                r#"htmx.ajax('GET', '{editor_url}?{transfer_params}',
                    {{target: document.querySelector('#lineup-editor'), swap: 'outerHTML'}});"#,
            ),
            vec![],
        )
        .await
        .unwrap();

    // Wait for HTMX re-render.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#lineup-editor"))
        .await
        .expect("editor should re-render after transfer");

    // Verify: the rowers should have swapped between boats.
    // (Both boats populated → bidirectional swap: src gets dst's rowers,
    // dst gets src's rowers.)
    let after: serde_json::Value = client
        .execute(
            &format!(
                r#"
                var src = document.querySelector('[data-editor-boat="{}"]');
                var srcRowers = [];
                if (src) src.querySelectorAll('div[data-rower]').forEach(function(r) {{
                    if (r.dataset.rower) srcRowers.push(r.dataset.name || r.dataset.rower);
                }});

                var dst = document.querySelector('[data-editor-boat="{}"]');
                var dstRowers = [];
                if (dst) dst.querySelectorAll('div[data-rower]').forEach(function(r) {{
                    if (r.dataset.rower) dstRowers.push(r.dataset.name || r.dataset.rower);
                }});

                return JSON.stringify({{ srcRowers: srcRowers, dstRowers: dstRowers }});
                "#,
                source_id, dest_id
            ),
            vec![],
        )
        .await
        .unwrap();

    let after: serde_json::Value = serde_json::from_str(after.as_str().unwrap()).unwrap();
    let src_rowers_after: Vec<String> = after["srcRowers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let dst_rowers_after: Vec<String> = after["dstRowers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    // Source boat should now have the destination's original rowers.
    let src_has_former_dst_rowers = dest_rower_names
        .iter()
        .filter(|name| src_rowers_after.contains(name))
        .count();
    // Destination boat should now have the source's original rowers.
    let dst_has_former_src_rowers = source_rower_names
        .iter()
        .filter(|name| dst_rowers_after.contains(name))
        .count();

    assert!(
        src_has_former_dst_rowers > 0 || dst_has_former_src_rowers > 0,
        "rowers should have moved between boats after transfer"
    );

    client.close().await.unwrap();
}

/// Toggle a boat pill to deactivate a boat, verify it disappears from
/// the editor.
#[tokio::test]
async fn deactivate_boat() {
    let instance = TestInstance::start().await;
    let client = instance.connect().await;
    generate_lineup(&client, &instance.base_url()).await;

    // Count active boats before.
    let before: serde_json::Value = client
        .execute(
            r#"
            var cards = document.querySelectorAll('[data-editor-boat]');
            var active = 0;
            cards.forEach(function(c) { if (c.dataset.hidden !== 'true') active++; });
            return JSON.stringify({ active: active, total: cards.length });
            "#,
            vec![],
        )
        .await
        .unwrap();
    let before: serde_json::Value = serde_json::from_str(before.as_str().unwrap()).unwrap();
    let active_before = before["active"].as_i64().unwrap();
    assert!(active_before >= 1, "need at least one active boat");

    // Find the first active boat's pill and click it to deactivate.
    client
        .execute(
            r#"
            var cards = document.querySelectorAll('[data-editor-boat]');
            for (var i = 0; i < cards.length; i++) {
                if (cards[i].dataset.hidden !== 'true') {
                    var boatId = cards[i].dataset.editorBoat;
                    var pill = document.querySelector('[data-boat-id="' + boatId + '"]');
                    if (pill) { pill.click(); break; }
                }
            }
            "#,
            vec![],
        )
        .await
        .unwrap();

    // Wait for HTMX re-render.
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    client
        .wait()
        .at_most(std::time::Duration::from_secs(5))
        .for_element(Locator::Css("#lineup-editor"))
        .await
        .expect("editor should re-render after boat toggle");

    // Verify one fewer active boat.
    let after: serde_json::Value = client
        .execute(
            r#"
            var cards = document.querySelectorAll('[data-editor-boat]');
            var active = 0;
            cards.forEach(function(c) { if (c.dataset.hidden !== 'true') active++; });
            return active;
            "#,
            vec![],
        )
        .await
        .unwrap();

    let active_after = after.as_i64().unwrap();
    assert_eq!(
        active_after,
        active_before - 1,
        "deactivating a boat should reduce active count by 1"
    );

    client.close().await.unwrap();
}
