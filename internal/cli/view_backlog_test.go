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
	"testing"
)

func TestExecuteIssueViewBacklogFixedOutputAndJSON(t *testing.T) {
	const source = "* 見出し\n一行目\n二行目\n\n#contents\n\n''本文''\n[[APP-2]]\n{code}\n#rev(11) &amp; &#27;\n{/code}\n\n日本語👩‍💻\x1b[31m赤\x1b[0m &#27;[2J\x1b]52;c;stolen\a test-secret+/?日本"
	for _, flags := range [][]string{nil, {"--no-input"}, {"--json"}} {
		t.Run(fmt.Sprint(flags), func(t *testing.T) {
			requests := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				requests++
				switch r.URL.Path {
				case "/api/v2/issues/APP-1":
					_ = json.NewEncoder(w).Encode(map[string]any{"issueKey": "OTHER-9", "projectId": 42, "summary": "Backlog件名", "description": source})
				case "/api/v2/projects/42":
					_, _ = fmt.Fprint(w, `{"textFormattingRule":"backlog"}`)
				default:
					t.Errorf("unexpected API: %s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), append([]string{"issue", "view", "APP-1"}, flags...), unreadableInput{t}, &stdout, &stderr, server.Client())
			if exit != 0 || stderr.Len() != 0 {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
			if len(flags) > 0 && flags[0] == "--json" {
				var item struct {
					Description string `json:"description"`
				}
				if err := json.Unmarshal(stdout.Bytes(), &item); err != nil {
					t.Fatal(err)
				}
				if requests != 1 || item.Description != strings.ReplaceAll(source, "test-secret+/?日本", "[REDACTED]") {
					t.Fatalf("JSON changed or fetched project: requests=%d description=%q", requests, item.Description)
				}
				return
			}
			if requests != 2 {
				t.Fatalf("requests=%d", requests)
			}
			for _, want := range []string{"OTHER-9", "Backlog件名", "見出し", "目次", "本文", "一行目", "二行目", "#rev(11) &amp; &#27;", "日本語👩‍💻", "U+001B", "[REDACTED]", server.URL + "/view/APP-2"} {
				if !strings.Contains(stdout.String(), want) {
					t.Errorf("missing %q: %s", want, &stdout)
				}
			}
			for _, bad := range []string{"''本文''", "#contents", "stolen", "test-secret", "\x1b", "\a"} {
				if strings.Contains(stdout.String(), bad) {
					t.Errorf("unsafe/unconverted %q: %s", bad, &stdout)
				}
			}
		})
	}
}
