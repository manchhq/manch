use std::sync::Arc;

use axum::{routing::get, Router as AxumRouter};
use connectrpc::Router as ConnectRouter;
use manch_server::{ManchServiceImpl, proto::manch::v1::ManchServiceExt as _};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = Arc::new(ManchServiceImpl);
    let connect = service.register(ConnectRouter::new());

    let app = AxumRouter::new()
        .route("/health", get(|| async { "OK" }))
        .fallback_service(connect.into_axum_service());

    let addr = std::env::var("MANCH_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("manch-server listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
