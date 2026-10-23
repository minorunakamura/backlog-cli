package backlog_test

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/output"
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"sync/atomic"
	"testing"
	"time"
)

func TestCreateIssueRefusesRedirectAndEnforcesRequestPolicy(t *testing.T) {
	var calls, targetCalls atomic.Int32
	target := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { targetCalls.Add(1) }))
	defer target.Close()
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		if r.Method != "POST" || r.URL.Path != "/api/v2/issues" || r.URL.RawQuery != "" || r.Header.Get("Content-Type") != "application/x-www-form-urlencoded" || r.Header.Get("Backlog-API-Key") != "secret" || r.Header.Get("Idempotency-Key") != "" || r.Header.Get("X-Idempotency-Key") != "" {
			t.Error("unexpected POST contract")
		}
		if err := r.ParseForm(); err != nil {
			t.Error(err)
		}
		if r.PostForm.Encode() != "description=+%E6%9C%AC%E6%96%87%0A&issueTypeId=7&priorityId=3&projectId=42&summary=%E4%BB%B6%E5%90%8D" {
			t.Errorf("form=%v", r.PostForm)
		}
		w.Header().Set("Location", target.URL+"/view/APP-1")
		w.WriteHeader(307)
	}))
	defer server.Close()
	injected := &http.Client{Transport: observeTransport{server.Client().Transport, t}, Timeout: time.Millisecond}
	client, err := backlog.New(server.URL, "secret", injected)
	if err != nil {
		t.Fatal(err)
	}
	creation := &output.Creation{State: "not_created"}
	_, err = client.CreateIssue(context.Background(), url.Values{"projectId": {"42"}, "summary": {"件名"}, "issueTypeId": {"7"}, "priorityId": {"3"}, "description": {" 本文\n"}}, creation)
	var failure *output.Failure
	if !errors.As(err, &failure) || failure.Code != "response_error" || creation.State != "unknown" || creation.URL != nil || targetCalls.Load() != 0 || calls.Load() != 1 || injected.Timeout != time.Millisecond || injected.CheckRedirect != nil {
		t.Fatalf("err=%v creation=%+v calls=%d target=%d", err, creation, calls.Load(), targetCalls.Load())
	}
}

func TestCreateIssueDeadlineWhileReading201BodyKeepsCreated(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = r.ParseForm()
		w.Header().Set("Location", "/view/APP-1")
		w.WriteHeader(201)
		_, _ = fmt.Fprint(w, `{`)
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
	creation := &output.Creation{State: "not_created"}
	_, err = client.CreateIssue(ctx, url.Values{"summary": {"title"}}, creation)
	var failure *output.Failure
	if !errors.As(err, &failure) || failure.Code != "transport_error" || failure.HTTPStatus == nil || *failure.HTTPStatus != 201 || creation.State != "created" || creation.IssueKey == nil || *creation.IssueKey != "APP-1" {
		t.Fatalf("error=%v creation=%+v", err, creation)
	}
}
