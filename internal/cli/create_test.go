package cli_test

import (
	"backlog-cli/internal/cli"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
)

func TestExecuteIssueCreateUsesSelectedProfileForSelf(t *testing.T) {
	calls := 0
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		if r.Header.Get("Backlog-API-Key") != "alternate-secret" {
			t.Error("wrong selected profile key")
		}
		switch r.URL.Path {
		case "/api/v2/users/myself":
			_, _ = fmt.Fprint(w, `{"id":99}`)
		case "/api/v2/issues":
			_ = r.ParseForm()
			if r.Form.Get("assigneeId") != "99" {
				t.Errorf("form=%v", r.Form)
			}
			w.WriteHeader(201)
			_, _ = fmt.Fprint(w, `{"issueKey":"APP-1"}`)
		default:
			t.Errorf("unexpected path %s", r.URL.Path)
		}
	}))
	defer server.Close()
	configure(t, "https://unused.invalid")
	t.Setenv("ALTERNATE_KEY", "alternate-secret")
	data, err := json.Marshal(map[string]any{"profiles": map[string]any{"default": map[string]string{"spaceUrl": "https://unused.invalid", "apiKeyEnv": "SLICE_TEST_KEY"}, "work": map[string]string{"spaceUrl": server.URL, "apiKeyEnv": "ALTERNATE_KEY"}}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(os.Getenv("XDG_CONFIG_HOME")+"/backlog/config.json", data, 0o600); err != nil {
		t.Fatal(err)
	}
	var stdout, stderr bytes.Buffer
	exit := cli.ExecuteWithInput(context.Background(), append(createArgs(), "--profile=work", "-aself"), unreadableInput{t}, &stdout, &stderr, server.Client())
	if exit != 0 || calls != 2 || stderr.Len() != 0 || !strings.Contains(stdout.String(), server.URL+"/view/APP-1") {
		t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls, &stdout, &stderr)
	}
}

func TestExecuteIssueCreateBodySources(t *testing.T) {
	for _, tc := range []struct {
		name        string
		flags       []string
		stdin, body string
		present     bool
	}{
		{"omitted", nil, "", "", false},
		{"body", []string{"--body= \n本文\r\n\t "}, "", " \n本文\r\n\t ", true},
		{"short body", []string{"-b", "本文"}, "", "本文", true},
		{"empty body", []string{"--body="}, "", "", true},
		{"file", []string{"--body-file=body.txt"}, "", " \nfile本文\r\n ", true},
		{"stdin", []string{"--body-file=-"}, " \nstdin本文\r\n ", " \nstdin本文\r\n ", true},
		{"empty stdin", []string{"--body-file=-"}, "", "", true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			calls := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls++
				_ = r.ParseForm()
				_, present := r.PostForm["description"]
				if r.PostForm.Get("description") != tc.body || present != tc.present {
					t.Errorf("description=%q present=%v", r.PostForm.Get("description"), present)
				}
				w.WriteHeader(201)
				_, _ = fmt.Fprint(w, `{"issueKey":"APP-1"}`)
			}))
			defer server.Close()
			configure(t, server.URL)
			if err := os.WriteFile("body.txt", []byte(" \nfile本文\r\n "), 0o600); err != nil {
				t.Fatal(err)
			}
			// A valid but nonexistent editor must never be launched when required flags are supplied.
			path := os.Getenv("XDG_CONFIG_HOME") + "/backlog/config.json"
			data, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			data = bytes.Replace(data, []byte(`{"profiles":`), []byte(`{"editor":["/nonexistent-editor-must-not-run"],"profiles":`), 1)
			if err := os.WriteFile(path, data, 0o600); err != nil {
				t.Fatal(err)
			}
			var stdout, stderr bytes.Buffer
			var input interface{ Read([]byte) (int, error) } = unreadableInput{t}
			if tc.name == "stdin" || tc.name == "empty stdin" {
				input = strings.NewReader(tc.stdin)
			}
			exit := cli.ExecuteWithInput(context.Background(), append(createArgs(), tc.flags...), input, &stdout, &stderr, server.Client())
			if exit != 0 || calls != 1 || stderr.Len() != 0 {
				t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueCreateDefaultsNamesAndAssignees(t *testing.T) {
	for _, tc := range []struct {
		name     string
		flags    []string
		assignee string
		paths    string
	}{
		{"defaults", nil, "10", "/api/v2/projects/APP,/api/v2/projects/42/issueTypes,/api/v2/priorities,/api/v2/users/myself,/api/v2/issues"},
		{"direct overrides", []string{"-p42", "--issue-type-id=7", "--priority-id=3", "--assignee-id=12"}, "12", "/api/v2/issues"},
		{"literal self", []string{"--assignee-name=self"}, "11", "/api/v2/projects/APP,/api/v2/projects/42/issueTypes,/api/v2/priorities,/api/v2/projects/42/users,/api/v2/issues"},
		{"ordinary", []string{"-a山田"}, "12", "/api/v2/projects/APP,/api/v2/projects/42/issueTypes,/api/v2/priorities,/api/v2/projects/42/users,/api/v2/issues"},
		{"numeric names", []string{"--issue-type=123", "--priority=456", "--assignee=123"}, "13", "/api/v2/projects/APP,/api/v2/projects/42/issueTypes,/api/v2/priorities,/api/v2/projects/42/users,/api/v2/issues"},
		{"clear default", []string{"--no-assignee"}, "", "/api/v2/projects/APP,/api/v2/projects/42/issueTypes,/api/v2/priorities,/api/v2/issues"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				switch r.URL.Path {
				case "/api/v2/projects/APP":
					_, _ = fmt.Fprint(w, `{"id":42}`)
				case "/api/v2/projects/42/issueTypes":
					_, _ = fmt.Fprint(w, `[{"id":7,"name":"タスク"},{"id":8,"name":"123"}]`)
				case "/api/v2/priorities":
					_, _ = fmt.Fprint(w, `[{"id":3,"name":"通常"},{"id":4,"name":"456"}]`)
				case "/api/v2/users/myself":
					_, _ = fmt.Fprint(w, `{"id":10}`)
				case "/api/v2/projects/42/users":
					_, _ = fmt.Fprint(w, `[{"id":11,"name":"self"},{"id":12,"name":"山田"},{"id":13,"name":"123"}]`)
				case "/api/v2/issues":
					_ = r.ParseForm()
					it, priority := "7", "3"
					if tc.name == "numeric names" {
						it, priority = "8", "4"
					}
					if r.Method != "POST" || r.Form.Get("assigneeId") != tc.assignee || r.Form.Get("issueTypeId") != it || r.Form.Get("priorityId") != priority || r.Form.Get("projectId") != "42" {
						t.Errorf("form=%v", r.Form)
					}
					w.WriteHeader(201)
					_, _ = fmt.Fprint(w, `{"issueKey":"APP-1"}`)
				default:
					t.Errorf("unexpected path %s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			if err := os.Mkdir(".backlog", 0o700); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(".backlog/settings.json", []byte(`{"project":"APP","issue":{"create":{"issueType":"タスク","priority":"通常","assignee":"self"},"list":{"status":["invalid"]}}}`), 0o600); err != nil {
				t.Fatal(err)
			}
			var stdout, stderr bytes.Buffer
			args := append([]string{"issue", "create", "--title=件名", "--json"}, tc.flags...)
			exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, server.Client())
			if exit != 0 || stderr.Len() != 0 || strings.Join(paths, ",") != tc.paths {
				t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueCreatePostsOnceAndPreservesResponse(t *testing.T) {
	calls := 0
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		if r.Method != "POST" || r.URL.Path != "/api/v2/issues" || r.URL.RawQuery != "" || r.Header.Get("Backlog-API-Key") != "test-secret+/?日本" || r.Header.Get("Content-Type") != "application/x-www-form-urlencoded" {
			t.Errorf("unexpected request: %s %s headers=%v", r.Method, r.URL, r.Header)
		}
		if err := r.ParseForm(); err != nil {
			t.Fatal(err)
		}
		if r.PostForm.Encode() != "issueTypeId=7&priorityId=3&projectId=42&summary=+%E4%BB%B6%E5%90%8D+" {
			t.Errorf("form=%v", r.PostForm)
		}
		w.WriteHeader(201)
		_, _ = fmt.Fprint(w, `{"issueKey":"APP-1","summary":"件名","future":{"number":12345678901234567890,"text":"a\u001bb"}}`)
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.ExecuteWithInput(context.Background(), []string{"issue", "create", "-p42", "--issue-type-id=7", "--priority-id=3", "-t", " 件名 ", "--json"}, unreadableInput{t}, &stdout, &stderr, server.Client())
	var got map[string]json.RawMessage
	if exit != 0 || calls != 1 || stderr.Len() != 0 || json.Unmarshal(stdout.Bytes(), &got) != nil || string(got["url"]) != `"`+server.URL+`/view/APP-1"` || !strings.Contains(string(got["future"]), "12345678901234567890") {
		t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls, &stdout, &stderr)
	}
}
