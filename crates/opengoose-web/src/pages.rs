use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

/// Template data for the error page.
#[derive(Template)]
#[template(path = "error.html")]
pub struct ErrorPage {
    /// `<title>` content used by the shared base template.
    pub page_title: String,
    /// Current navigation slug used by the shared base template.
    pub current_nav: String,
    /// Short status label shown in the header eyebrow (e.g. "404 Not Found").
    pub eyebrow: String,
    /// Primary heading (e.g. "Page not found").
    pub title: String,
    /// One-sentence description shown below the heading.
    pub summary: String,
    /// Optional hint to help the user recover (shown in the blue callout).
    pub hint: Option<String>,
    /// URL that the "Retry" button links to (typically the same page).
    pub retry_href: String,
    /// Technical error string shown in the collapsible `<details>`.
    pub detail: String,
    /// Emoji icon shown next to the heading.
    pub icon: String,
    /// CSS color for the eyebrow text.
    pub tone_color: String,
}

impl ErrorPage {
    pub fn not_found(path: &str) -> Self {
        Self {
            page_title: "Page not found".into(),
            current_nav: String::new(),
            eyebrow: "404 Not Found".into(),
            title: "Page not found".into(),
            summary: "The requested resource could not be located on this server.".into(),
            hint: Some(
                "Check that the URL is correct. If you followed a link, it may be outdated."
                    .into(),
            ),
            retry_href: path.to_string(),
            detail: format!("GET {path} → 404 Not Found"),
            icon: "🔍".into(),
            tone_color: "#fb923c".into(),
        }
    }

    #[allow(dead_code)]
    pub fn internal_error(detail: &str) -> Self {
        Self {
            page_title: "Internal error".into(),
            current_nav: String::new(),
            eyebrow: "500 Internal Server Error".into(),
            title: "Something went wrong".into(),
            summary: "An unexpected error occurred while processing your request. The OpenGoose runtime may be experiencing issues.".into(),
            hint: Some("Try refreshing the page. If the problem persists, check the server logs.".into()),
            retry_href: "/".into(),
            detail: detail.to_string(),
            icon: "⚠️".into(),
            tone_color: "#f87171".into(),
        }
    }
}

impl IntoResponse for ErrorPage {
    fn into_response(self) -> Response {
        let status = if self.eyebrow.starts_with("404") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };

        match self.render() {
            Ok(html) => (status, Html(html)).into_response(),
            Err(_) => (status, "An error occurred.").into_response(),
        }
    }
}

/// Axum fallback handler — returns a 404 HTML error page for any unmatched route.
pub async fn not_found_handler(uri: axum::http::Uri) -> impl IntoResponse {
    ErrorPage::not_found(uri.path())
}
