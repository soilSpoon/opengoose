use axum::response::Html;

const INDEX_HTML: &str = include_str!("../static/index.html");

pub async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn index_returns_embedded_html() {
        let html = index().await;
        assert!(html.0.contains("<!DOCTYPE html>") || html.0.contains("<html"));
    }
}
