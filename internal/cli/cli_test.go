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
		{"project flag not registered", []string{"project", "list", "--project", "--json"}, 200, `[]`, "input_error", 2},
		{"short project flag not registered", []string{"priority", "list", "-p", "--json"}, 200, `[]`, "input_error", 2},
		{"root project flag not registered", []string{"--project", "--json"}, 200, `[]`, "input_error", 2},
		{"unknown command project flag", []string{"bogus", "--project", "--json"}, 200, `[]`, "input_error", 2},
		{"extra arg project flag", []string{"project", "list", "extra", "--project", "--json"}, 200, `[]`, "input_error", 2},
		{"explicit JSON disabled then enabled", []string{"project", "list", "--unknown", "--json=false", "--json"}, 200, `[]`, "input_error", 2},
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

func TestExecuteIssueTypeListDirectIDPreservesAPIObjects(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "GET" || r.URL.Path != "/api/v2/projects/9223372036854775807/issueTypes" || r.URL.RawQuery != "" || r.Header.Get("Backlog-API-Key") != "test-secret+/?日本" {
			t.Errorf("unexpected request: %s %s", r.Method, r.URL)
			http.Error(w, "unexpected endpoint", 404)
			return
		}
		_, _ = fmt.Fprint(w, `[{"id":7,"name":"日本語👩‍💻\u001b[31m","future":{"value":12345678901234567890},"secret":"test-secret+/?日本"}]`)
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"issue-type", "list", "-p", "9223372036854775807", "--json"}, &stdout, &stderr, server.Client())
	want := `{"items":[{"future":{"value":12345678901234567890},"id":7,"name":"日本語👩‍💻\u001b[31m","secret":"[REDACTED]"}]}` + "\n"
	if exit != 0 || stdout.String() != want || stderr.Len() != 0 {
		t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
	}
}

func TestExecuteIssueTypeProjectResolutionAndPrecedence(t *testing.T) {
	for _, tc := range []struct {
		name, local string
		flags       []string
		paths       []string
	}{
		{"key", "", []string{"--project", "APP"}, []string{"/api/v2/projects/APP", "/api/v2/projects/42/issueTypes"}},
		{"short key", "", []string{"-pAPP"}, []string{"/api/v2/projects/APP", "/api/v2/projects/42/issueTypes"}},
		{"local key", "APP", nil, []string{"/api/v2/projects/APP", "/api/v2/projects/42/issueTypes"}},
		{"explicit overrides local", "OTHER", []string{"-p", "APP"}, []string{"/api/v2/projects/APP", "/api/v2/projects/42/issueTypes"}},
		{"local ID", "42", nil, []string{"/api/v2/projects/42/issueTypes"}},
		{"explicit ID overrides local", "APP", []string{"--project=42"}, []string{"/api/v2/projects/42/issueTypes"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				switch r.URL.Path {
				case "/api/v2/projects/APP":
					_, _ = fmt.Fprint(w, `{"id":42,"projectKey":"APP","future":true}`)
				case "/api/v2/projects/42/issueTypes":
					_, _ = fmt.Fprint(w, `[{"id":7,"name":"タスク"}]`)
				default:
					http.Error(w, "unexpected endpoint", 404)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			if tc.local != "" {
				if err := os.Mkdir(".backlog", 0o700); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(".backlog/settings.json", []byte(fmt.Sprintf(`{"project":%q}`, tc.local)), 0o600); err != nil {
					t.Fatal(err)
				}
			}
			var stdout, stderr bytes.Buffer
			args := append([]string{"issue-type", "list"}, tc.flags...)
			exit := cli.Execute(context.Background(), args, &stdout, &stderr, server.Client())
			if exit != 0 || stdout.String() != "ID\tNAME\n7\tタスク\n" || stderr.Len() != 0 || strings.Join(paths, ",") != strings.Join(tc.paths, ",") {
				t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueTypeRejectsInvalidProjectBeforeAPI(t *testing.T) {
	for _, flags := range [][]string{
		{"--project", "all"},
		{"-p", "0"},
		{"-p=-1"},
		{"-p", "9223372036854775808"},
		{"-p", ""},
		{"--project", " \t"},
		{"--project", "APP", "-p", "APP"},
		{"-pAPP", "--project=APP"},
		{"--project=APP", "--project=OTHER"},
		{"-p42", "-p43"},
	} {
		t.Run(strings.Join(flags, " "), func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				t.Error("invalid input reached API")
				http.Error(w, "unexpected", 500)
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			args := append([]string{"issue-type", "list", "--json"}, flags...)
			exit := cli.Execute(context.Background(), args, &stdout, &stderr, server.Client())
			if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"input_error"`) {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteJSONFlagValuesAndTerminatorDoNotEnableJSONErrors(t *testing.T) {
	for _, args := range [][]string{
		{"--unknown", "--profile", "--json"},
		{"project", "list", "--unknown", "--profile", "--json"},
		{"bogus", "--profile", "--json"},
		{"project", "list", "--", "--json"},
		{"project", "list", "--unknown", "--json", "--json=false"},
	} {
		t.Run(strings.Join(args, " "), func(t *testing.T) {
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(context.Background(), args, &stdout, &stderr, nil)
			if exit != 2 || stdout.Len() != 0 || stderr.Len() == 0 || strings.HasPrefix(stderr.String(), "{") {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteProjectFlagValueDoesNotEnableJSONErrors(t *testing.T) {
	for _, flags := range [][]string{{"--project", "--json"}, {"-p", "--json"}, {"-p--json"}, {"-p=--json"}, {"--project=--json"}} {
		t.Run(strings.Join(flags, " "), func(t *testing.T) {
			var stdout, stderr bytes.Buffer
			args := append([]string{"issue-type", "list", "--unknown"}, flags...)
			exit := cli.Execute(context.Background(), args, &stdout, &stderr, nil)
			if exit != 2 || stdout.Len() != 0 || strings.HasPrefix(stderr.String(), "{") {
				t.Fatalf("project value misread as flag: exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
		})
	}
}

type unreadableInput struct{ t *testing.T }

func (r unreadableInput) Read([]byte) (int, error) {
	r.t.Error("noninteractive command read stdin")
	return 0, errors.New("unexpected read")
}

func TestExecuteIssueTypeMissingProjectDoesNotPromptNonTTY(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Error("missing project called API")
		http.Error(w, "unexpected", 500)
	}))
	defer server.Close()
	configure(t, server.URL)
	for _, flags := range [][]string{nil, {"--json"}, {"--no-input"}, {"--json", "--no-input"}} {
		var stdout, stderr bytes.Buffer
		args := append([]string{"issue-type", "list"}, flags...)
		exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, server.Client())
		if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), "project is required") {
			t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
		}
	}
}

func TestExecuteIssueTypeFailuresDoNotBecomeEmptyResults(t *testing.T) {
	for _, tc := range []struct {
		name, project, detail, types, code string
		status                             int
	}{
		{"detail rejection", "APP", `{"errors":[{"code":11,"message":"test-secret+/?日本"}]}`, `[]`, "api_error", 403},
		{"detail malformed", "APP", `{broken`, `[]`, "response_error", 200},
		{"detail list", "APP", `[]`, `[]`, "response_error", 200},
		{"detail null", "APP", `null`, `[]`, "response_error", 200},
		{"detail missing ID", "APP", `{"name":"APP"}`, `[]`, "response_error", 200},
		{"detail invalid ID", "APP", `{"id":0}`, `[]`, "response_error", 200},
		{"detail overflow ID", "APP", `{"id":9223372036854775808}`, `[]`, "response_error", 200},
		{"types rejected", "42", ``, `{"errors":[{"code":11,"message":"test-secret+/?日本"}]}`, "api_error", 403},
		{"types malformed", "42", ``, `{broken`, "response_error", 200},
		{"types null", "42", ``, `null`, "response_error", 200},
		{"types nonobject", "42", ``, `[1]`, "response_error", 200},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(tc.status)
				if strings.HasSuffix(r.URL.Path, "/issueTypes") {
					_, _ = fmt.Fprint(w, tc.types)
				} else {
					_, _ = fmt.Fprint(w, tc.detail)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(context.Background(), []string{"issue-type", "list", "-p", tc.project, "--json"}, &stdout, &stderr, server.Client())
			if exit != 1 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"`+tc.code+`"`) || !strings.Contains(stderr.String(), fmt.Sprintf(`"httpStatus":%d`, tc.status)) || strings.Contains(stderr.String(), "test-secret") {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueTypeDoesNotIgnoreInvalidLocalConfiguration(t *testing.T) {
	for _, local := range []string{`{"unknown":true}`, `{"project":"0"}`, `{"project":"-1"}`, `{"project":"9223372036854775808"}`, `{"project":"all"}`} {
		for _, explicit := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/explicit=%t", local, explicit), func(t *testing.T) {
				var requests atomic.Int32
				server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					requests.Add(1)
					_, _ = fmt.Fprint(w, `[]`)
				}))
				defer server.Close()
				configure(t, server.URL)
				if err := os.Mkdir(".backlog", 0o700); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(".backlog/settings.json", []byte(local), 0o600); err != nil {
					t.Fatal(err)
				}
				args := []string{"issue-type", "list", "--json"}
				if explicit {
					args = append(args, "-p", "42")
				}
				var stdout, stderr bytes.Buffer
				exit := cli.Execute(context.Background(), args, &stdout, &stderr, server.Client())
				if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"config_error"`) || requests.Load() != 0 {
					t.Fatalf("exit=%d requests=%d stdout=%s stderr=%s", exit, requests.Load(), &stdout, &stderr)
				}
			})
		}
	}
}

func TestExecuteIssueTypeFixedOutputIsSafeAndEmptyJSONIsArray(t *testing.T) {
	for _, tc := range []struct {
		body, want string
		flags      []string
	}{
		{`[{"id":7,"name":"日本語👩‍💻\u001b[31m赤\u001b[0m\u0001\u0085\u007ftest-secret+/?日本"}]`, "ID\tNAME\n7\t日本語👩‍💻赤[REDACTED]\n", nil},
		{`[]`, "{\"items\":[]}\n", []string{"--json"}},
	} {
		server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = fmt.Fprint(w, tc.body) }))
		configure(t, server.URL)
		var stdout, stderr bytes.Buffer
		args := append([]string{"issue-type", "list", "-p42"}, tc.flags...)
		exit := cli.Execute(context.Background(), args, &stdout, &stderr, server.Client())
		server.Close()
		if exit != 0 || stdout.String() != tc.want || stderr.Len() != 0 {
			t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
		}
	}
}
