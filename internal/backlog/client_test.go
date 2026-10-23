package backlog_test

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/output"
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

type observeTransport struct {
	base http.RoundTripper
	t    *testing.T
}

func (o observeTransport) RoundTrip(r *http.Request) (*http.Response, error) {
	o.t.Helper()
	deadline, ok := r.Context().Deadline()
	if !ok || time.Until(deadline) < 29*time.Second || time.Until(deadline) > 30*time.Second {
		o.t.Errorf("request timeout is not 30 seconds: %v %v", deadline, ok)
	}
	return o.base.RoundTrip(r)
}

func TestClientOperationsEnforceTimeoutAndRequestContract(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "GET" || r.URL.RawQuery != "" || r.Header.Get("Backlog-API-Key") != "secret" || r.Header.Get("Idempotency-Key") != "" || r.Header.Get("X-Idempotency-Key") != "" {
			t.Error("incorrect API authentication or query")
		}
		switch r.URL.Path {
		case "/api/v2/projects/APP":
			_, _ = fmt.Fprint(w, `{"id":42}`)
		case "/api/v2/projects", "/api/v2/priorities", "/api/v2/projects/42/issueTypes":
			_, _ = fmt.Fprint(w, `[]`)
		default:
			t.Errorf("path=%s", r.URL.Path)
		}
	}))
	defer server.Close()
	transport := server.Client().Transport
	injected := &http.Client{Transport: observeTransport{transport, t}, Timeout: time.Millisecond}
	client, err := backlog.New(server.URL+"/", "secret", injected)
	if err != nil {
		t.Fatal(err)
	}
	projects, err := client.Projects(context.Background())
	if err != nil || projects == nil || len(projects) != 0 {
		t.Fatalf("projects=%s error=%v", projects, err)
	}
	priorities, err := client.Priorities(context.Background())
	if err != nil || priorities == nil || len(priorities) != 0 {
		t.Fatalf("priorities=%s error=%v", priorities, err)
	}
	project, err := client.Project(context.Background(), "APP")
	if err != nil || string(project) != `{"id":42}` {
		t.Fatalf("project=%s error=%v", project, err)
	}
	types, err := client.IssueTypes(context.Background(), "42")
	if err != nil || types == nil || len(types) != 0 {
		t.Fatalf("types=%s error=%v", types, err)
	}
	if injected.Timeout != time.Millisecond {
		t.Fatal("mutated caller's client")
	}
}

func TestClientBodyReadRespectsContextDeadline(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`[`))
		w.(http.Flusher).Flush()
		<-r.Context().Done()
	}))
	defer server.Close()
	client, err := backlog.New(server.URL, "secret", server.Client())
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()
	_, err = client.Projects(ctx)
	var failure *output.Failure
	if !errors.As(err, &failure) || failure.Code != "transport_error" || failure.HTTPStatus == nil || *failure.HTTPStatus != 200 {
		t.Fatalf("error=%#v", err)
	}
}

func TestProjectOperationsEscapeOnePathSegment(t *testing.T) {
	for _, tc := range []struct{ value, escaped string }{
		{"日本/APP?x#y%", "%E6%97%A5%E6%9C%AC%2FAPP%3Fx%23y%25"},
		{".", "%2E"},
		{"..", "%2E%2E"},
	} {
		t.Run(tc.value, func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				path := r.URL.EscapedPath()
				if r.URL.RawQuery != "" || r.Header.Get("Backlog-API-Key") != "secret" {
					t.Error("unsafe request")
				}
				switch path {
				case "/api/v2/projects/" + tc.escaped:
					_, _ = fmt.Fprint(w, `{"id":42,"unknown":12345678901234567890}`)
				case "/api/v2/projects/" + tc.escaped + "/issueTypes":
					_, _ = fmt.Fprint(w, `[]`)
				default:
					t.Errorf("path not a single escaped segment: %s", path)
					http.Error(w, "unexpected", 404)
				}
			}))
			defer server.Close()
			client, err := backlog.New(server.URL, "secret", server.Client())
			if err != nil {
				t.Fatal(err)
			}
			project, err := client.Project(context.Background(), tc.value)
			if err != nil || string(project) != `{"id":42,"unknown":12345678901234567890}` {
				t.Fatalf("project=%s error=%v", project, err)
			}
			items, err := client.IssueTypes(context.Background(), tc.value)
			if err != nil || items == nil || len(items) != 0 {
				t.Fatalf("items=%s error=%v", items, err)
			}
		})
	}
}
