package cli_test

import (
	"backlog-cli/internal/cli"
	"bytes"
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"reflect"
	"strings"
	"testing"
)

func TestExecuteIssueListCancellationStopsSearch(t *testing.T) {
	for _, target := range []string{"/api/v2/issues/count", "/api/v2/issues"} {
		t.Run(target, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				if r.URL.Path == target {
					w.WriteHeader(200)
					_, _ = fmt.Fprint(w, `{`)
					w.(http.Flusher).Flush()
					cancel()
					<-r.Context().Done()
					return
				}
				_, _ = fmt.Fprint(w, `{"count":1}`)
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(ctx, []string{"issue", "list", "-p42", "--json"}, &stdout, &stderr, server.Client())
			want := "/api/v2/issues/count"
			if target == "/api/v2/issues" {
				want += ",/api/v2/issues"
			}
			if exit != 130 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"cancelled"`) || strings.Join(paths, ",") != want {
				t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueListOutputFailure(t *testing.T) {
	for _, flags := range [][]string{{"--json"}, {"--no-input"}} {
		calls := 0
		server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			calls++
			if r.URL.Path == "/api/v2/issues/count" {
				_, _ = fmt.Fprint(w, `{"count":1}`)
			} else {
				_, _ = fmt.Fprint(w, `[{"issueKey":"APP-1"}]`)
			}
		}))
		configure(t, server.URL)
		var stderr bytes.Buffer
		exit := cli.Execute(context.Background(), append([]string{"issue", "list", "-p42"}, flags...), failingWriter{}, &stderr, server.Client())
		server.Close()
		if exit != 1 || calls != 2 || !strings.Contains(stderr.String(), "output") {
			t.Fatalf("exit=%d calls=%d stderr=%s", exit, calls, &stderr)
		}
	}
}

func TestExecuteIssueListFlagValuesDoNotEnableJSONErrors(t *testing.T) {
	for _, flag := range []string{"--project", "-p", "--limit", "--offset", "--status-id", "--assignee-id", "--milestone-id"} {
		var stdout, stderr bytes.Buffer
		exit := cli.Execute(context.Background(), []string{"issue", "list", "--unknown", flag, "--json"}, &stdout, &stderr, nil)
		if exit != 2 || stdout.Len() != 0 || stderr.Len() == 0 || strings.HasPrefix(stderr.String(), "{") {
			t.Fatalf("flag=%s exit=%d stdout=%s stderr=%s", flag, exit, &stdout, &stderr)
		}
	}
}

func TestExecuteIssueListFailuresNeverBecomeEmptyOrPartialSuccess(t *testing.T) {
	for _, tc := range []struct {
		name, endpoint, body, code string
		status                     int
		truncated                  bool
	}{
		{"count rejected", "count", `{"errors":[{"message":"test-secret+/?日本","code":11,"future":{"keep":true}}]}`, "api_error", 403, false},
		{"count malformed", "count", `{broken`, "response_error", 200, false},
		{"count missing", "count", `{}`, "response_error", 200, false},
		{"count null", "count", `{"count":null}`, "response_error", 200, false},
		{"count negative", "count", `{"count":-1}`, "response_error", 200, false},
		{"count overflow", "count", `{"count":9223372036854775808}`, "response_error", 200, false},
		{"count wrong type", "count", `{"count":"1"}`, "response_error", 200, false},
		{"count fractional", "count", `{"count":1.5}`, "response_error", 200, false},
		{"count array", "count", `[]`, "response_error", 200, false},
		{"count null object", "count", `null`, "response_error", 200, false},
		{"count communication", "count", `{}`, "transport_error", 200, true},
		{"list communication", "list", `[]`, "transport_error", 200, true},
		{"list rejected", "list", `{"errors":[{"message":"test-secret+/?日本","code":11}]}`, "api_error", 429, false},
		{"list unexpected success", "list", `[]`, "response_error", 201, false},
		{"list malformed", "list", `{broken`, "response_error", 200, false},
		{"list null", "list", `null`, "response_error", 200, false},
		{"list nonobject", "list", `[1]`, "response_error", 200, false},
		{"list missing key", "list", `[{"issueKey":"APP-1"},{}]`, "response_error", 200, false},
		{"list malicious key", "list", `[{"issueKey":"../APP-1"}]`, "response_error", 200, false},
		{"list wrong key type", "list", `[{"issueKey":1}]`, "response_error", 200, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				if tc.endpoint == "list" && r.URL.Path == "/api/v2/issues/count" {
					_, _ = fmt.Fprint(w, `{"count":7}`)
					return
				}
				if tc.truncated {
					w.Header().Set("Content-Length", "1000")
				}
				w.WriteHeader(tc.status)
				_, _ = fmt.Fprint(w, tc.body)
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(context.Background(), []string{"issue", "list", "-p42", "--json"}, &stdout, &stderr, server.Client())
			wantPaths := "/api/v2/issues/count"
			if tc.endpoint == "list" {
				wantPaths += ",/api/v2/issues"
			}
			if exit != 1 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"`+tc.code+`"`) || !strings.Contains(stderr.String(), fmt.Sprintf(`"httpStatus":%d`, tc.status)) || strings.Contains(stderr.String(), "test-secret") || strings.Join(paths, ",") != wantPaths {
				t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueListFixedPagesUseStartingTotalAndActualCount(t *testing.T) {
	for _, tc := range []struct{ name, total, body, offset, limit, want string }{
		{"zero", "0", `[]`, "0", "20", "KEY\tSUMMARY\tSTATUS\tASSIGNEE\n"},
		{"empty despite total", "99", `[]`, "3", "20", "KEY\tSUMMARY\tSTATUS\tASSIGNEE\n"},
		{"more from actual count", "10", `[{"issueKey":"APP-1","summary":"件名","status":{"name":"処理中"},"assignee":{"name":"山田"}}]`, "2", "100", "KEY\tSUMMARY\tSTATUS\tASSIGNEE\nAPP-1\t件名\t処理中\t山田\nNext offset: 3\n"},
		{"last", "3", `[{"issueKey":"APP-1"}]`, "2", "1", "KEY\tSUMMARY\tSTATUS\tASSIGNEE\nAPP-1\t\t\t\n"},
		{"total smaller than page", "0", `[{"issueKey":"APP-1"}]`, "0", "1", "KEY\tSUMMARY\tSTATUS\tASSIGNEE\nAPP-1\t\t\t\n"},
		{"sum exceeds int64", "9223372036854775807", `[{"issueKey":"APP-1"},{"issueKey":"APP-2"}]`, "9223372036854775806", "20", "KEY\tSUMMARY\tSTATUS\tASSIGNEE\nAPP-1\t\t\t\nAPP-2\t\t\t\n"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				switch r.URL.Path {
				case "/api/v2/issues/count":
					_, _ = fmt.Fprintf(w, `{"count":%s}`, tc.total)
				case "/api/v2/issues":
					if r.URL.Query().Get("count") != tc.limit || r.URL.Query().Get("offset") != tc.offset {
						t.Errorf("query=%v", r.URL.Query())
					}
					_, _ = fmt.Fprint(w, tc.body)
				default:
					t.Errorf("unexpected path %s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), []string{"issue", "list", "-p42", "--limit=" + tc.limit, "--offset=" + tc.offset, "--no-input"}, unreadableInput{t}, &stdout, &stderr, server.Client())
			if exit != 0 || stdout.String() != tc.want || stderr.Len() != 0 || strings.Join(paths, ",") != "/api/v2/issues/count,/api/v2/issues" {
				t.Fatalf("exit=%d paths=%v stdout=%q stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueListRejectsInvalidOrUnsupportedInputWithoutAPI(t *testing.T) {
	cases := [][]string{
		{"--project"},
		{"--limit"},
		{"--offset"},
		{"--status-id"},
		{"--assignee-id"},
		{"--milestone-id"},
		{"extra", "arguments"},
		{"--project=all"},
		{"--project="},
		{"-p0"},
		{"-p-1"},
		{"-p9223372036854775808"},
		{"--project=42", "-p42"},
		{"-p42", "--project=42"},
		{"--limit=1", "--limit=2"},
		{"--offset=0", "--offset=1"},
		{"--profile=default", "--profile=default"},
		{"--limit=0"},
		{"--limit=101"},
		{"--limit=-1"},
		{"--limit=9223372036854775808"},
		{"--offset=-1"},
		{"--offset=9223372036854775808"},
		{"--offset=1.5"},
		{"--status=all"},
		{"-s未対応"},
		{"--assignee=self"},
		{"-a山田"},
		{"--assignee-name=self"},
		{"--milestone=v1"},
		{"-mv1"},
		{"--sort=created"},
	}
	for _, name := range []string{"status-id", "assignee-id", "milestone-id"} {
		for _, value := range []string{"", "0", "-1", "9223372036854775808", "1,2", "1.0", " 1", "self"} {
			cases = append(cases, []string{"--" + name + "=" + value})
		}
	}
	for _, flags := range cases {
		t.Run(strings.Join(flags, " "), func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { t.Error("invalid input called API") }))
			defer server.Close()
			configure(t, server.URL)
			if err := os.Mkdir(".backlog", 0o700); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(".backlog/settings.json", []byte(`{"project":"42"}`), 0o600); err != nil {
				t.Fatal(err)
			}
			args := append([]string{"issue", "list", "--json"}, flags...)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, server.Client())
			if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"input_error"`) {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
		})
	}
	for _, args := range [][]string{{"issue", "mine", "--json"}, {"issue", "--json"}} {
		var stdout, stderr bytes.Buffer
		exit := cli.Execute(context.Background(), args, &stdout, &stderr, nil)
		if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"input_error"`) {
			t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
		}
	}
}

func TestExecuteIssueListMissingProjectDoesNotPromptNonTTY(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { t.Error("missing project called API") }))
	defer server.Close()
	configure(t, server.URL)
	for _, flags := range [][]string{nil, {"--json"}, {"--no-input"}, {"--json", "--no-input"}} {
		var stdout, stderr bytes.Buffer
		exit := cli.ExecuteWithInput(context.Background(), append([]string{"issue", "list"}, flags...), unreadableInput{t}, &stdout, &stderr, server.Client())
		if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), "project is required") {
			t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
		}
	}
}

func TestExecuteIssueListProjectAndStatusDefaults(t *testing.T) {
	for _, tc := range []struct {
		name, local string
		flags       []string
		paths       []string
		exit        int
	}{
		{"local project", `{"project":"APP","issue":{"create":{"assignee":"self"}}}`, nil, []string{"/api/v2/projects/APP", "/api/v2/issues/count", "/api/v2/issues"}, 0},
		{"explicit project", `{"project":"OTHER","issue":{"list":{"status":[]}}}`, []string{"-p42"}, []string{"/api/v2/issues/count", "/api/v2/issues"}, 0},
		{"unresolved local status", `{"project":"APP","issue":{"list":{"status":["未対応"]}}}`, nil, nil, 2},
		{"explicit status ID overrides local", `{"project":"42","issue":{"list":{"status":["未対応"]}}}`, []string{"--status-id=1"}, []string{"/api/v2/issues/count", "/api/v2/issues"}, 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				switch r.URL.Path {
				case "/api/v2/projects/APP":
					_, _ = fmt.Fprint(w, `{"id":42}`)
				case "/api/v2/issues/count":
					if r.URL.Query().Get("projectId[]") != "42" {
						t.Errorf("project filter=%v", r.URL.Query())
					}
					if len(r.URL.Query()["assigneeId[]"]) != 0 {
						t.Error("create assignee leaked into list")
					}
					if len(tc.flags) > 0 && tc.flags[0] == "--status-id=1" && r.URL.Query().Get("statusId[]") != "1" {
						t.Error("status override missing")
					}
					_, _ = fmt.Fprint(w, `{"count":0}`)
				case "/api/v2/issues":
					_, _ = fmt.Fprint(w, `[]`)
				default:
					t.Errorf("unexpected endpoint: %s", r.URL.Path)
					http.Error(w, "unexpected", 500)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			if err := os.Mkdir(".backlog", 0o700); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(".backlog/settings.json", []byte(tc.local), 0o600); err != nil {
				t.Fatal(err)
			}
			var stdout, stderr bytes.Buffer
			args := append([]string{"issue", "list", "--json"}, tc.flags...)
			exit := cli.Execute(context.Background(), args, &stdout, &stderr, server.Client())
			if exit != tc.exit || !reflect.DeepEqual(paths, tc.paths) || (exit != 0 && (stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"input_error"`))) || (exit == 0 && stderr.Len() != 0) {
				t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueListResolvedFiltersAreShared(t *testing.T) {
	var paths []string
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		paths = append(paths, r.URL.Path)
		want := url.Values{"projectId[]": {"42"}, "statusId[]": {"1", "9223372036854775807"}, "assigneeId[]": {"2", "3"}, "milestoneId[]": {"4"}, "keyword": {"日本語,+/ 検索"}}
		switch r.URL.Path {
		case "/api/v2/issues":
			want.Set("count", "100")
			want.Set("offset", "9223372036854775807")
			want.Set("sort", "updated")
			want.Set("order", "desc")
			_, _ = fmt.Fprint(w, `[]`)
		case "/api/v2/issues/count":
			_, _ = fmt.Fprint(w, `{"count":9223372036854775807}`)
		default:
			t.Errorf("unexpected endpoint: %s", r.URL.Path)
		}
		if !reflect.DeepEqual(r.URL.Query(), want) {
			t.Errorf("query=%v want=%v", r.URL.Query(), want)
		}
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"issue", "list", "日本語,+/ 検索", "-p+042", "--status-id=01", "--status-id=+1", "--status-id=9223372036854775807", "--assignee-id=2", "--assignee-id=3", "--milestone-id=004", "--milestone-id=4", "--limit=100", "--offset=9223372036854775807", "--json"}, &stdout, &stderr, server.Client())
	want := `{"items":[],"pagination":{"offset":9223372036854775807,"limit":100,"count":0,"total":9223372036854775807}}` + "\n"
	if exit != 0 || stderr.Len() != 0 || stdout.String() != want || strings.Join(paths, ",") != "/api/v2/issues/count,/api/v2/issues" {
		t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
	}
}

func TestExecuteIssueListCountsThenListsOnePage(t *testing.T) {
	var paths []string
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		paths = append(paths, r.URL.Path)
		if r.Method != "GET" || r.Header.Get("Backlog-API-Key") != "test-secret+/?日本" {
			t.Error("incorrect API authentication or method")
		}
		switch r.URL.Path {
		case "/api/v2/issues/count":
			if r.URL.Query().Encode() != "projectId%5B%5D=42" {
				t.Errorf("count query=%s", r.URL.RawQuery)
			}
			_, _ = fmt.Fprint(w, `{"count":7,"future":true}`)
		case "/api/v2/issues":
			if r.URL.Query().Encode() != "count=20&offset=0&order=desc&projectId%5B%5D=42&sort=updated" {
				t.Errorf("list query=%s", r.URL.RawQuery)
			}
			_, _ = fmt.Fprint(w, `[{"issueKey":"APP-1","summary":"日本語\u001b[31m","future":{"number":12345678901234567890}}]`)
		default:
			t.Errorf("unexpected endpoint %s", r.URL.Path)
			http.Error(w, "unexpected endpoint", 500)
		}
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.ExecuteWithInput(context.Background(), []string{"issue", "list", "-p42", "--json"}, unreadableInput{t}, &stdout, &stderr, server.Client())
	want := fmt.Sprintf(`{"items":[{"future":{"number":12345678901234567890},"issueKey":"APP-1","summary":"日本語\u001b[31m","url":%q}],"pagination":{"offset":0,"limit":20,"count":1,"total":7}}`+"\n", server.URL+"/view/APP-1")
	if exit != 0 || stderr.Len() != 0 || stdout.String() != want || strings.Join(paths, ",") != "/api/v2/issues/count,/api/v2/issues" {
		t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
	}
}
