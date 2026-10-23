package cli_test

import (
	"backlog-cli/internal/cli"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func createArgs() []string {
	return []string{"issue", "create", "-p42", "--issue-type-id=7", "--priority-id=3", "-t件名", "--json"}
}

func TestExecuteIssueCreateDisconnectIsUnknownWithoutRetry(t *testing.T) {
	var calls atomic.Int32
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		if r.Method != "POST" {
			t.Error("unexpected non-POST")
		}
		connection, _, err := w.(http.Hijacker).Hijack()
		if err != nil {
			t.Error(err)
			return
		}
		_ = connection.Close()
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.ExecuteWithInput(context.Background(), createArgs(), unreadableInput{t}, &stdout, &stderr, server.Client())
	if exit != 1 || calls.Load() != 1 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"state":"unknown"`) || !strings.Contains(stderr.String(), `"httpStatus":null`) {
		t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls.Load(), &stdout, &stderr)
	}
}

func TestExecuteIssueCreateCancellationPreservesCertainty(t *testing.T) {
	for _, stage := range []string{"before POST", "after POST", "after 201", "timeout"} {
		t.Run(stage, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if stage == "timeout" {
				var timeoutCancel context.CancelFunc
				ctx, timeoutCancel = context.WithTimeout(ctx, 100*time.Millisecond)
				defer timeoutCancel()
			}
			if stage == "before POST" {
				cancel()
			}
			var calls atomic.Int32
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls.Add(1)
				_ = r.ParseForm()
				if stage == "after 201" {
					w.Header().Set("Location", "/view/APP-9")
					w.WriteHeader(201)
					_, _ = fmt.Fprint(w, `{`)
					w.(http.Flusher).Flush()
				}
				if stage == "after POST" {
					cancel()
				}
				<-r.Context().Done()
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			client := server.Client()
			if stage == "after 201" {
				client.Transport = cancelAfterResponseTransport{base: client.Transport, cancel: cancel}
			}
			exit := cli.ExecuteWithInput(ctx, createArgs(), unreadableInput{t}, &stdout, &stderr, client)
			state, wantCalls, wantExit := "unknown", int32(1), 130
			if stage == "before POST" {
				state, wantCalls = "not_created", 0
			}
			if stage == "after 201" {
				state = "created"
			}
			if stage == "timeout" {
				wantExit = 1
			}
			if exit != wantExit || calls.Load() != wantCalls || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"state":"`+state+`"`) {
				t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls.Load(), &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueCreateOutputFailureIsStillCreated(t *testing.T) {
	for _, jsonMode := range []bool{false, true} {
		t.Run(fmt.Sprint(jsonMode), func(t *testing.T) {
			calls := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls++
				w.WriteHeader(201)
				_, _ = fmt.Fprint(w, `{"issueKey":"APP-1"}`)
			}))
			defer server.Close()
			configure(t, server.URL)
			var stderr bytes.Buffer
			args := createArgs()
			if !jsonMode {
				args = args[:len(args)-1]
			}
			exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, failingWriter{}, &stderr, server.Client())
			if exit != 3 || calls != 1 || !strings.Contains(stderr.String(), "created") || !strings.Contains(stderr.String(), "APP-1") || !strings.Contains(stderr.String(), server.URL+"/view/APP-1") {
				t.Fatalf("exit=%d calls=%d stderr=%s", exit, calls, &stderr)
			}
		})
	}
}

type cancelOnWrite struct{ cancel context.CancelFunc }

func (w cancelOnWrite) Write(data []byte) (int, error) { w.cancel(); return len(data), nil }

func TestExecuteIssueCreateCancellationDuringOutputWinsOverSuccess(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(201)
		_, _ = fmt.Fprint(w, `{"issueKey":"APP-1"}`)
	}))
	defer server.Close()
	configure(t, server.URL)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	var stderr bytes.Buffer
	exit := cli.ExecuteWithInput(ctx, createArgs(), unreadableInput{t}, cancelOnWrite{cancel}, &stderr, server.Client())
	if exit != 130 || !strings.Contains(stderr.String(), `"code":"cancelled"`) || !strings.Contains(stderr.String(), `"state":"created"`) {
		t.Fatalf("exit=%d stderr=%s", exit, &stderr)
	}
}

func TestExecuteIssueCreateCertaintyAndStatus(t *testing.T) {
	for _, tc := range []struct {
		name              string
		status            int
		body, state, code string
		exit              int
		truncated         bool
		location          string
	}{
		{"rejected", 403, `{"errors":[{"code":11,"message":"test-secret+/?日本","future":true}]}`, "not_created", "api_error", 1, false, ""},
		{"rate limited", 429, `{}`, "not_created", "api_error", 1, false, ""},
		{"server failed", 500, `{}`, "unknown", "api_error", 1, false, ""},
		{"unexpected success", 200, `{"issueKey":"APP-2"}`, "unknown", "response_error", 1, false, ""},
		{"redirect", 302, `{}`, "unknown", "response_error", 1, false, "https://elsewhere.invalid/view/APP-2"},
		{"broken JSON", 201, `{broken`, "created", "response_error", 3, false, "/view/APP-2"},
		{"no identity", 201, `{broken`, "created", "response_error", 3, false, ""},
		{"null object", 201, `null`, "created", "response_error", 3, false, ""},
		{"array", 201, `[]`, "created", "response_error", 3, false, ""},
		{"missing key", 201, `{}`, "created", "response_error", 3, false, ""},
		{"wrong key type", 201, `{"issueKey":1}`, "created", "response_error", 3, false, ""},
		{"invalid key", 201, `{"issueKey":"../bad"}`, "created", "response_error", 3, false, "https://elsewhere.invalid/view/APP-2"},
		{"body read failed", 201, `{`, "created", "transport_error", 3, true, "/view/APP-2"},
		{"key despite read failure", 201, `{"issueKey":"APP-2"}`, "created", "transport_error", 3, true, ""},
		{"rejection body read failed", 403, `{`, "not_created", "transport_error", 1, true, ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			calls := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls++
				if r.Method != "POST" {
					t.Error("unexpected GET/retry")
				}
				if tc.truncated {
					w.Header().Set("Content-Length", "1000")
				}
				w.Header().Set("Location", tc.location)
				w.WriteHeader(tc.status)
				_, _ = fmt.Fprint(w, tc.body)
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), createArgs(), unreadableInput{t}, &stdout, &stderr, server.Client())
			var got struct {
				Error struct {
					Code       string
					HTTPStatus int
					Creation   struct {
						State         string
						IssueKey, URL *string
					}
					BacklogErrors []json.RawMessage
				}
			}
			if json.Unmarshal(stderr.Bytes(), &got) != nil || exit != tc.exit || calls != 1 || stdout.Len() != 0 || got.Error.Code != tc.code || got.Error.HTTPStatus != tc.status || got.Error.Creation.State != tc.state || strings.Contains(stderr.String(), "test-secret") {
				t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls, &stdout, &stderr)
			}
			if tc.location == "/view/APP-2" || tc.name == "key despite read failure" {
				if got.Error.Creation.IssueKey == nil || *got.Error.Creation.IssueKey != "APP-2" || got.Error.Creation.URL == nil || *got.Error.Creation.URL != server.URL+"/view/APP-2" {
					t.Fatalf("missing location identity: %s", &stderr)
				}
			} else if tc.name == "invalid key" && (got.Error.Creation.IssueKey != nil || got.Error.Creation.URL != nil) {
				t.Fatalf("unsafe identity: %s", &stderr)
			}
			if tc.status == 403 && !tc.truncated && (len(got.Error.BacklogErrors) != 1 || !strings.Contains(string(got.Error.BacklogErrors[0]), `"future":true`)) {
				t.Fatalf("lost API errors: %s", &stderr)
			}
		})
	}
}
