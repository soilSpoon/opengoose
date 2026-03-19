use axum::response::Html;
use axum::response::IntoResponse;
use axum::http::header;

const INDEX_HTML: &str = include_str!("../static/index.html");
const PICO_CSS: &str = include_str!("../static/pico.min.css");
const ALPINE_JS: &str = include_str!("../static/alpine.min.js");

pub async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

pub async fn pico_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], PICO_CSS)
}

pub async fn alpine_js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], ALPINE_JS)
}
