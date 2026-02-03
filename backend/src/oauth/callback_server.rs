use crate::errors::AppError;
use crate::oauth::types::OAuthAuthorizationResponse;
use axum::{
    extract::Query,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::sync::Arc;
use tokio::sync::oneshot;

/// Start a local callback server to receive OAuth authorization code
pub async fn start_callback_server(
    port: u16,
) -> Result<(String, oneshot::Receiver<Result<OAuthAuthorizationResponse, String>>), AppError> {
    let (tx, rx) = oneshot::channel();
    let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));

    let app = Router::new().route(
        "/callback",
        get({
            let tx = tx.clone();
            move |query: Query<OAuthAuthorizationResponse>| {
                handle_callback(query, tx.clone())
            }
        }),
    );

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| AppError::OAuthError {
            message: format!("Failed to bind callback server: {}", e),
        })?;

    let callback_url = format!("http://localhost:{}/callback", port);

    // Spawn server in background
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    Ok((callback_url, rx))
}

async fn handle_callback(
    Query(params): Query<OAuthAuthorizationResponse>,
    tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<Result<OAuthAuthorizationResponse, String>>>>>,
) -> impl IntoResponse {
    let mut tx_guard = tx.lock().await;
    if let Some(sender) = tx_guard.take() {
        let _ = sender.send(Ok(params));
    }

    Html(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>OAuth Authorization</title>
            <style>
                body {
                    font-family: Arial, sans-serif;
                    display: flex;
                    justify-content: center;
                    align-items: center;
                    height: 100vh;
                    margin: 0;
                    background-color: #f5f5f5;
                }
                .container {
                    text-align: center;
                    padding: 40px;
                    background: white;
                    border-radius: 8px;
                    box-shadow: 0 2px 10px rgba(0,0,0,0.1);
                }
                h1 { color: #4CAF50; }
                p { color: #666; }
            </style>
        </head>
        <body>
            <div class="container">
                <h1>✓ Authorization Successful</h1>
                <p>You can close this window and return to the terminal.</p>
            </div>
        </body>
        </html>
        "#,
    )
}
