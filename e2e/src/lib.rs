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
/// Returns the directory containing `50_mesa.json` so libglvnd can load
/// Mesa's software EGL (llvmpipe) on headless CI runners.
fn find_mesa_egl_vendor_dir() -> String {
    // Check if the system already has a vendor dir set.
    if let Ok(dir) = std::env::var("__EGL_VENDOR_LIBRARY_DIRS") {
        if std::path::Path::new(&dir).exists() {
            return dir;
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
                return vendor_dir.to_string_lossy().into_owned();
            }
        }
    }
    // Fallback — let libglvnd use its default search.
    String::new()
}

/// Pick a free display number for Xvfb (99 + port-based offset to avoid collisions).
fn xvfb_display(port: u16) -> u32 {
    99 + (port as u32 % 900)
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
        let app_port = available_port();
        let driver_port = available_port();
        let display_num = xvfb_display(app_port);
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
        // On headless CI runners (no GPU), WebKitGTK needs Mesa's software
        // EGL implementation (llvmpipe). We point libglvnd at Mesa's vendor
        // JSON so it can find libEGL_mesa.so, and force software rendering.
        let mesa_egl_dir = find_mesa_egl_vendor_dir();
        let webdriver = unsafe {
            Command::new("WebKitWebDriver")
                .arg("-p")
                .arg(driver_port.to_string())
                .env("DISPLAY", &display)
                // Tell libglvnd where to find Mesa's EGL vendor JSON so
                // WebKit can create an EGL display via the software renderer.
                // Without this, EGL initialization fails with EGL_BAD_PARAMETER
                // and WebProcess crashes.
                // Ref: https://github.com/NVIDIA/libglvnd
                .env("__EGL_VENDOR_LIBRARY_DIRS", &mesa_egl_dir)
                // Force Mesa to use its software rasterizer (llvmpipe) instead
                // of attempting hardware DRI access.
                // Ref: https://docs.mesa3d.org/envvars.html
                .env("LIBGL_ALWAYS_SOFTWARE", "1")
                // Disable the DMA-BUF renderer introduced in WebKitGTK 2.46+
                // which requires kernel DRM/GPU access.
                // Ref: https://trac.webkit.org/wiki/EnvironmentVariables
                .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
                .stdout(std::process::Stdio::inherit())
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
