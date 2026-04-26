//! E2E tests for the coach attendance grid (`/team/attendance`).
//!
//! These tests use reqwest directly against the in-process Axum server
//! to verify the attendance grid renders correctly and toggles work.

use lineup_e2e::TestInstance;
use reqwest::StatusCode;

/// Build a reqwest client that follows redirects and stores cookies.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap()
}

/// Create a demo tenant (auto-logged-in as PD via cookie).
async fn setup_demo(base: &str, client: &reqwest::Client) {
    let resp = client.post(format!("{base}/demo")).send().await.unwrap();
    assert!(
        resp.url().to_string().contains("/practices"),
        "should redirect to practices after demo creation"
    );
}

#[tokio::test]
async fn attendance_grid_loads_with_rower_data() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();
    setup_demo(&base, &client).await;

    // Fetch the attendance grid.
    let resp = client
        .get(format!("{base}/team/attendance"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();

    // Verify the grid contains cells with data-rower attributes.
    assert!(
        body.contains("data-rower=\""),
        "attendance grid should contain data-rower attributes"
    );

    // Verify the grid contains cells with data-practice attributes.
    assert!(
        body.contains("data-practice=\""),
        "attendance grid should contain data-practice attributes"
    );

    // Verify there are both available (emerald) and absent (red) cells.
    assert!(
        body.contains("bg-emerald-400"),
        "attendance grid should contain available (bg-emerald-400) cells"
    );
    assert!(
        body.contains("bg-red-400"),
        "attendance grid should contain absent (bg-red-400) cells"
    );
}

#[tokio::test]
async fn attendance_toggle_changes_status() {
    let instance = TestInstance::start().await;
    let base = instance.base_url();
    let client = http_client();
    setup_demo(&base, &client).await;

    // Fetch the attendance grid.
    let resp = client
        .get(format!("{base}/team/attendance"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.text().await.unwrap();

    // Find a cell that is currently available (bg-emerald-400) and extract
    // its rower_id and practice_id from data attributes.
    //
    // We look for a fragment like:
    //   data-rower="42" data-practice="7" ... bg-emerald-400
    // The cell element should have both data attributes and the class.
    let (rower_id, practice_id) = find_available_cell(&body)
        .expect("should find at least one available (bg-emerald-400) cell with data attributes");

    // Toggle that cell to "No".
    let toggle_resp = client
        .post(format!("{base}/team/attendance/toggle"))
        .form(&[
            ("rower_id", rower_id.to_string()),
            ("practice_id", practice_id.to_string()),
            ("status", "No".to_string()),
        ])
        .send()
        .await
        .unwrap();
    assert!(
        toggle_resp.status().is_success(),
        "attendance toggle should succeed, got {}",
        toggle_resp.status()
    );

    // Reload the attendance grid and verify the cell changed to absent.
    let resp = client
        .get(format!("{base}/team/attendance"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_after = resp.text().await.unwrap();

    // Find the specific cell by its data attributes and check it now has bg-red-400.
    let cell_fragment = extract_cell_fragment(&body_after, rower_id, practice_id)
        .expect("cell should still exist after toggle");
    assert!(
        cell_fragment.contains("bg-red-400"),
        "cell for rower {rower_id} / practice {practice_id} should be red after toggle to No, got: {cell_fragment}"
    );
}

/// Search the HTML for a cell element that has both `data-rower` and
/// `data-practice` attributes and contains `bg-emerald-400`.
/// Returns `(rower_id, practice_id)`.
fn find_available_cell(html: &str) -> Option<(i32, i32)> {
    let mut search_from = 0;
    while let Some(pos) = html[search_from..].find("data-rower=\"") {
        let abs_pos = search_from + pos;
        let after_attr = abs_pos + "data-rower=\"".len();
        search_from = after_attr;

        let rower_str = html[after_attr..].split('"').next()?;
        let rower_id: i32 = match rower_str.parse() {
            Ok(id) => id,
            Err(_) => continue,
        };

        // Get the full <td element by looking backwards.
        let td_start = html[..abs_pos].rfind("<td")?;
        let td_end = html[abs_pos..]
            .find("</td>")
            .map(|i| abs_pos + i + 5)
            .unwrap_or(html.len().min(abs_pos + 500));
        let element = &html[td_start..td_end];

        // Check this element has data-practice and bg-emerald-400.
        if !element.contains("bg-emerald-400") {
            continue;
        }
        if let Some(rest) = element.split("data-practice=\"").nth(1) {
            let practice_str = rest.split('"').next()?;
            if let Ok(practice_id) = practice_str.parse::<i32>() {
                return Some((rower_id, practice_id));
            }
        }
    }
    None
}

/// Extract the full `<td ...>` element for a specific cell identified by
/// rower_id and practice_id. Looks backwards from the `data-rower` marker
/// to capture the opening `<td` tag (which contains the class).
fn extract_cell_fragment(html: &str, rower_id: i32, practice_id: i32) -> Option<String> {
    let rower_marker = format!("data-rower=\"{rower_id}\"");
    let practice_marker = format!("data-practice=\"{practice_id}\"");

    let mut search_from = 0;
    while let Some(pos) = html[search_from..].find(&rower_marker) {
        let abs_pos = search_from + pos;
        search_from = abs_pos + rower_marker.len();

        // Check that this element also has the matching practice_id nearby.
        let end = html.len().min(abs_pos + 500);
        let after = &html[abs_pos..end];
        if !after.contains(&practice_marker) {
            continue;
        }

        // Walk backwards to find the opening `<td` for this element.
        let start = html[..abs_pos].rfind("<td").unwrap_or(abs_pos);
        // Walk forward to find the closing `</td>` or next `<td`.
        let close = html[abs_pos..]
            .find("</td>")
            .map(|i| abs_pos + i + 5)
            .unwrap_or(end);

        return Some(html[start..close].to_string());
    }
    None
}
