package cli_test

import (
	"backlog-cli/internal/cli"
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestExecuteIssueViewFormattingErrorsNeverPrintBody(t *testing.T) {
	for _, tc := range []struct {
		name, issue, project string
		status, requests     int
	}{
		{"unknown", `{"issueKey":"APP-1","projectId":42,"description":"PRIVATE BODY"}`, `{"textFormattingRule":"future"}`, 200, 2},
		{"backlog", `{"issueKey":"APP-1","projectId":42,"description":"PRIVATE BODY"}`, `{"textFormattingRule":"backlog"}`, 200, 2},
		{"missing rule", `{"issueKey":"APP-1","projectId":42}`, `{}`, 200, 2},
		{"invalid rule", `{"issueKey":"APP-1","projectId":42}`, `{"textFormattingRule":1}`, 200, 2},
		{"invalid project", `{"issueKey":"APP-1","projectId":42}`, `null`, 200, 2},
		{"denied project", `{"issueKey":"APP-1","projectId":42}`, `{"errors":[]}`, 403, 2},
		{"no project ID", `{"issueKey":"APP-1"}`, `{}`, 200, 1},
		{"invalid project ID", `{"issueKey":"APP-1","projectId":"42"}`, `{}`, 200, 1},
		{"unsafe response key", `{"issueKey":"../APP-1","projectId":42}`, `{}`, 200, 1},
		{"invalid description", `{"issueKey":"APP-1","projectId":42,"description":42}`, `{}`, 200, 1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			requests := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				requests++
				switch r.URL.Path {
				case "/api/v2/issues/APP-1":
					_, _ = fmt.Fprint(w, tc.issue)
				case "/api/v2/projects/42":
					w.WriteHeader(tc.status)
					_, _ = fmt.Fprint(w, tc.project)
				default:
					t.Errorf("unexpected API %s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), []string{"issue", "view", "APP-1"}, unreadableInput{t}, &stdout, &stderr, server.Client())
			if exit != 1 || requests != tc.requests || stdout.Len() != 0 || stderr.Len() == 0 || strings.Contains(stderr.String(), "PRIVATE BODY") {
				t.Fatalf("exit=%d requests=%d stdout=%q stderr=%q", exit, requests, &stdout, &stderr)
			}
			if (tc.name == "unknown" || tc.name == "backlog") && !strings.Contains(stderr.String(), "--json") {
				t.Fatalf("no JSON guidance: %s", &stderr)
			}
		})
	}
}

func TestExecuteIssueViewMarkdownEmptyBodyAndOutputFailure(t *testing.T) {
	for _, fail := range []bool{false, true} {
		t.Run(fmt.Sprint(fail), func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				switch r.URL.Path {
				case "/api/v2/issues/APP-1":
					_, _ = fmt.Fprint(w, `{"issueKey":"APP-1","projectId":42,"description":null}`)
				case "/api/v2/projects/42":
					_, _ = fmt.Fprint(w, `{"textFormattingRule":"markdown"}`)
				default:
					t.Errorf("unexpected API: %s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			var writer io.Writer = &stdout
			if fail {
				writer = failingWriter{}
			}
			exit := cli.ExecuteWithInput(context.Background(), []string{"issue", "view", "APP-1", "--no-input"}, unreadableInput{t}, writer, &stderr, server.Client())
			if fail {
				if exit != 1 || !strings.Contains(stderr.String(), "could not write output") {
					t.Fatalf("exit=%d stderr=%s", exit, &stderr)
				}
			} else if exit != 0 || stderr.Len() != 0 || !strings.Contains(stdout.String(), "APP-1") {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
		})
	}
}
