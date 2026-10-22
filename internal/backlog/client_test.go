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
		if r.URL.Path != "/api/v2/projects" && r.URL.Path != "/api/v2/priorities" {
			t.Errorf("path=%s", r.URL.Path)
		}
		_, _ = fmt.Fprint(w, `[]`)
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
