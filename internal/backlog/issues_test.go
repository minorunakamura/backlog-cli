package backlog_test

import (
	"backlog-cli/internal/backlog"
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"reflect"
	"testing"
)

func TestIssueCountAndIssuesUseSameFilterWithoutCandidateRequests(t *testing.T) {
	var requests []string
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests = append(requests, r.URL.RequestURI())
		if r.Method != "GET" || r.Header.Get("Backlog-API-Key") != "test-key" {
			t.Error("invalid request authentication or method")
		}
		switch r.URL.Path {
		case "/api/v2/issues/count":
			_, _ = fmt.Fprint(w, `{"count":9,"future":1}`)
		case "/api/v2/issues":
			_, _ = fmt.Fprint(w, `[{"issueKey":"APP-1","future":12345678901234567890}]`)
		default:
			t.Errorf("unexpected path %s", r.URL.Path)
		}
	}))
	defer server.Close()
	client, err := backlog.New(server.URL, "test-key", server.Client())
	if err != nil {
		t.Fatal(err)
	}
	filter := backlog.IssueFilter{ProjectIDs: []int64{42}, StatusIDs: []int64{1, 2}, AssigneeIDs: []int64{3}, MilestoneIDs: []int64{4}, Keyword: "日本語 +/"}
	total, err := client.IssueCount(context.Background(), filter)
	if err != nil || total != 9 {
		t.Fatalf("total=%d err=%v", total, err)
	}
	items, err := client.Issues(context.Background(), filter, 2, 3)
	if err != nil || len(items) != 1 || string(items[0]) != `{"issueKey":"APP-1","future":12345678901234567890}` {
		t.Fatalf("items=%s err=%v", items, err)
	}
	want := []string{
		"/api/v2/issues/count?assigneeId%5B%5D=3&keyword=%E6%97%A5%E6%9C%AC%E8%AA%9E+%2B%2F&milestoneId%5B%5D=4&projectId%5B%5D=42&statusId%5B%5D=1&statusId%5B%5D=2",
		"/api/v2/issues?assigneeId%5B%5D=3&count=2&keyword=%E6%97%A5%E6%9C%AC%E8%AA%9E+%2B%2F&milestoneId%5B%5D=4&offset=3&order=desc&projectId%5B%5D=42&sort=updated&statusId%5B%5D=1&statusId%5B%5D=2",
	}
	if !reflect.DeepEqual(requests, want) {
		t.Fatalf("requests=%v", requests)
	}
}
