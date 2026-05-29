use axum::Json;
use serde_json::json;

pub async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "app": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_response_contains_app_and_version() {
        let Json(body) = health_handler().await;

        assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("healthy"));
        assert_eq!(body.get("app").and_then(|v| v.as_str()), Some(env!("CARGO_PKG_NAME")));
        assert_eq!(
            body.get("version").and_then(|v| v.as_str()),
            Some(env!("CARGO_PKG_VERSION"))
        );
    }
}
