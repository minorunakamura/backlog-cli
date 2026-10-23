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
	"path/filepath"
	"reflect"
	"slices"
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
	for _, command := range []string{"list", "mine"} {
		for _, flag := range []string{"--project", "-p", "--limit", "--offset", "--status", "-s", "--status-id", "--assignee", "-a", "--assignee-name", "--assignee-id", "--milestone", "-m", "--milestone-id"} {
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(context.Background(), []string{"issue", command, "--unknown", flag, "--json"}, &stdout, &stderr, nil)
			if exit != 2 || stdout.Len() != 0 || stderr.Len() == 0 || strings.HasPrefix(stderr.String(), "{") {
				t.Fatalf("flag=%s exit=%d stdout=%s stderr=%s", flag, exit, &stdout, &stderr)
			}
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
	for _, args := range [][]string{{"issue", "--json"}} {
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
		{"resolved local status", `{"project":"APP","issue":{"list":{"status":["未対応"]}}}`, nil, []string{"/api/v2/projects/APP", "/api/v2/projects/42/statuses", "/api/v2/issues/count", "/api/v2/issues"}, 0},
		{"explicit status ID overrides local", `{"project":"42","issue":{"list":{"status":["未対応"]}}}`, []string{"--status-id=1"}, []string{"/api/v2/issues/count", "/api/v2/issues"}, 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				switch r.URL.Path {
				case "/api/v2/projects/APP":
					_, _ = fmt.Fprint(w, `{"id":42}`)
				case "/api/v2/projects/42/statuses":
					_, _ = fmt.Fprint(w, `[{"id":1,"name":"未対応"}]`)
				case "/api/v2/issues/count":
					if r.URL.Query().Get("projectId[]") != "42" {
						t.Errorf("project filter=%v", r.URL.Query())
					}
					if len(r.URL.Query()["assigneeId[]"]) != 0 {
						t.Error("create assignee leaked into list")
					}
					if (tc.name == "resolved local status" || tc.name == "explicit status ID overrides local") && r.URL.Query().Get("statusId[]") != "1" {
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

func TestExecuteIssueListStatusNames(t *testing.T) {
	var paths []string
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		paths = append(paths, r.URL.Path)
		switch r.URL.Path {
		case "/api/v2/projects/42/statuses":
			_, _ = fmt.Fprint(w, `[{"id":1,"name":"未対応"},{"id":2,"name":"123"},{"id":3,"name":"a,b"}]`)
		case "/api/v2/issues/count", "/api/v2/issues":
			if !reflect.DeepEqual(r.URL.Query()["statusId[]"], []string{"1", "2", "3"}) {
				t.Errorf("status filter=%v", r.URL.Query())
			}
			if r.URL.Path == "/api/v2/issues/count" {
				_, _ = fmt.Fprint(w, `{"count":0}`)
			} else {
				_, _ = fmt.Fprint(w, `[]`)
			}
		default:
			t.Errorf("unexpected path=%s", r.URL.Path)
		}
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"issue", "list", "-p42", "-s未対応", "--status=123", "-sa,b", "-s未対応", "--json"}, &stdout, &stderr, server.Client())
	if exit != 0 || stderr.Len() != 0 || stdout.String() != "{\"items\":[],\"pagination\":{\"offset\":0,\"limit\":20,\"count\":0,\"total\":0}}\n" || strings.Join(paths, ",") != "/api/v2/projects/42/statuses,/api/v2/issues/count,/api/v2/issues" {
		t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
	}
}

func TestExecuteIssueListStatusDefaultsAndAll(t *testing.T) {
	for _, tc := range []struct {
		name, local, want string
		flags             []string
		candidates        bool
		exit              int
	}{
		{"local names", `["未対応"]`, "1", nil, true, 0},
		{"empty local", `[]`, "", nil, false, 0},
		{"explicit name replaces local", `["missing"]`, "2", []string{"-s完了"}, true, 0},
		{"explicit ID replaces local", `["missing"]`, "3", []string{"--status-id=3"}, false, 0},
		{"all removes local", `["missing"]`, "", []string{"--status=all"}, false, 0},
		{"literal all by ID", `[]`, "4", []string{"--status-id=4"}, false, 0},
		{"all and name", `[]`, "", []string{"-sall", "-s完了"}, false, 2},
		{"all repeated", `[]`, "", []string{"-sall", "--status=all"}, false, 2},
		{"all and ID", `[]`, "", []string{"-sall", "--status-id=1"}, false, 2},
		{"name and ID", `[]`, "", []string{"-s完了", "--status-id=1"}, false, 2},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				switch r.URL.Path {
				case "/api/v2/projects/42/statuses":
					if !tc.candidates {
						t.Error("unnecessary statuses request")
					}
					_, _ = fmt.Fprint(w, `[{"id":1,"name":"未対応"},{"id":2,"name":"完了"},{"id":4,"name":"all"}]`)
				case "/api/v2/issues/count", "/api/v2/issues":
					if r.URL.Query().Get("statusId[]") != tc.want {
						t.Errorf("query=%v", r.URL.Query())
					}
					if r.URL.Path == "/api/v2/issues/count" {
						_, _ = fmt.Fprint(w, `{"count":0}`)
					} else {
						_, _ = fmt.Fprint(w, `[]`)
					}
				default:
					t.Errorf("unexpected path=%s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			if err := os.Mkdir(".backlog", 0o700); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(".backlog/settings.json", []byte(`{"project":"42","issue":{"list":{"status":`+tc.local+`}}}`), 0o600); err != nil {
				t.Fatal(err)
			}
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(context.Background(), append([]string{"issue", "list", "--json"}, tc.flags...), &stdout, &stderr, server.Client())
			wantCalls := 2
			if tc.candidates {
				wantCalls++
			}
			if exit != tc.exit || (exit == 0 && (stderr.Len() != 0 || len(paths) != wantCalls)) || (exit != 0 && (len(paths) != 0 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"input_error"`))) {
				t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueListMilestoneNames(t *testing.T) {
	var paths []string
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		paths = append(paths, r.URL.Path)
		switch r.URL.Path {
		case "/api/v2/projects/42/versions":
			_, _ = fmt.Fprint(w, `[{"id":7,"name":"v1"},{"id":8,"name":"123"},{"id":9,"name":"a,b"}]`)
		case "/api/v2/issues/count", "/api/v2/issues":
			if !reflect.DeepEqual(r.URL.Query()["milestoneId[]"], []string{"7", "8", "9"}) {
				t.Errorf("query=%v", r.URL.Query())
			}
			if r.URL.Path == "/api/v2/issues/count" {
				_, _ = fmt.Fprint(w, `{"count":0}`)
			} else {
				_, _ = fmt.Fprint(w, `[]`)
			}
		default:
			t.Errorf("unexpected path=%s", r.URL.Path)
		}
	}))
	defer server.Close()
	configure(t, server.URL)
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"issue", "list", "-p42", "-mv1", "--milestone=123", "-ma,b", "-mv1", "--json"}, &stdout, &stderr, server.Client())
	if exit != 0 || stderr.Len() != 0 || !strings.Contains(stdout.String(), `"items":[]`) || strings.Join(paths, ",") != "/api/v2/projects/42/versions,/api/v2/issues/count,/api/v2/issues" {
		t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
	}
}

func TestExecuteIssueListAssigneeNamesAndSelf(t *testing.T) {
	for _, tc := range []struct {
		name                      string
		flags, wantIDs, wantPaths []string
	}{
		{"self only", []string{"-aself", "--assignee=self"}, []string{"10"}, []string{"/api/v2/users/myself"}},
		{"literal self", []string{"--assignee-name=self"}, []string{"11"}, []string{"/api/v2/projects/42/users"}},
		{"combined names", []string{"-aself", "-a山田", "--assignee-name=self", "--assignee-name=本人", "-a123", "-aa,b", "-a山田"}, []string{"10", "11", "12", "13", "14"}, []string{"/api/v2/users/myself", "/api/v2/projects/42/users"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				switch r.URL.Path {
				case "/api/v2/users/myself":
					_, _ = fmt.Fprint(w, `{"id":10,"name":"本人"}`)
				case "/api/v2/projects/42/users":
					_, _ = fmt.Fprint(w, `[{"id":10,"name":"本人"},{"id":11,"name":"self"},{"id":12,"name":"山田"},{"id":13,"name":"123"},{"id":14,"name":"a,b"}]`)
				case "/api/v2/issues/count", "/api/v2/issues":
					actual := r.URL.Query()["assigneeId[]"]
					slices.Sort(actual)
					if !reflect.DeepEqual(actual, tc.wantIDs) {
						t.Errorf("query=%v want assignees=%v", r.URL.Query(), tc.wantIDs)
					}
					if r.URL.Path == "/api/v2/issues/count" {
						_, _ = fmt.Fprint(w, `{"count":0}`)
					} else {
						_, _ = fmt.Fprint(w, `[]`)
					}
				default:
					t.Errorf("unexpected path=%s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(context.Background(), append([]string{"issue", "list", "-p42", "--json"}, tc.flags...), &stdout, &stderr, server.Client())
			wantPaths := append(append([]string{}, tc.wantPaths...), "/api/v2/issues/count", "/api/v2/issues")
			if exit != 0 || stderr.Len() != 0 || !strings.Contains(stdout.String(), `"items":[]`) || !reflect.DeepEqual(paths, wantPaths) {
				t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueMineSharesListSearchAndOutput(t *testing.T) {
	for _, mode := range []string{"--json", "--no-input"} {
		t.Run(mode, func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				switch r.URL.Path {
				case "/api/v2/users/myself":
					_, _ = fmt.Fprint(w, `{"id":10}`)
				case "/api/v2/projects/42/statuses":
					_, _ = fmt.Fprint(w, `[{"id":1,"name":"未対応"}]`)
				case "/api/v2/projects/42/versions":
					_, _ = fmt.Fprint(w, `[{"id":7,"name":"v1"}]`)
				case "/api/v2/issues/count", "/api/v2/issues":
					want := url.Values{"projectId[]": {"42"}, "statusId[]": {"1"}, "assigneeId[]": {"10"}, "milestoneId[]": {"7"}, "keyword": {"検索"}}
					if r.URL.Path == "/api/v2/issues/count" {
						_, _ = fmt.Fprint(w, `{"count":9}`)
					} else {
						want.Set("count", "2")
						want.Set("offset", "3")
						want.Set("sort", "updated")
						want.Set("order", "desc")
						_, _ = fmt.Fprint(w, `[{"issueKey":"APP-1","summary":"件名","status":{"name":"未対応"},"assignee":{"name":"本人"},"future":true}]`)
					}
					if !reflect.DeepEqual(r.URL.Query(), want) {
						t.Errorf("query=%v want=%v", r.URL.Query(), want)
					}
				default:
					t.Errorf("unexpected path=%s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			if err := os.Mkdir(".backlog", 0o700); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(".backlog/settings.json", []byte(`{"project":"42","issue":{"list":{"status":["未対応"]}}}`), 0o600); err != nil {
				t.Fatal(err)
			}
			var outputs []string
			for _, command := range []string{"list", "mine"} {
				args := []string{"issue", command, "検索", "-mv1", "--limit=2", "--offset=3", mode}
				if command == "list" {
					args = append(args, "-aself")
				}
				var stdout, stderr bytes.Buffer
				exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, server.Client())
				if exit != 0 || stderr.Len() != 0 {
					t.Fatalf("%s exit=%d stdout=%s stderr=%s", command, exit, &stdout, &stderr)
				}
				outputs = append(outputs, stdout.String())
			}
			if outputs[0] != outputs[1] || !strings.Contains(outputs[1], "APP-1") {
				t.Fatalf("list=%q mine=%q", outputs[0], outputs[1])
			}
			if mode == "--json" && !strings.Contains(outputs[1], `"pagination":{"offset":3,"limit":2,"count":1,"total":9}`) {
				t.Fatal(outputs[1])
			}
			if mode == "--no-input" && outputs[1] != "KEY\tSUMMARY\tSTATUS\tASSIGNEE\nAPP-1\t件名\t未対応\t本人\nNext offset: 4\n" {
				t.Fatal(outputs[1])
			}
		})
	}
}

func TestExecuteIssueCrossProjectSearch(t *testing.T) {
	for _, tc := range []struct {
		name, command, keyword string
		flags                  []string
		self                   bool
		exit                   int
	}{
		{"keyword", "list", "検索", nil, false, 0},
		{"self", "list", "", []string{"-aself", "--assignee=self"}, true, 0},
		{"keyword and self", "list", " 検索 ", []string{"-aself"}, true, 0},
		{"mine", "mine", "", nil, true, 0},
		{"mine keyword", "mine", "検索", nil, true, 0},
		{"unconditional", "list", "", nil, false, 2},
		{"whitespace keyword", "list", " \t\n", nil, false, 2},
		{"status name", "list", "検索", []string{"-s完了"}, false, 2},
		{"status all", "list", "検索", []string{"-sall"}, false, 2},
		{"status ID", "list", "検索", []string{"--status-id=1"}, false, 2},
		{"milestone name", "list", "検索", []string{"-mv1"}, false, 2},
		{"milestone ID", "list", "検索", []string{"--milestone-id=1"}, false, 2},
		{"ordinary assignee", "list", "検索", []string{"-a山田"}, false, 2},
		{"self and ordinary", "list", "検索", []string{"-aself", "-a山田"}, false, 2},
		{"literal self", "list", "検索", []string{"--assignee-name=self"}, false, 2},
		{"assignee ID", "list", "検索", []string{"--assignee-id=10"}, false, 2},
		{"mine status", "mine", "検索", []string{"-sall"}, false, 2},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var paths []string
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				paths = append(paths, r.URL.Path)
				switch r.URL.Path {
				case "/api/v2/users/myself":
					_, _ = fmt.Fprint(w, `{"id":10}`)
				case "/api/v2/issues/count", "/api/v2/issues":
					want := url.Values{}
					if tc.keyword != "" {
						want.Set("keyword", tc.keyword)
					}
					if tc.self {
						want.Add("assigneeId[]", "10")
					}
					if r.URL.Path == "/api/v2/issues/count" {
						_, _ = fmt.Fprint(w, `{"count":0}`)
					} else {
						want.Set("count", "2")
						want.Set("offset", "3")
						want.Set("sort", "updated")
						want.Set("order", "desc")
						_, _ = fmt.Fprint(w, `[]`)
					}
					if !reflect.DeepEqual(r.URL.Query(), want) {
						t.Errorf("query=%v want=%v", r.URL.Query(), want)
					}
				default:
					t.Errorf("unexpected request=%s", r.URL.Path)
				}
			}))
			defer server.Close()
			configure(t, server.URL)
			if err := os.Mkdir(".backlog", 0o700); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(".backlog/settings.json", []byte(`{"project":"IGNORED","issue":{"list":{"status":["missing"]},"create":{"assignee":"missing"}}}`), 0o600); err != nil {
				t.Fatal(err)
			}
			args := []string{"issue", tc.command, "--project=all", "--limit=2", "--offset=3", "--json"}
			if tc.keyword != "" {
				args = append(args, tc.keyword)
			}
			args = append(args, tc.flags...)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, server.Client())
			wantPaths := []string{"/api/v2/issues/count", "/api/v2/issues"}
			if tc.self {
				wantPaths = append([]string{"/api/v2/users/myself"}, wantPaths...)
			}
			if tc.exit != 0 {
				wantPaths = nil
			}
			if exit != tc.exit || !reflect.DeepEqual(paths, wantPaths) || (exit == 0 && (stderr.Len() != 0 || stdout.String() != "{\"items\":[],\"pagination\":{\"offset\":3,\"limit\":2,\"count\":0,\"total\":0}}\n")) || (exit != 0 && (stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"input_error"`))) {
				t.Fatalf("exit=%d paths=%v stdout=%s stderr=%s", exit, paths, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueSearchRejectsMixedNamesAndIDsAndMineAssignees(t *testing.T) {
	for _, command := range []string{"list", "mine"} {
		cases := [][]string{
			{"-s完了", "--status-id=1"},
			{"-mv1", "--milestone-id=7"},
			{"-aself", "--assignee-id=10"},
			{"--assignee-name=self", "--assignee-id=11"},
			{"--project=42", "-p42"},
			{"--limit=1", "--limit=2"},
			{"--offset=0", "--offset=1"},
		}
		if command == "mine" {
			for _, flag := range []string{"--assignee=self", "-aself", "--assignee=山田", "--assignee=", "--assignee-name=self", "--assignee-name=", "--assignee-id=10", "--assignee-id="} {
				cases = append(cases, []string{flag})
			}
		}
		for _, flags := range cases {
			t.Run(command+" "+strings.Join(flags, " "), func(t *testing.T) {
				server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { t.Errorf("invalid input called %s", r.URL.Path) }))
				defer server.Close()
				configure(t, server.URL)
				var stdout, stderr bytes.Buffer
				exit := cli.Execute(context.Background(), append([]string{"issue", command, "--json"}, flags...), &stdout, &stderr, server.Client())
				if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"input_error"`) {
					t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
				}
			})
		}
	}
}

func TestExecuteIssueNameResolutionFailures(t *testing.T) {
	for _, field := range []struct{ flag, endpoint string }{{"--status", "statuses"}, {"--milestone", "versions"}, {"--assignee", "users"}, {"--assignee-name", "users"}} {
		for _, tc := range []struct {
			name, body, code string
			status           int
		}{
			{"missing", `[]`, "input_error", 200},
			{"not exact", `[{"id":1,"name":" target "},{"id":2,"name":"TARGET"}]`, "input_error", 200},
			{"ambiguous", `[{"id":1,"name":"target"},{"id":2,"name":"target"}]`, "input_error", 200},
			{"duplicate same ID", `[{"id":1,"name":"target"},{"id":1,"name":"target"}]`, "input_error", 200},
			{"missing ID", `[{"name":"target"}]`, "response_error", 200},
			{"zero ID", `[{"id":0,"name":"target"}]`, "response_error", 200},
			{"negative ID", `[{"id":-1,"name":"target"}]`, "response_error", 200},
			{"overflow ID", `[{"id":9223372036854775808,"name":"target"}]`, "response_error", 200},
			{"string ID", `[{"id":"1","name":"target"}]`, "response_error", 200},
			{"missing name", `[{"id":1}]`, "response_error", 200},
			{"null name", `[{"id":1,"name":null}]`, "response_error", 200},
			{"empty name", `[{"id":1,"name":""}]`, "response_error", 200},
			{"wrong name type", `[{"id":1,"name":1}]`, "response_error", 200},
			{"null list", `null`, "response_error", 200},
			{"nonobject", `[null]`, "response_error", 200},
			{"malformed", `[`, "response_error", 200},
			{"API rejected", `{"errors":[{"code":11,"message":"test-secret+/?日本"}]}`, "api_error", 403},
		} {
			t.Run(field.flag+"/"+tc.name, func(t *testing.T) {
				calls := 0
				server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					calls++
					if r.URL.Path != "/api/v2/projects/42/"+field.endpoint {
						t.Errorf("unexpected path=%s", r.URL.Path)
					}
					w.WriteHeader(tc.status)
					_, _ = fmt.Fprint(w, tc.body)
				}))
				defer server.Close()
				configure(t, server.URL)
				var stdout, stderr bytes.Buffer
				exit := cli.Execute(context.Background(), []string{"issue", "list", "-p42", field.flag + "=target", "--json"}, &stdout, &stderr, server.Client())
				wantExit := 1
				if tc.code == "input_error" {
					wantExit = 2
				}
				if exit != wantExit || calls != 1 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"`+tc.code+`"`) || strings.Contains(stderr.String(), "test-secret") {
					t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls, &stdout, &stderr)
				}
			})
		}
	}
}

func TestExecuteIssueSelfUsesSelectedProfile(t *testing.T) {
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Backlog-API-Key") != "work-secret" {
			t.Error("self did not use selected profile")
		}
		switch r.URL.Path {
		case "/api/v2/users/myself":
			_, _ = fmt.Fprint(w, `{"id":20}`)
		case "/api/v2/issues/count", "/api/v2/issues":
			if !reflect.DeepEqual(r.URL.Query()["assigneeId[]"], []string{"20"}) {
				t.Errorf("query=%v", r.URL.Query())
			}
			if r.URL.Path == "/api/v2/issues/count" {
				_, _ = fmt.Fprint(w, `{"count":0}`)
			} else {
				_, _ = fmt.Fprint(w, `[]`)
			}
		default:
			t.Errorf("unexpected path=%s", r.URL.Path)
		}
	}))
	defer server.Close()
	configure(t, "https://default.invalid")
	t.Setenv("WORK_TEST_KEY", "work-secret")
	if err := os.WriteFile(filepath.Join(os.Getenv("XDG_CONFIG_HOME"), "backlog", "config.json"), []byte(fmt.Sprintf(`{"profiles":{"default":{"spaceUrl":"https://default.invalid","apiKeyEnv":"SLICE_TEST_KEY"},"work":{"spaceUrl":%q,"apiKeyEnv":"WORK_TEST_KEY"}}}`, server.URL)), 0o600); err != nil {
		t.Fatal(err)
	}
	var stdout, stderr bytes.Buffer
	exit := cli.Execute(context.Background(), []string{"--profile=work", "issue", "mine", "-pall", "--json"}, &stdout, &stderr, server.Client())
	if exit != 0 || stderr.Len() != 0 || !strings.Contains(stdout.String(), `"items":[]`) {
		t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
	}
}

func TestExecuteIssueSelfFailuresStopSearch(t *testing.T) {
	for _, tc := range []struct {
		name, body, code string
		status           int
		truncated        bool
	}{
		{"missing ID", `{}`, "response_error", 200, false},
		{"null ID", `{"id":null}`, "response_error", 200, false},
		{"zero ID", `{"id":0}`, "response_error", 200, false},
		{"negative ID", `{"id":-1}`, "response_error", 200, false},
		{"overflow ID", `{"id":9223372036854775808}`, "response_error", 200, false},
		{"fractional ID", `{"id":1.5}`, "response_error", 200, false},
		{"string ID", `{"id":"1"}`, "response_error", 200, false},
		{"null object", `null`, "response_error", 200, false},
		{"array", `[]`, "response_error", 200, false},
		{"malformed", `{`, "response_error", 200, false},
		{"API rejected", `{"errors":[{"code":11,"message":"test-secret+/?日本"}]}`, "api_error", 403, false},
		{"unexpected success", `{"id":1}`, "response_error", 201, false},
		{"communication", `{}`, "transport_error", 200, true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			calls := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls++
				if r.URL.Path != "/api/v2/users/myself" {
					t.Errorf("unexpected path=%s", r.URL.Path)
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
			exit := cli.Execute(context.Background(), []string{"issue", "mine", "-pall", "--json"}, &stdout, &stderr, server.Client())
			if exit != 1 || calls != 1 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"`+tc.code+`"`) || !strings.Contains(stderr.String(), fmt.Sprintf(`"httpStatus":%d`, tc.status)) || strings.Contains(stderr.String(), "test-secret") {
				t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueCandidateCancellationStopsSearch(t *testing.T) {
	for _, tc := range []struct{ flag, path string }{{"-s未対応", "/api/v2/projects/42/statuses"}, {"-aself", "/api/v2/users/myself"}} {
		t.Run(tc.flag, func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			calls := 0
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls++
				if r.URL.Path != tc.path {
					t.Errorf("unexpected path=%s", r.URL.Path)
				}
				_, _ = fmt.Fprint(w, `{`)
				w.(http.Flusher).Flush()
				cancel()
				<-r.Context().Done()
			}))
			defer server.Close()
			configure(t, server.URL)
			var stdout, stderr bytes.Buffer
			exit := cli.Execute(ctx, []string{"issue", "list", "-p42", tc.flag, "--json"}, &stdout, &stderr, server.Client())
			if exit != 130 || calls != 1 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"cancelled"`) {
				t.Fatalf("exit=%d calls=%d stdout=%s stderr=%s", exit, calls, &stdout, &stderr)
			}
		})
	}
}
