package backlog_test

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/output"
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestIssueFetchesOneEscapedPathSegmentAndPreservesObject(t *testing.T) {
	for _, tc := range []struct{ input, path string }{
		{"123", "/api/v2/issues/123"},
		{"APP_2-12", "/api/v2/issues/APP_2-12"},
		{"APP/日本?#-1", "/api/v2/issues/APP%2F%E6%97%A5%E6%9C%AC%3F%23-1"},
	} {
		t.Run(tc.input, func(t *testing.T) {
			requests := 0
			const body = `{"issueKey":"APP-12","future":{"number":12345678901234567890},"description":"日本語\u001b[31m"}`
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				requests++
				if r.Method != "GET" || r.URL.EscapedPath() != tc.path || r.URL.RawQuery != "" || r.Header.Get("Backlog-API-Key") != "test-key" {
					t.Errorf("incorrect request: %s %s", r.Method, r.URL.RequestURI())
				}
				_, _ = fmt.Fprint(w, body)
			}))
			defer server.Close()
			client, err := backlog.New(server.URL, "test-key", server.Client())
			if err != nil {
				t.Fatal(err)
			}
			item, err := client.Issue(context.Background(), tc.input)
			if err != nil || string(item) != body || requests != 1 {
				t.Fatalf("item=%s err=%v requests=%d", item, err, requests)
			}
		})
	}
}

func TestIssueRejectsNonObjectOrInvalidJSON(t *testing.T) {
	for _, body := range []string{"", " \n", "null", "[]", `"text"`, "1", "{broken", `{"issueKey":"APP-1"}{}`} {
		t.Run(body, func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				_, _ = fmt.Fprint(w, body)
			}))
			defer server.Close()
			client, err := backlog.New(server.URL, "test-key", server.Client())
			if err != nil {
				t.Fatal(err)
			}
			item, err := client.Issue(context.Background(), "APP-1")
			var failure *output.Failure
			if item != nil || !errors.As(err, &failure) || failure.Code != "response_error" || failure.ExitCode() != 1 {
				t.Fatalf("item=%s err=%v", item, err)
			}
		})
	}
}
