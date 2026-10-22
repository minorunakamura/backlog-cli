package cli_test

import (
	"backlog-cli/internal/cli"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
)

func TestExecuteHelpNeedsNoConfiguration(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	t.Setenv("HOME", t.TempDir())
	for _, args := range [][]string{{"--help"}, {"help", "project", "list"}, {"priority", "list", "--help"}} {
		var stdout, stderr bytes.Buffer
		exit := cli.Execute(context.Background(), args, &stdout, &stderr, nil)
		if exit != 0 || stderr.Len() != 0 || !strings.Contains(stdout.String(), "Usage:") {
			t.Fatalf("exit=%d stdout=%q stderr=%q", exit, &stdout, &stderr)
		}
	}
}

func configure(t *testing.T, origin string) {
	t.Helper()
	base := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", base)
	t.Setenv("SLICE_TEST_KEY", "test-secret+/?日本")
	if err := os.Mkdir(filepath.Join(base, "backlog"), 0o700); err != nil {
		t.Fatal(err)
	}
	data, err := json.Marshal(map[string]any{"profiles": map[string]any{"default": map[string]string{"spaceUrl": origin, "apiKeyEnv": "SLICE_TEST_KEY"}}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(base, "backlog", "config.json"), data, 0o600); err != nil {
		t.Fatal(err)
	}
	t.Chdir(t.TempDir())
}

func TestExecuteProjectListPreservesAPIObjects(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "GET" || r.URL.Path != "/api/v2/projects" || r.URL.RawQuery != "" || r.Header.Get("Backlog-API-Key") != "test-secret+/?日本" {
			t.Errorf("incorrect API request")
		}
		_, _ = fmt.Fprint(w, `[{"id":1,"projectKey":"APP","name":"日本語\u001b[31m","future":{"value":12345678901234567890}}]`)
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"project", "list", "--json", "--no-input"}, &stdout, &stderr, server.Client())
	want := `{"items":[{"id":1,"projectKey":"APP","name":"日本語\u001b[31m","future":{"value":12345678901234567890}}]}` + "\n"
	if exit != 0 || stderr.Len() != 0 || stdout.String() != want {
		t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
	}
}

func TestExecuteErrorsUseStderrJSONAndExitContract(t *testing.T) {
	for _, tc := range []struct {
		name   string
		args   []string
		status int
		body   string
		code   string
		exit   int
	}{
		{"unknown flag", []string{"project", "list", "--unknown", "--json"}, 200, `[]`, "input_error", 2},
		{"unknown subcommand", []string{"project", "bogus", "--json"}, 200, `[]`, "input_error", 2},
		{"unknown command", []string{"bogus", "--json"}, 200, `[]`, "input_error", 2},
		{"extra arg", []string{"project", "list", "extra", "--json"}, 200, `[]`, "input_error", 2},
		{"duplicate profile", []string{"--profile=default", "project", "list", "--profile", "default", "--json"}, 200, `[]`, "input_error", 2},
		{"missing flag value", []string{"project", "list", "--json", "--profile"}, 200, `[]`, "input_error", 2},
		{"API rejection", []string{"project", "list", "--json"}, 403, `{"errors":[{"message":"test-secret+/?日本","code":11,"more":{"keep":true}}]}`, "api_error", 1},
		{"invalid JSON", []string{"priority", "list", "--json"}, 200, `{broken`, "response_error", 1},
		{"null response", []string{"project", "list", "--json"}, 200, `null`, "response_error", 1},
		{"nonobject item", []string{"priority", "list", "--json"}, 200, `[1]`, "response_error", 1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			requests := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				requests++
				w.WriteHeader(tc.status)
				_, _ = fmt.Fprint(w, tc.body)
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(context.Background(), tc.args, &stdout, &stderr, server.Client())
			var result struct {
				Error struct {
					Code          string           `json:"code"`
					Message       string           `json:"message"`
					HTTPStatus    *int             `json:"httpStatus"`
					BacklogErrors []map[string]any `json:"backlogErrors"`
					Creation      any              `json:"creation"`
				} `json:"error"`
			}
			if err := json.Unmarshal(stderr.Bytes(), &result); err != nil {
				t.Fatalf("exit=%d invalid JSON: %s (%v)", exit, &stderr, err)
			}
			if exit != tc.exit || stdout.Len() != 0 || result.Error.Code != tc.code || result.Error.BacklogErrors == nil || result.Error.Creation != nil || strings.Contains(stderr.String(), "test-secret") {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
			if tc.code == "input_error" && requests != 0 {
				t.Fatal("input error called API")
			}
			if tc.status == 403 && (result.Error.HTTPStatus == nil || *result.Error.HTTPStatus != 403 || len(result.Error.BacklogErrors) != 1 || result.Error.BacklogErrors[0]["message"] != "[REDACTED]") {
				t.Fatalf("lost API error structure: %s", &stderr)
			}
		})
	}
}

func TestExecuteFixedListsAreSafeAndEmptyJSONIsAnArray(t *testing.T) {
	for _, tc := range []struct {
		name, body, want string
		args             []string
	}{
		{"project text", `[{"id":1,"projectKey":"APP","name":"日本語👩‍💻\r\n\t\u001b[31m赤\u001b[0m\u0007test-secret+/?日本"}]`, "ID\tKEY\tNAME\n1\tAPP\t日本語👩‍💻\n\t赤[REDACTED]\n", []string{"project", "list", "--no-input"}},
		{"priority text", `[{"id":3,"name":"中"}]`, "ID\tNAME\n3\t中\n", []string{"priority", "list"}},
		{"empty JSON", `[]`, "{\"items\":[]}\n", []string{"priority", "list", "--json"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = fmt.Fprint(w, tc.body) }))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(context.Background(), tc.args, &stdout, &stderr, server.Client())
			if exit != 0 || stdout.String() != tc.want || stderr.Len() != 0 {
				t.Fatalf("exit=%d stdout=%q stderr=%s", exit, &stdout, &stderr)
			}
		})
	}
}

type failingWriter struct{}

func (failingWriter) Write([]byte) (int, error) { return 0, errors.New("write failed") }

func TestExecuteStderrFailurePreservesCancellationExit(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	for _, tc := range []struct {
		name string
		ctx  context.Context
		exit int
	}{
		{"cancelled", ctx, 130},
		{"not cancelled", context.Background(), 1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var stdout bytes.Buffer
			exit := cli.Execute(tc.ctx, []string{"project", "--json"}, &stdout, failingWriter{}, nil)
			if exit != tc.exit || stdout.Len() != 0 {
				t.Fatalf("exit=%d stdout=%s", exit, &stdout)
			}
		})
	}
}

func TestExecuteOutputFailureIncludesHelp(t *testing.T) {
	for _, tc := range []struct {
		name       string
		args       []string
		wantStatus string
	}{
		{"help text", []string{"--help"}, "could not write output\n"},
		{"help JSON", []string{"--help", "--json"}, `"httpStatus":null`},
		{"priority JSON", []string{"priority", "list", "--json"}, `"httpStatus":200`},
		{"project JSON", []string{"project", "list", "--json"}, `"httpStatus":200`},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = w.Write([]byte(`[]`)) }))
			defer server.Close()
			configure(t, server.URL)
			var stderr bytes.Buffer
			exit := cli.Execute(context.Background(), tc.args, failingWriter{}, &stderr, server.Client())
			if exit != 1 || !strings.Contains(stderr.String(), "output") || !strings.Contains(stderr.String(), tc.wantStatus) {
				t.Fatalf("exit=%d stderr=%q", exit, &stderr)
			}
		})
	}
}

func TestExecuteRejectsRedirectWithoutLeakingAuthentication(t *testing.T) {
	var reached atomic.Int32
	target := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { reached.Add(1) }))
	defer target.Close()
	var requests atomic.Int32
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests.Add(1)
		w.Header().Set("Location", target.URL+"/stolen")
		w.WriteHeader(http.StatusFound)
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"project", "list", "--json"}, &stdout, &stderr, server.Client())
	if exit != 1 || stdout.Len() != 0 || reached.Load() != 0 || requests.Load() != 1 || !strings.Contains(stderr.String(), `"httpStatus":302`) || strings.Contains(stderr.String(), server.URL) || strings.Contains(stderr.String(), "test-secret") {
		t.Fatalf("exit=%d requests=%d target=%d stdout=%s stderr=%s", exit, requests.Load(), reached.Load(), &stdout, &stderr)
	}
}

type cancelAfterResponseTransport struct {
	base   http.RoundTripper
	cancel context.CancelFunc
}

func (t cancelAfterResponseTransport) RoundTrip(r *http.Request) (*http.Response, error) {
	response, err := t.base.RoundTrip(r)
	if err == nil {
		t.cancel()
	}
	return response, err
}

func TestExecuteCancellationWhileReadingBody(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(200)
		_, _ = w.Write([]byte(`[`))
		w.(http.Flusher).Flush()
		<-r.Context().Done()
	}))
	defer server.Close()
	configure(t, server.URL)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	client := server.Client()
	client.Transport = cancelAfterResponseTransport{base: client.Transport, cancel: cancel}
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(ctx, []string{"priority", "list", "--json"}, &stdout, &stderr, client)
	if exit != 130 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"cancelled"`) || !strings.Contains(stderr.String(), `"httpStatus":200`) {
		t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
	}
}

func TestExecuteDoesNotTrustUnconfiguredTLS(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { t.Error("untrusted request reached API") }))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"priority", "list", "--json"}, &stdout, &stderr, nil)
	if exit != 1 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"transport_error"`) || strings.Contains(stderr.String(), server.URL) {
		t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
	}
}

func TestExecuteConfigurationErrorDoesNotCallAPI(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"project", "list", "--json"}, &stdout, &stderr, nil)
	if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"config_error"`) {
		t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
	}
}
