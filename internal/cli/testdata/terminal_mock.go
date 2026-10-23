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
	"strings"
)

func main() { os.Exit(run()) }

func run() int {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Backlog-API-Key") != "mock-only-secret" || r.URL.RawQuery != "" {
			http.Error(w, "unexpected authentication or query", 400)
			return
		}
		switch {
		case r.URL.Path == "/api/v2/projects":
			projects := make([]map[string]any, 25)
			for i := range projects {
				projects[i] = map[string]any{"id": i + 1, "projectKey": fmt.Sprintf("APP_%02d", i+1), "name": strings.Repeat("長い日本語👩‍💻", 12) + "\x1b[31m赤\x1b[0m\x07mock-only-secret"}
			}
			_ = json.NewEncoder(w).Encode(projects)
		case r.URL.Path == "/api/v2/projects/APP":
			_, _ = fmt.Fprint(w, `{"id":42,"projectKey":"APP"}`)
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
