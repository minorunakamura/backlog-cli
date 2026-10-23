package backlog_test

import (
	"backlog-cli/internal/backlog"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestSearchCandidateOperationsPreserveObjectsAndEscapeProject(t *testing.T) {
	for _, tc := range []struct {
		name, endpoint string
		list           func(*backlog.Client, context.Context, string) ([]json.RawMessage, error)
	}{
		{"Statuses", "statuses", (*backlog.Client).Statuses},
		{"Users", "users", (*backlog.Client).Users},
		{"Versions", "versions", (*backlog.Client).Versions},
	} {
		t.Run(tc.name, func(t *testing.T) {
			for _, project := range []struct{ value, escaped string }{{"日本/APP?x#y%", "%E6%97%A5%E6%9C%AC%2FAPP%3Fx%23y%25"}, {"..", "%2E%2E"}} {
				body := `{"id":7,"name":"日本語","future":12345678901234567890}`
				server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					if r.Method != "GET" || r.URL.EscapedPath() != "/api/v2/projects/"+project.escaped+"/"+tc.endpoint || r.URL.RawQuery != "" || r.Header.Get("Backlog-API-Key") != "secret" {
						t.Errorf("incorrect candidate request: %s", r.URL.EscapedPath())
					}
					_, _ = fmt.Fprintf(w, "[%s]", body)
				}))
				client, err := backlog.New(server.URL, "secret", server.Client())
				if err != nil {
					t.Fatal(err)
				}
				items, err := tc.list(client, context.Background(), project.value)
				server.Close()
				if err != nil || len(items) != 1 || string(items[0]) != body {
					t.Fatalf("items=%s err=%v", items, err)
				}
			}
		})
	}
}

func TestMyselfPreservesAuthenticatedUserObject(t *testing.T) {
	body := `{"id":9223372036854775807,"name":"self","future":12345678901234567890}`
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "GET" || r.URL.Path != "/api/v2/users/myself" || r.URL.RawQuery != "" || r.Header.Get("Backlog-API-Key") != "secret" {
			t.Error("incorrect authenticated user request")
		}
		_, _ = fmt.Fprint(w, body)
	}))
	defer server.Close()
	client, err := backlog.New(server.URL, "secret", server.Client())
	if err != nil {
		t.Fatal(err)
	}
	user, err := client.Myself(context.Background())
	if err != nil || string(user) != body {
		t.Fatalf("user=%s err=%v", user, err)
	}
}
