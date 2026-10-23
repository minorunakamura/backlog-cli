// Run from the repository root: go run ./internal/cli/testdata/terminal_mock.go issue-type list
// This harness uses an isolated TLS mock and temporary configuration, never a real Backlog space.
package main

import (
	"backlog-cli/internal/cli"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"os/signal"
	"path/filepath"
	"strconv"
	"strings"
)

func main() { os.Exit(run()) }

func run() int {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		issues := r.URL.Path == "/api/v2/issues" || r.URL.Path == "/api/v2/issues/count"
		if r.Header.Get("Backlog-API-Key") != "mock-only-secret" || (!issues && r.URL.RawQuery != "") {
			http.Error(w, "unexpected authentication or query", 400)
			return
		}
		switch {
		case r.URL.Path == "/api/v2/issues/count":
			_, _ = fmt.Fprint(w, `{"count":3}`)
		case r.URL.Path == "/api/v2/issues" && r.Method == http.MethodPost:
			if r.Header.Get("Content-Type") != "application/x-www-form-urlencoded" || r.URL.RawQuery != "" || r.ParseForm() != nil || r.PostForm.Get("projectId") == "" || r.PostForm.Get("issueTypeId") == "" || r.PostForm.Get("priorityId") == "" || strings.TrimSpace(r.PostForm.Get("summary")) == "" {
				http.Error(w, "unexpected create form", 400)
				return
			}
			w.Header().Set("Location", "/view/APP-4")
			w.WriteHeader(http.StatusCreated)
			_ = json.NewEncoder(w).Encode(map[string]any{"issueKey": "APP-4", "summary": r.PostForm.Get("summary"), "description": r.PostForm.Get("description"), "future": json.Number("12345678901234567890")})
		case r.URL.Path == "/api/v2/priorities":
			_, _ = fmt.Fprint(w, `[{"id":3,"name":"中"}]`)
		case r.URL.Path == "/api/v2/issues":
			limit, limitErr := strconv.ParseInt(r.URL.Query().Get("count"), 10, 64)
			offset, offsetErr := strconv.ParseInt(r.URL.Query().Get("offset"), 10, 64)
			if limitErr != nil || offsetErr != nil || limit < 1 || limit > 100 || offset < 0 {
				http.Error(w, "unexpected pagination", 400)
				return
			}
			items := []map[string]any{}
			for i := offset; i < 3 && int64(len(items)) < limit; i++ {
				items = append(items, map[string]any{"issueKey": fmt.Sprintf("APP-%d", i+1), "summary": "日本語👩‍💻\x1b[31m赤\x1b[0m\x07mock-only-secret", "status": map[string]string{"name": "処理中"}, "assignee": nil, "future": json.Number("12345678901234567890")})
			}
			_ = json.NewEncoder(w).Encode(items)
		case r.URL.Path == "/api/v2/projects":
			projects := make([]map[string]any, 25)
			for i := range projects {
				projects[i] = map[string]any{"id": i + 1, "projectKey": fmt.Sprintf("APP_%02d", i+1), "name": strings.Repeat("長い日本語👩‍💻", 12) + "\x1b[31m赤\x1b[0m\x07mock-only-secret"}
			}
			_ = json.NewEncoder(w).Encode(projects)
		case r.URL.Path == "/api/v2/projects/APP":
			_, _ = fmt.Fprint(w, `{"id":42,"projectKey":"APP"}`)
		case r.URL.Path == "/api/v2/users/myself":
			_, _ = fmt.Fprint(w, `{"id":10,"name":"本人"}`)
		case strings.HasPrefix(r.URL.Path, "/api/v2/projects/") && strings.HasSuffix(r.URL.Path, "/statuses"):
			_, _ = fmt.Fprint(w, `[{"id":1,"name":"未対応"},{"id":2,"name":"処理中"},{"id":3,"name":"完了"},{"id":4,"name":"123"},{"id":5,"name":"a,b"},{"id":6,"name":"all"},{"id":7,"name":"重複"},{"id":8,"name":"重複"}]`)
		case strings.HasPrefix(r.URL.Path, "/api/v2/projects/") && strings.HasSuffix(r.URL.Path, "/users"):
			_, _ = fmt.Fprint(w, `[{"id":10,"name":"本人"},{"id":11,"name":"self"},{"id":12,"name":"山田"},{"id":13,"name":"123"},{"id":14,"name":"a,b"},{"id":15,"name":"重複"},{"id":16,"name":"重複"}]`)
		case strings.HasPrefix(r.URL.Path, "/api/v2/projects/") && strings.HasSuffix(r.URL.Path, "/versions"):
			_, _ = fmt.Fprint(w, `[{"id":7,"name":"v1.0"},{"id":8,"name":"123"},{"id":9,"name":"a,b"},{"id":10,"name":"重複"},{"id":11,"name":"重複"}]`)
		case strings.HasPrefix(r.URL.Path, "/api/v2/projects/") && strings.HasSuffix(r.URL.Path, "/issueTypes"):
			project := strings.TrimSuffix(strings.TrimPrefix(r.URL.Path, "/api/v2/projects/"), "/issueTypes")
			_ = json.NewEncoder(w).Encode([]map[string]any{{"id": 7, "name": "タスク👩‍💻 (project " + project + ")", "future": json.Number("12345678901234567890")}})
		default:
			http.Error(w, "mock endpoint not found", 404)
		}
	}))
	defer server.Close()
	base, err := os.MkdirTemp("", "bl-terminal-mock-*")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	defer os.RemoveAll(base)
	if err := os.Mkdir(filepath.Join(base, "backlog"), 0o700); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	config, err := json.Marshal(map[string]any{"profiles": map[string]any{"default": map[string]string{"spaceUrl": server.URL, "apiKeyEnv": "BL_TERMINAL_MOCK_KEY"}}})
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.WriteFile(filepath.Join(base, "backlog", "config.json"), config, 0o600); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.Setenv("XDG_CONFIG_HOME", base); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.Setenv("BL_TERMINAL_MOCK_KEY", "mock-only-secret"); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.Chdir(base); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt)
	defer stop()
	return cli.ExecuteWithInput(ctx, os.Args[1:], os.Stdin, os.Stdout, os.Stderr, server.Client())
}
