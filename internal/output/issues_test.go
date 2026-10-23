package output_test

import (
	"backlog-cli/internal/output"
	"bytes"
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

func TestWriteIssueListJSONPreservesOriginalFieldsAndReplacesUntrustedURL(t *testing.T) {
	var stdout bytes.Buffer
	item := json.RawMessage(`{"issueKey":"APP_1-2","summary":"日本語👩‍💻\r\n\u001b]52;c;payload\u0007","description":"\t原文","url":"https://evil.example/","future":{"large":12345678901234567890,"exponent":1e9999,"secret":"test-secret"}}`)
	err := output.WriteIssueList(&stdout, []json.RawMessage{item}, output.Pagination{Offset: 0, Limit: 20, Count: 1, Total: 1}, "https://configured.example", true, "test-secret")
	want := `{"items":[{"description":"\t原文","future":{"exponent":1e9999,"large":12345678901234567890,"secret":"[REDACTED]"},"issueKey":"APP_1-2","summary":"日本語👩‍💻\r\n\u001b]52;c;payload\u0007","url":"https://configured.example/view/APP_1-2"}],"pagination":{"offset":0,"limit":20,"count":1,"total":1}}` + "\n"
	if err != nil || stdout.String() != want {
		t.Fatalf("err=%v stdout=%s", err, &stdout)
	}
}

func TestWriteIssueListInvalidResponseDoesNotWritePartialPage(t *testing.T) {
	for _, tc := range []struct {
		item     string
		jsonMode bool
	}{
		{`{"issueKey":"../APP-1"}`, true},
		{`{"issueKey":"https://evil.example/APP-1"}`, true},
		{`{"issueKey":"APP-1?x=y"}`, true},
		{`{"issueKey":"APP-1\u001b"}`, true},
		{`{"issueKey":null}`, true},
		{`{"issueKey":""}`, true},
		{`{}`, false},
		{`null`, true},
		{`{"issueKey":"APP-1","summary":1}`, false},
		{`{"issueKey":"APP-1","status":"完了"}`, false},
		{`{"issueKey":"APP-1","assignee":{"name":1}}`, false},
	} {
		t.Run(tc.item, func(t *testing.T) {
			var stdout bytes.Buffer
			err := output.WriteIssueList(&stdout, []json.RawMessage{json.RawMessage(`{"issueKey":"APP-1","summary":"valid"}`), json.RawMessage(tc.item)}, output.Pagination{Count: 2, Total: 2}, "https://example.test", tc.jsonMode, "")
			var failure *output.Failure
			if !errors.As(err, &failure) || failure.Code != "response_error" || stdout.Len() != 0 {
				t.Fatalf("err=%v stdout=%s", err, &stdout)
			}
		})
	}
}

func TestWriteIssueListFixedPageAndContinuation(t *testing.T) {
	for _, tc := range []struct {
		name  string
		items []json.RawMessage
		page  output.Pagination
		want  string
	}{
		{"more uses actual count", []json.RawMessage{json.RawMessage(`{"issueKey":"APP-1","summary":"日本語👩‍💻\u001b[31m赤\u001b[0m\u0007sec\u0001ret","status":{"name":"処理中\u0085"},"assignee":{"name":"secret"}}`)}, output.Pagination{Offset: 2, Limit: 20, Count: 1, Total: 7}, "KEY\tSUMMARY\tSTATUS\tASSIGNEE\nAPP-1\t日本語👩‍💻赤[REDACTED]\t処理中\t[REDACTED]\nNext offset: 3\n"},
		{"empty despite total", nil, output.Pagination{Offset: 2, Limit: 20, Count: 0, Total: 7}, "KEY\tSUMMARY\tSTATUS\tASSIGNEE\n"},
		{"last page", []json.RawMessage{json.RawMessage(`{"issueKey":"APP-1","summary":"件名","status":{"name":"完了"},"assignee":null}`)}, output.Pagination{Offset: 6, Limit: 20, Count: 1, Total: 7}, "KEY\tSUMMARY\tSTATUS\tASSIGNEE\nAPP-1\t件名\t完了\t\n"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var stdout bytes.Buffer
			err := output.WriteIssueList(&stdout, tc.items, tc.page, "https://example.test", false, "secret")
			if err != nil || stdout.String() != tc.want {
				t.Fatalf("err=%v stdout=%q", err, &stdout)
			}
		})
	}
}

func TestPaginationNextOffsetStopsWithoutOverflow(t *testing.T) {
	for _, tc := range []struct {
		name string
		page output.Pagination
		next int64
		more bool
	}{
		{"empty", output.Pagination{Offset: 0, Count: 0, Total: 100}, 0, false},
		{"zero total", output.Pagination{Offset: 0, Count: 1, Total: 0}, 0, false},
		{"more", output.Pagination{Offset: 3, Count: 2, Total: 6}, 5, true},
		{"last", output.Pagination{Offset: 3, Count: 2, Total: 5}, 0, false},
		{"total shrank", output.Pagination{Offset: 4, Count: 3, Total: 2}, 0, false},
		{"near max", output.Pagination{Offset: 9223372036854775805, Count: 1, Total: 9223372036854775807}, 9223372036854775806, true},
		{"sum overflows", output.Pagination{Offset: 9223372036854775806, Count: 2, Total: 9223372036854775807}, 0, false},
		{"max offset", output.Pagination{Offset: 9223372036854775807, Count: 1, Total: 9223372036854775807}, 0, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			next, more := tc.page.NextOffset()
			if next != tc.next || more != tc.more {
				t.Fatalf("next=%d more=%t", next, more)
			}
		})
	}
}

func TestWriteIssueListJSONHasNoContinuationField(t *testing.T) {
	var stdout bytes.Buffer
	err := output.WriteIssueList(&stdout, nil, output.Pagination{Offset: 0, Limit: 20, Count: 0, Total: 100}, "https://example.test", true, "")
	if err != nil || stdout.String() != "{\"items\":[],\"pagination\":{\"offset\":0,\"limit\":20,\"count\":0,\"total\":100}}\n" || strings.Contains(stdout.String(), "hasMore") {
		t.Fatalf("err=%v stdout=%s", err, &stdout)
	}
}
