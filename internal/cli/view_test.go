package cli_test

import (
	"backlog-cli/internal/cli"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"
)

func TestExecuteIssueViewJSONFetchesOnlyIssueAndPreservesValues(t *testing.T) {
	for _, input := range []string{"42", "APP_2-12", "9223372036854775807"} {
		t.Run(input, func(t *testing.T) {
			requests := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				requests++
				if r.Method != "GET" || r.URL.RequestURI() != "/api/v2/issues/"+input || r.Header.Get("Backlog-API-Key") != "test-secret+/?日本" {
					t.Errorf("unexpected request: %s %s", r.Method, r.URL.RequestURI())
				}
				_, _ = fmt.Fprint(w, `{"id":12345678901234567890,"issueKey":"OTHER-9","description":"* 本文\n日本語👩‍💻\r\n\t\u001b[31m\u0007 test-secret+/?日本","future":{"array":[null,true,{"未知":"値"}]},"url":"https://untrusted.test"}`)
			}))
			defer server.Close()
			configure(t, server.URL)
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()
			var stdout, stderr bytes.Buffer
			args := []string{"issue", "view", input, "--json"}
			if input == "42" {
				args = append(args, "--no-input")
			}
			exit := cli.ExecuteWithInput(ctx, args, unreadableInput{t}, &stdout, &stderr, server.Client())
			if !strings.HasSuffix(stdout.String(), "\n") {
				t.Fatalf("JSON is not newline terminated: %q", &stdout)
			}
			var result struct {
				ID          json.Number     `json:"id"`
				IssueKey    string          `json:"issueKey"`
				Description string          `json:"description"`
				Future      json.RawMessage `json:"future"`
				URL         string          `json:"url"`
			}
			decoder := json.NewDecoder(&stdout)
			if err := decoder.Decode(&result); err != nil {
				t.Fatalf("exit=%d stderr=%s decode=%v", exit, &stderr, err)
			}
			if exit != 0 || stderr.Len() != 0 || requests != 1 || result.ID.String() != "12345678901234567890" || result.IssueKey != "OTHER-9" || result.Description != "* 本文\n日本語👩‍💻\r\n\t\x1b[31m\a [REDACTED]" || string(result.Future) != `{"array":[null,true,{"未知":"値"}]}` || result.URL != server.URL+"/view/OTHER-9" {
				t.Fatalf("exit=%d requests=%d result=%+v stderr=%s", exit, requests, result, &stderr)
			}
			var extra any
			if err := decoder.Decode(&extra); err != io.EOF {
				t.Fatalf("extra stdout: %v", err)
			}
		})
	}
}

func TestExecuteIssueViewRejectsInvalidInputBeforeAPI(t *testing.T) {
	for _, args := range [][]string{
		{"issue", "view", "--json"},
		{"issue", "view", "APP-1", "extra", "--json"},
		{"issue", "view", "--json", "--unknown"},
		{"issue", "view", "APP-1", "--project", "APP", "--json"},
		{"issue", "view", "APP-1", "--profile=default", "--profile=default", "--json"},
	} {
		t.Run(fmt.Sprint(args), func(t *testing.T) {
			assertIssueViewInputError(t, args)
		})
	}
	for _, value := range []string{"", " ", "0", "-1", "9223372036854775808", "1.5", "APP", "APP-", "APP-one", "APP-1/extra", "APP-1?x=1", "APP%2F-1", ".", "..", "https://other.test/APP-1", "日本-1", "APP-1\x1b[31m"} {
		t.Run(value, func(t *testing.T) {
			assertIssueViewInputError(t, []string{"issue", "view", "--json", "--", value})
		})
	}
}

func assertIssueViewInputError(t *testing.T, args []string) {
	t.Helper()
	requests := 0
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests++
		_, _ = fmt.Fprint(w, `{"issueKey":"APP-1"}`)
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, server.Client())
	var result struct {
		Error struct {
			Code       string `json:"code"`
			HTTPStatus *int   `json:"httpStatus"`
			Creation   any    `json:"creation"`
		} `json:"error"`
	}
	if err := json.Unmarshal(stderr.Bytes(), &result); err != nil {
		t.Fatalf("invalid JSON error: %s (%v)", &stderr, err)
	}
	if exit != 2 || stdout.Len() != 0 || requests != 0 || result.Error.Code != "input_error" || result.Error.HTTPStatus != nil || result.Error.Creation != nil {
		t.Fatalf("exit=%d requests=%d stdout=%s stderr=%s", exit, requests, &stdout, &stderr)
	}
}

func TestExecuteIssueViewTextIsRejectedWithoutPrintingRawBody(t *testing.T) {
	for _, flags := range [][]string{nil, {"--no-input"}, {"--json=false"}} {
		t.Run(fmt.Sprint(flags), func(t *testing.T) {
			requests := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				requests++
				_, _ = fmt.Fprint(w, `{"issueKey":"APP-1","description":"RAW BODY\u001b[31m"}`)
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), append([]string{"issue", "view", "APP-1"}, flags...), unreadableInput{t}, &stdout, &stderr, server.Client())
			if exit != 2 || stdout.Len() != 0 || requests != 0 || !strings.Contains(stderr.String(), "--json") || strings.Contains(stderr.String(), "RAW BODY") || strings.Contains(stderr.String(), "\x1b") {
				t.Fatalf("exit=%d requests=%d stdout=%q stderr=%q", exit, requests, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueViewFailuresHaveNoSuccessOutput(t *testing.T) {
	for _, tc := range []struct {
		name   string
		status int
		body   string
		code   string
	}{
		{"not found", 404, `{"errors":[{"code":6,"message":"test-secret+/?日本","future":{"keep":true}}]}`, "api_error"},
		{"authentication", 401, `{"errors":[{"code":11,"message":"invalid key"}]}`, "api_error"},
		{"permission", 403, `{"errors":[]}`, "api_error"},
		{"rate limit", 429, `{"errors":[]}`, "api_error"},
		{"server failure", 500, `{"errors":[]}`, "api_error"},
		{"unexpected success", 201, `{"issueKey":"APP-1"}`, "response_error"},
		{"broken JSON", 200, `{broken`, "response_error"},
		{"empty body", 200, ``, "response_error"},
		{"null", 200, `null`, "response_error"},
		{"array", 200, `[{"issueKey":"APP-1"}]`, "response_error"},
		{"scalar", 200, `1`, "response_error"},
		{"trailing JSON", 200, `{"issueKey":"APP-1"}{}`, "response_error"},
		{"missing key", 200, `{"description":"本文"}`, "response_error"},
		{"numeric key", 200, `{"issueKey":123}`, "response_error"},
		{"null key", 200, `{"issueKey":null}`, "response_error"},
		{"unsafe key", 200, `{"issueKey":"../APP-1"}`, "response_error"},
		{"key URL", 200, `{"issueKey":"https://other.test/APP-1"}`, "response_error"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			requests := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				requests++
				if r.URL.Path != "/api/v2/issues/APP-1" {
					t.Errorf("unexpected request: %s", r.URL.Path)
				}
				w.WriteHeader(tc.status)
				_, _ = fmt.Fprint(w, tc.body)
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), []string{"issue", "view", "APP-1", "--json"}, unreadableInput{t}, &stdout, &stderr, server.Client())
			var result struct {
				Error struct {
					Code          string            `json:"code"`
					HTTPStatus    int               `json:"httpStatus"`
					BacklogErrors []json.RawMessage `json:"backlogErrors"`
					Creation      any               `json:"creation"`
				} `json:"error"`
			}
			if err := json.Unmarshal(stderr.Bytes(), &result); err != nil {
				t.Fatalf("invalid JSON error: %s (%v)", &stderr, err)
			}
			if exit != 1 || stdout.Len() != 0 || requests != 1 || result.Error.Code != tc.code || result.Error.HTTPStatus != tc.status || result.Error.Creation != nil || result.Error.BacklogErrors == nil || strings.Contains(stderr.String(), "test-secret") {
				t.Fatalf("exit=%d requests=%d stdout=%s stderr=%s", exit, requests, &stdout, &stderr)
			}
			if tc.status == 404 && (len(result.Error.BacklogErrors) != 1 || string(result.Error.BacklogErrors[0]) != `{"code":6,"future":{"keep":true},"message":"[REDACTED]"}`) {
				t.Fatalf("lost API error structure: %s", &stderr)
			}
		})
	}
}

func TestExecuteIssueViewTransportCancellationAndOutputErrors(t *testing.T) {
	for _, mode := range []string{"transport", "cancelled", "output"} {
		t.Run(mode, func(t *testing.T) {
			requests := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				requests++
				if mode == "transport" {
					connection, _, err := w.(http.Hijacker).Hijack()
					if err != nil {
						t.Error(err)
						return
					}
					_ = connection.Close()
					return
				}
				_, _ = fmt.Fprint(w, `{"issueKey":"APP-1"}`)
			}))
			defer server.Close()
			configure(t, server.URL)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if mode == "cancelled" {
				cancel()
			}
			var stdout, stderr bytes.Buffer
			var writer io.Writer = &stdout
			wantCode, wantExit, wantRequests, wantStatus := "transport_error", 1, 1, `"httpStatus":null`
			switch mode {
			case "output":
				writer = failingWriter{}
				wantCode, wantStatus = "output_error", `"httpStatus":200`
			case "cancelled":
				wantCode, wantExit, wantRequests = "cancelled", 130, 0
			}
			exit := cli.ExecuteWithInput(ctx, []string{"issue", "view", "42", "--json"}, unreadableInput{t}, writer, &stderr, server.Client())
			if exit != wantExit || requests != wantRequests || stdout.Len() != 0 || !json.Valid(stderr.Bytes()) || !strings.Contains(stderr.String(), `"code":"`+wantCode+`"`) || !strings.Contains(stderr.String(), wantStatus) || !strings.Contains(stderr.String(), `"creation":null`) {
				t.Fatalf("exit=%d requests=%d stdout=%s stderr=%s", exit, requests, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueViewUsesProfileOriginWithoutApplyingLocalDefaults(t *testing.T) {
	requests := 0
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests++
		if r.URL.RequestURI() != "/api/v2/issues/APP-1" || r.Header.Get("Backlog-API-Key") != "alternate-secret" {
			t.Errorf("incorrect selected profile request: %s", r.URL.RequestURI())
		}
		_, _ = fmt.Fprint(w, `{"issueKey":"APP-1"}`)
	}))
	defer server.Close()
	configure(t, "https://unused.invalid")
	t.Setenv("ALTERNATE_KEY", "alternate-secret")
	data, err := json.Marshal(map[string]any{"profiles": map[string]any{"default": map[string]string{"spaceUrl": "https://unused.invalid", "apiKeyEnv": "SLICE_TEST_KEY"}, "work": map[string]string{"spaceUrl": server.URL + "/", "apiKeyEnv": "ALTERNATE_KEY"}}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(os.Getenv("XDG_CONFIG_HOME")+"/backlog/config.json", data, 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(".backlog", 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(".backlog/settings.json", []byte(`{"project":"IGNORED","issue":{"list":{"status":["missing"]},"create":{"assignee":"missing"}}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	var stdout, stderr bytes.Buffer
	exit := cli.ExecuteWithInput(context.Background(), []string{"issue", "view", "APP-1", "--profile=work", "--json"}, unreadableInput{t}, &stdout, &stderr, server.Client())
	if exit != 0 || requests != 1 || stderr.Len() != 0 || !strings.Contains(stdout.String(), server.URL+"/view/APP-1") {
		t.Fatalf("exit=%d requests=%d stdout=%s stderr=%s", exit, requests, &stdout, &stderr)
	}
	stdout.Reset()
	if err := os.WriteFile(".backlog/settings.json", []byte(`{broken`), 0o600); err != nil {
		t.Fatal(err)
	}
	exit = cli.ExecuteWithInput(context.Background(), []string{"issue", "view", "APP-1", "--profile=work", "--json"}, unreadableInput{t}, &stdout, &stderr, server.Client())
	if exit != 2 || requests != 1 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"config_error"`) {
		t.Fatalf("exit=%d requests=%d stdout=%s stderr=%s", exit, requests, &stdout, &stderr)
	}
}
