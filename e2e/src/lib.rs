use fantoccini::{Client, ClientBuilder};
pub use lineup_server::mailer::{ChannelMailer, MailMessage};
use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{sleep, Duration};

/// Finds an available TCP port by binding to port 0 and returning the assigned port.
fn available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("should bind to ephemeral port")
        .local_addr()
        .expect("should have local addr")
        .port()
}

/// Returns the project root directory (parent of the e2e crate).
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Locate Mesa's EGL vendor JSON directory by searching nix store paths
/// on `LD_LIBRARY_PATH` / `LIBGL_DRIVERS_PATH`, or falling back to a glob.
/// Returns `Some(dir)` containing `50_mesa.json` so libglvnd can load
/// Mesa's software EGL (llvmpipe) on headless CI runners.
/// Returns `None` when no vendor dir is found — callers should skip
/// setting `__EGL_VENDOR_LIBRARY_DIRS` so the system default is preserved.
fn find_mesa_egl_vendor_dir() -> Option<String> {
    // Check if the system already has a vendor dir set.
    if let Ok(dir) = std::env::var("__EGL_VENDOR_LIBRARY_DIRS") {
        if std::path::Path::new(&dir).exists() {
            return Some(dir);
        }
    }
    // Search nix store for mesa's EGL vendor JSON.
    for e in std::fs::read_dir("/nix/store")
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.contains("mesa-") && !name.contains(".drv") {
            let vendor_dir = e.path().join("share/glvnd/egl_vendor.d");
            if vendor_dir.join("50_mesa.json").exists() {
                return Some(vendor_dir.to_string_lossy().into_owned());
            }
        }
    }
    // Not found — don't override the system default.
    None
}

/// Pick a unique display number for Xvfb. Uses the driver port directly
/// since it's unique per test process (nextest runs each test in its own
/// process, so in-memory coordination isn't possible). Ephemeral ports
/// are typically 32768+ which is well above any real X display number.
fn xvfb_display(driver_port: u16) -> u32 {
    driver_port as u32
}

/// An isolated test instance with its own WebKitWebDriver, in-process Axum
/// server, and ephemeral databases. Runs headlessly via Xvfb.
pub struct TestInstance {
    xvfb: Child,
    webdriver: Child,
    app_port: u16,
    driver_port: u16,
    temp_dir: PathBuf,
    pub app_state: lineup_server::AppState,
}

impl TestInstance {
    /// Spawns Xvfb + WebKitWebDriver and starts the Axum server in-process
    /// with fresh ephemeral databases. Uses `LogMailer` (emails are discarded).
    pub async fn start() -> Self {
        let mailer: Arc<dyn lineup_server::mailer::Mailer> =
            Arc::new(lineup_server::mailer::LogMailer);
        Self::start_inner(mailer).await
    }

    /// Like [`start`](Self::start) but with a [`ChannelMailer`] so tests can
    /// receive and assert on sent emails. Returns the instance and the
    /// receiving half of the mail channel.
    pub async fn start_with_mail() -> (Self, UnboundedReceiver<MailMessage>) {
        let (mailer, rx) = ChannelMailer::new();
        let mailer: Arc<dyn lineup_server::mailer::Mailer> = Arc::new(mailer);
        (Self::start_inner(mailer).await, rx)
    }

    async fn start_inner(mailer: Arc<dyn lineup_server::mailer::Mailer>) -> Self {
        // Init tracing when RUST_LOG is set so solver/server logs are
        // visible during e2e debugging. Off by default to keep test
        // output clean. Example: RUST_LOG=lineup_solver=debug
        use std::sync::Once;
        static TRACING: Once = Once::new();
        if std::env::var("RUST_LOG").is_ok() {
            TRACING.call_once(|| {
                tracing_subscriber::fmt()
                    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                    .with_test_writer()
                    .try_init()
                    .ok();
            });
        }

        let app_port = available_port();
        let driver_port = available_port();
        let display_num = xvfb_display(driver_port);
        let display = format!(":{display_num}");

        // Create a temp directory for this test's databases.
        let temp_dir = std::env::temp_dir().join(format!("lineup_e2e_{app_port}"));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("data/demos")).expect("should create temp data dir");

        let master_db = temp_dir.join("master.db").to_string_lossy().into_owned();
        let data_dir = temp_dir.join("data").to_string_lossy().into_owned();
        let public_dir = project_root()
            .join("crates/server/public")
            .to_string_lossy()
            .into_owned();

        // Start the Axum server in-process.
        let (router, app_state) =
            lineup_server::build_router(&master_db, &data_dir, &public_dir, mailer)
                .expect("should build router");

        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{app_port}"))
            .await
            .expect("should bind to app port");
        tokio::spawn(async move {
            axum::serve(listener, router.into_make_service())
                .await
                .unwrap();
        });

        // Spawn Xvfb for headless rendering.
        let xvfb = unsafe {
            Command::new("Xvfb")
                .arg(&display)
                .arg("-screen")
                .arg("0")
                .arg("1280x1024x24")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .pre_exec(|| {
                    libc::setpgid(0, 0);
                    Ok(())
                })
                .spawn()
                .expect("Xvfb must be installed")
        };

        // Give Xvfb a moment to start.
        sleep(Duration::from_millis(200)).await;

        // Spawn WebKitWebDriver on the virtual display.
        //
        // On headless CI runners, WebKitGTK needs Mesa's software EGL
        // implementation (llvmpipe). We detect CI by checking for
        // LIBGL_ALWAYS_SOFTWARE=1 (set in the CI workflow) — /dev/dri
        // can exist on CI runners even when the GPU isn't usable.
        let headless = std::env::var("LIBGL_ALWAYS_SOFTWARE").as_deref() == Ok("1");
        let mut cmd = Command::new("WebKitWebDriver");
        cmd.arg("-p")
            .arg(driver_port.to_string())
            .env("DISPLAY", &display);

        if headless {
            // Tell libglvnd where to find Mesa's EGL vendor JSON so
            // WebKit can create an EGL display via the software renderer.
            // Only set this when we actually find the directory — setting
            // it to an empty string *breaks* EGL vendor discovery.
            if let Some(ref mesa_egl_dir) = find_mesa_egl_vendor_dir() {
                eprintln!("[e2e] Using Mesa EGL vendor dir: {mesa_egl_dir}");
                cmd.env("__EGL_VENDOR_LIBRARY_DIRS", mesa_egl_dir);
            } else {
                eprintln!("[e2e] WARNING: Could not find Mesa EGL vendor dir in /nix/store");
            }
            cmd
                // Force Mesa to use its software rasterizer (llvmpipe).
                .env("LIBGL_ALWAYS_SOFTWARE", "1")
                // Disable the DMA-BUF renderer (requires kernel DRM access).
                .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            eprintln!(
                "[e2e] Headless mode: LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1"
            );
        } else {
            eprintln!("[e2e] GPU detected (/dev/dri exists), using native rendering");
        }

        let webdriver = unsafe {
            cmd.stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .pre_exec(|| {
                    libc::setpgid(0, 0);
                    Ok(())
                })
                .spawn()
                .expect("WebKitWebDriver must be installed (add webkitgtk_4_1 to flake.nix)")
        };

        // Wait for WebKitWebDriver to accept connections.
        let driver_url = format!("http://localhost:{driver_port}/status");
        let http = reqwest::Client::new();
        let mut ready = false;
        for _ in 0..20 {
            sleep(Duration::from_millis(250)).await;
            if http.get(&driver_url).send().await.is_ok() {
                ready = true;
                break;
            }
        }
        if !ready {
            panic!("WebKitWebDriver did not become ready on port {driver_port} within 5s");
        }

        Self {
            xvfb,
            webdriver,
            app_port,
            driver_port,
            temp_dir,
            app_state,
        }
    }

    /// Connects a fantoccini client to WebKitWebDriver and bootstraps a demo
    /// tenant by posting to `/demo`. Returns an authenticated client on `/practices`.
    pub async fn connect(&self) -> Client {
        let client = ClientBuilder::native()
            .connect(&format!("http://localhost:{}", self.driver_port))
            .await
            .expect("should connect to WebKitWebDriver");

        // Set a consistent window size.
        let _ = client.set_window_size(1280, 1024).await;

        // Create a demo tenant by submitting a form. This is more reliable
        // than fetch() because the browser handles the redirect natively
        // and correctly stores the Set-Cookie headers.
        client
            .goto(&format!("http://localhost:{}/login", self.app_port))
            .await
            .unwrap();

        // Inject and submit a form that POSTs to /demo.
        client
            .execute(
                &format!(
                    r#"
                    var form = document.createElement('form');
                    form.method = 'POST';
                    form.action = 'http://localhost:{}/demo';
                    document.body.appendChild(form);
                    form.submit();
                    "#,
                    self.app_port
                ),
                vec![],
            )
            .await
            .unwrap();

        // Wait for the form submit + redirect to /practices to complete.
        for _ in 0..40 {
            sleep(Duration::from_millis(250)).await;
            if let Ok(url) = client.current_url().await {
                if url.path() == "/practices" {
                    break;
                }
            }
        }

        client
    }

    pub fn base_url(&self) -> String {
        format!("http://localhost:{}", self.app_port)
    }

    /// Path to the master (tenant registry) SQLite database.
    /// Useful for tests that need to modify tenant billing status.
    pub fn master_db_path(&self) -> String {
        self.temp_dir
            .join("master.db")
            .to_string_lossy()
            .into_owned()
    }

    /// Refresh all cached tenant configs from the master DB without
    /// dropping DB connections. Call after modifying tenant records
    /// (e.g. billing status) so the server picks up the changes.
    pub async fn refresh_tenant_configs(&self) {
        self.app_state.refresh_tenant_configs().await;
    }
}

/// Sets a `<select>` element's value via JavaScript, bypassing visibility
/// constraints that cause `ElementNotInteractable` in WebKitWebDriver.
pub async fn select_value(client: &Client, css_selector: &str, value: &str) {
    let js = format!(
        r#"document.querySelector("{}").value = "{}";"#,
        css_selector, value
    );
    client.execute(&js, vec![]).await.unwrap();
}

/// Scrolls an element into view and clicks it. Works around
/// `ElementNotInteractable` errors when elements are off-screen.
pub async fn scroll_and_click(client: &Client, css_selector: &str) {
    let js = format!(
        r#"var el = document.querySelector("{}"); el.scrollIntoView(); el.click();"#,
        css_selector
    );
    client.execute(&js, vec![]).await.unwrap();
}

/// Sets an input element's value via JavaScript, clearing it first.
/// Works around `ElementNotInteractable` for off-screen inputs.
pub async fn set_input_value(client: &Client, css_selector: &str, value: &str) {
    let js = format!(
        r#"var el = document.querySelector("{}"); el.scrollIntoView(); el.value = "{}"; el.dispatchEvent(new Event("input", {{ bubbles: true }}));"#,
        css_selector, value
    );
    client.execute(&js, vec![]).await.unwrap();
}

// ── Plan editor helpers ──────────────────────────────────────────────

pub mod plan {
    use fantoccini::{Client, Locator};
    use std::time::Duration;

    /// Navigate to the plan templates list page.
    pub async fn goto_templates(client: &Client, base_url: &str) {
        client
            .goto(&format!("{base_url}/admin/plan-templates"))
            .await
            .unwrap();
        client
            .wait()
            .at_most(Duration::from_secs(5))
            .for_element(Locator::XPath("//*[contains(text(), 'Plan templates')]"))
            .await
            .expect("expected plan templates page");
    }

    /// Click "New template" and wait for the detail page to load.
    /// Returns the full URL of the template detail page.
    pub async fn create_template(client: &Client) -> String {
        click_button(client, "New template").await;
        client
            .wait()
            .at_most(Duration::from_secs(5))
            .for_element(Locator::Id("template-name"))
            .await
            .expect("expected template detail page");
        // Extract the template detail URL from the page (meta form action or back link)
        let url: serde_json::Value = client
            .execute(
                r#"
                var link = document.querySelector('button[hx-get*="/admin/plan-templates/"][hx-get*="/detail"]');
                if (link) return link.getAttribute('hx-get').replace('/detail', '');
                // Fallback: extract from any form action or hx-post
                var form = document.querySelector('form[hx-post*="/admin/plan-templates/"]');
                if (form) {
                    var m = form.getAttribute('hx-post').match(/\/admin\/plan-templates\/(\d+)/);
                    if (m) return '/admin/plan-templates/' + m[1];
                }
                return '';
                "#,
                vec![],
            )
            .await
            .unwrap();
        let path = url.as_str().unwrap_or("").to_string();
        let base = client.current_url().await.unwrap();
        format!(
            "{}://{}{}/detail?plan_editor=open_preview",
            base.scheme(),
            base.host_str().unwrap_or("localhost"),
            path
        )
    }

    /// Click "edit plan" to open the timeline editor.
    /// Waits for palette buttons to appear.
    pub async fn open_editor(client: &Client) {
        // Check if the editor is already open (palette has submit buttons)
        let palette_count: u64 = client
            .execute(
                r#"return document.querySelectorAll('#timeline-section button[type="submit"]').length"#,
                vec![],
            )
            .await
            .map(|v| v.as_u64().unwrap_or(0))
            .unwrap_or(0);
        if palette_count == 0 {
            click_button(client, "edit plan").await;
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    /// Click a palette button to add a block/group by its label text.
    pub async fn add_item(client: &Client, label: &str) {
        // Find the button in the palette and click it via JS + HTMX
        let js = format!(
            r#"
            var btns = document.querySelectorAll('#timeline-section button[type="submit"]');
            for (var b of btns) {{
                if (b.textContent.trim() === '{}') {{ b.click(); return true; }}
            }}
            return false;
            "#,
            label
        );
        let clicked = client.execute(&js, vec![]).await.unwrap();
        assert_eq!(
            clicked.as_bool(),
            Some(true),
            "palette button '{label}' not found"
        );
        wait_for_htmx(client).await;
    }

    /// Click a button by its visible text. Tries exact match first,
    /// then falls back to starts-with match for longer button labels.
    pub async fn click_button(client: &Client, text: &str) {
        let js = format!(
            r#"
            var btns = document.querySelectorAll('button, [type="submit"]');
            var needle = '{}';
            // Exact match first
            for (var b of btns) {{
                if (b.textContent.trim() === needle) {{ b.scrollIntoView(); b.click(); return true; }}
            }}
            // Starts-with fallback
            for (var b of btns) {{
                if (b.textContent.trim().startsWith(needle)) {{ b.scrollIntoView(); b.click(); return true; }}
            }}
            return false;
            "#,
            text.replace('\'', "\\'")
        );
        let clicked = client.execute(&js, vec![]).await.unwrap();
        assert_eq!(clicked.as_bool(), Some(true), "button '{text}' not found");
        wait_for_htmx(client).await;
    }

    /// Wait for any in-flight HTMX requests to settle.
    pub async fn wait_for_htmx(client: &Client) {
        // Brief sleep to let HTMX fire, then poll for completion
        tokio::time::sleep(Duration::from_millis(100)).await;
        for _ in 0..50 {
            let pending: serde_json::Value = client
                .execute(
                    "return document.querySelectorAll('.htmx-request').length",
                    vec![],
                )
                .await
                .unwrap();
            if pending.as_u64().unwrap_or(0) == 0 {
                // Extra settle time for DOM updates
                tokio::time::sleep(Duration::from_millis(50)).await;
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("HTMX requests did not settle within 5s");
    }

    /// Count elements matching a CSS selector.
    pub async fn count(client: &Client, css: &str) -> u64 {
        let js = format!(
            r#"return document.querySelectorAll("{}").length"#,
            css.replace('"', r#"\""#)
        );
        let val = client.execute(&js, vec![]).await.unwrap();
        val.as_u64().unwrap_or(0)
    }

    /// Get the page source text content (for simple assertions).
    pub async fn page_text(client: &Client) -> String {
        client.source().await.unwrap()
    }

    /// Click an element in the strip by its data-tl-id.
    pub async fn click_strip_item(client: &Client, index: usize) {
        let js = format!(
            r#"
            var items = document.querySelectorAll('#tl-strip [data-tl-id]');
            if (items.length > {index}) {{ items[{index}].click(); return true; }}
            return false;
            "#
        );
        let clicked = client.execute(&js, vec![]).await.unwrap();
        assert_eq!(
            clicked.as_bool(),
            Some(true),
            "strip item at index {index} not found"
        );
        wait_for_htmx(client).await;
    }
}

impl Drop for TestInstance {
    fn drop(&mut self) {
        // Send SIGTERM to the WebKitWebDriver process group first so it
        // can shut down child browser processes gracefully.
        let pid = self.webdriver.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        // Brief grace period for cleanup, then force-kill.
        std::thread::sleep(std::time::Duration::from_millis(500));
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
        let _ = self.webdriver.wait();

        // Kill the Xvfb process group.
        let xvfb_pid = self.xvfb.id() as i32;
        unsafe {
            libc::kill(-xvfb_pid, libc::SIGTERM);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        unsafe {
            libc::kill(-xvfb_pid, libc::SIGKILL);
        }
        let _ = self.xvfb.wait();

        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}
