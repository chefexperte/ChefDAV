//! Entry point. On wasm this mounts the Leptos app; natively (when built by
//! `cargo leptos watch/build`) it is a tiny static server for `target/site`,
//! so the dev server and the production binary are the same thing.

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    chef_dav::reminders::register_service_worker();
    leptos::mount::mount_to_body(chef_dav::app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use axum::{routing::get, Router};
    use tower_http::services::{ServeDir, ServeFile};

    let site_root = std::env::var("CHEFDAV_SITE_ROOT").unwrap_or_else(|_| "dist".to_string());
    let addr = std::env::var("CHEFDAV_SITE_ADDR").unwrap_or_else(|_| "127.0.0.1:3100".to_string());

    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .fallback_service(
            ServeDir::new(&site_root)
                .fallback(ServeFile::new(format!("{site_root}/index.html")))
                .append_index_html_on_directories(true),
        );

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind dev server");
        println!("ChefDAV dev server serving {site_root} at http://{addr}");
        axum::serve(listener, app).await.expect("serve");
    });
}
