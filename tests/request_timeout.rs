mod common;
use common::Harness;
use serde_json::{Value, json};
use std::process::Output;
use std::time::{Duration, Instant};

#[test]
fn body_timeouts_preserve_status_and_write_certainty_in_parallel() {
    let cases: [(&str, fn()); 6] = [
        (
            "project_list::body_reading_is_inside_the_thirty_second_request_timeout",
            body_reading_is_inside_the_thirty_second_request_timeout,
        ),
        (
            "issue_create::create_body_timeout_retains_created_without_retry",
            create_body_timeout_retains_created_without_retry,
        ),
        (
            "issue_update::update_body_timeout_keeps_updated_without_retry",
            update_body_timeout_keeps_updated_without_retry,
        ),
        (
            "comment_update::success_body_timeout_retains_updated_without_retry",
            success_body_timeout_retains_updated_without_retry,
        ),
        (
            "comment_add::body_timeout_keeps_created_without_retry_for_post",
            body_timeout_keeps_created_without_retry_for_post,
        ),
        (
            "comment_add::body_timeout_keeps_updated_without_retry_for_patch",
            body_timeout_keeps_updated_without_retry_for_patch,
        ),
    ];
    std::thread::scope(|scope| {
        let handles: Vec<_> = cases
            .into_iter()
            .map(|(name, case)| (name, scope.spawn(case)))
            .collect();
        let mut failed = Vec::new();
        for (name, handle) in handles {
            if handle.join().is_err() {
                eprintln!("FAILED: {name}");
                failed.push(name);
            } else {
                eprintln!("passed: {name}");
            }
        }
        assert!(failed.is_empty(), "timeout cases failed: {failed:?}");
    });
}

fn timed_run(h: &Harness, args: &[&str]) -> Output {
    let start = Instant::now();
    let output = h.run(args);
    let elapsed = start.elapsed();
    eprintln!("{args:?}: CLI elapsed {elapsed:?}");
    assert!(elapsed >= Duration::from_secs(29), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(40), "{elapsed:?}");
    output
}

fn write_failure(output: &Output, operation: &str, outcome: &str, status: u16) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["error"]["kind"], "network", "{value}");
    assert_eq!(
        value["error"]["write"],
        json!({"operation":operation,"outcome":outcome}),
        "{value}"
    );
    assert_eq!(value["error"]["httpStatus"], status, "{value}");
    assert!(value["error"]["apiErrors"].is_array());
    assert_eq!(output.stderr.last(), Some(&b'\n'));
}

fn assert_requests(h: &Harness, expected: &[(&str, &str, Value)]) {
    let requests = h.requests();
    assert_eq!(
        requests.len(),
        expected.len(),
        "no retry, extra GET, or fallback"
    );
    for (request, (method, path, form)) in requests.iter().zip(expected) {
        assert_eq!(request["method"], *method);
        assert_eq!(request["path"], *path);
        assert_eq!(request["form"], *form);
        assert_eq!(request["headers"]["backlog-api-key"], "test-secret");
    }
}

fn body_reading_is_inside_the_thirty_second_request_timeout() {
    let h = Harness::new(json!({"body":"[]", "delay":32}));
    let error = common::failure(&timed_run(&h, &["project", "list", "--json"]), 1, "network");
    assert_eq!(error["error"]["httpStatus"], 200);
    assert_requests(&h, &[("GET", "/api/v2/projects", json!({}))]);
}

fn create_body_timeout_retains_created_without_retry() {
    let h = Harness::new(json!({"status":201,"body":"{}","delay":32}));
    let args = [
        "issue",
        "create",
        "-p",
        "123",
        "-t",
        " 修正 ",
        "--issue-type-id",
        "7",
        "--priority-id",
        "3",
        "--json",
    ];
    write_failure(&timed_run(&h, &args), "issue.create", "created", 201);
    assert_requests(
        &h,
        &[(
            "POST",
            "/api/v2/issues",
            json!({"projectId":["123"],"summary":[" 修正 "],"issueTypeId":["7"],"priorityId":["3"]}),
        )],
    );
}

fn update_body_timeout_keeps_updated_without_retry() {
    let latest = json!({"projectId":42,"summary":"old","description":"old body","issueType":{"id":1},"priority":{"id":2},"status":{"id":3},"assignee":{"id":4},"resolution":null,"startDate":"2026-10-01T00:00:00Z","dueDate":null,"milestone":[{"id":5},{"id":6}]});
    let h = Harness::new(json!({"routes":{
        "GET /api/v2/issues/P-1":{"body":latest.to_string()},
        "PATCH /api/v2/issues/P-1":{"body":"{}","delay":32}
    }}));
    write_failure(
        &timed_run(&h, &["issue", "update", "P-1", "--json", "-t", "new"]),
        "issue.update",
        "updated",
        200,
    );
    assert_requests(
        &h,
        &[
            ("GET", "/api/v2/issues/P-1", json!({})),
            ("PATCH", "/api/v2/issues/P-1", json!({"summary":["new"]})),
        ],
    );
}

fn success_body_timeout_retains_updated_without_retry() {
    let comment = json!({"id":7,"content":"old","createdUser":{"id":123456789012345678901234567890_u128,"name":"author"},"created":"created","updated":"updated","changeLog":null});
    let h = Harness::new(json!({"routes":{
        "GET /api/v2/issues/P-1/comments/7":{"body":comment.to_string()},
        "GET /api/v2/users/myself":{"body":r#"{"id":123456789012345678901234567890}"#},
        "PATCH /api/v2/issues/P-1/comments/7":{"body":"{}","delay":32}
    }}));
    write_failure(
        &timed_run(
            &h,
            &[
                "issue", "comment", "update", "P-1", "7", "--json", "-b", "new",
            ],
        ),
        "comment.update",
        "updated",
        200,
    );
    assert_requests(
        &h,
        &[
            ("GET", "/api/v2/issues/P-1/comments/7", json!({})),
            ("GET", "/api/v2/users/myself", json!({})),
            (
                "PATCH",
                "/api/v2/issues/P-1/comments/7",
                json!({"content":["new"]}),
            ),
        ],
    );
}

fn body_timeout_keeps_created_without_retry_for_post() {
    comment_add_body_timeout(false);
}

fn body_timeout_keeps_updated_without_retry_for_patch() {
    comment_add_body_timeout(true);
}

fn comment_add_body_timeout(patch: bool) {
    let status = if patch { 200 } else { 201 };
    let response = json!({"status":status,"body":"{}","delay":32});
    let latest = json!({"projectId":42,"status":{"id":3},"assignee":{"id":4},"resolution":null});
    let h = Harness::new(json!({"routes":{
        "GET /api/v2/issues/P-1":{"body":latest.to_string()},
        "POST /api/v2/issues/P-1/comments":response,
        "PATCH /api/v2/issues/P-1":response
    }}));
    let mut args = vec!["issue", "comment", "add", "P-1", "--json", "-b", "body"];
    if patch {
        args.extend(["--status-id", "8"]);
    }
    write_failure(
        &timed_run(&h, &args),
        "comment.add",
        if patch { "updated" } else { "created" },
        status,
    );
    if patch {
        assert_requests(
            &h,
            &[
                ("GET", "/api/v2/issues/P-1", json!({})),
                (
                    "PATCH",
                    "/api/v2/issues/P-1",
                    json!({"statusId":["8"],"comment":["body"]}),
                ),
            ],
        );
    } else {
        assert_requests(
            &h,
            &[(
                "POST",
                "/api/v2/issues/P-1/comments",
                json!({"content":["body"]}),
            )],
        );
    }
}
