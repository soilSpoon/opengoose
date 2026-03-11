use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

use super::super::router;
use super::support::{page_state, run_async, test_db, with_pages_home};

#[test]
fn page_router_get_routes_return_expected_statuses() {
    with_pages_home(|| {
        run_async(async {
            let app = router(page_state(test_db()));

            for path in [
                "/",
                "/dashboard/events",
                "/sessions",
                "/runs",
                "/agents",
                "/remote-agents",
                "/remote-agents/events",
                "/workflows",
                "/schedules",
                "/triggers",
                "/teams",
                "/queue",
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(Method::GET)
                            .uri(path)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .expect("request should be handled");

                assert_eq!(
                    response.status(),
                    StatusCode::OK,
                    "path `{path}` should render"
                );
            }
        });
    });
}

#[test]
fn page_router_post_routes_return_expected_statuses() {
    with_pages_home(|| {
        run_async(async {
            let app = router(page_state(test_db()));

            let schedule_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/schedules")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("intent=unsupported"))
                        .unwrap(),
                )
                .await
                .expect("schedule request should be handled");
            assert_eq!(schedule_response.status(), StatusCode::BAD_REQUEST);

            let team_response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/teams")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("original_name=broken&yaml=title%3A+broken"))
                        .unwrap(),
                )
                .await
                .expect("team request should be handled");
            assert_eq!(team_response.status(), StatusCode::OK);

            let trigger_response = app
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/triggers")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from("intent=unsupported"))
                        .unwrap(),
                )
                .await
                .expect("trigger request should be handled");
            assert_eq!(trigger_response.status(), StatusCode::BAD_REQUEST);
        });
    });
}
