package cli_test

import (
	"backlog-cli/internal/cli"
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestExecuteIssueCreateNamesMustMatchExactlyOnce(t *testing.T) {
	for _, flag := range []string{"--issue-type", "--priority", "--assignee", "--assignee-name"} {
		for _, name := range []string{"missing", "duplicate", " self "} {
			t.Run(flag+name, func(t *testing.T) {
				posts := 0
				server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					if r.Method == "POST" {
						posts++
						t.Error("unresolved candidate POSTed")
						return
					}
					_, _ = fmt.Fprint(w, `[{"id":1,"name":"duplicate"},{"id":2,"name":"duplicate"},{"id":3,"name":"self"}]`)
				}))
				defer server.Close()
				configure(t, server.URL)
				args := []string{"issue", "create", "-p42", "-t件名", "--json"}
				if flag != "--issue-type" {
					args = append(args, "--issue-type-id=7")
				}
				if flag != "--priority" {
					args = append(args, "--priority-id=3")
				}
				args = append(args, flag+"="+name)
				var stdout, stderr bytes.Buffer
				exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, server.Client())
				if exit != 2 || posts != 0 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"state":"not_created"`) {
					t.Fatalf("exit=%d posts=%d stdout=%s stderr=%s", exit, posts, &stdout, &stderr)
				}
			})
		}
	}
}

func TestExecuteIssueCreateFlagValuesDoNotEnableJSONErrors(t *testing.T) {
	for _, flag := range []string{"--project", "-p", "--issue-type", "--issue-type-id", "--priority", "--priority-id", "--title", "-t", "--body", "-b", "--body-file", "--assignee", "-a", "--assignee-name", "--assignee-id", "--profile"} {
		var stdout, stderr bytes.Buffer
		exit := cli.ExecuteWithInput(context.Background(), []string{"issue", "create", "--unknown", flag, "--json"}, unreadableInput{t}, &stdout, &stderr, nil)
		if exit != 2 || stdout.Len() != 0 || stderr.Len() == 0 || strings.HasPrefix(stderr.String(), "{") || !strings.Contains(stderr.String(), "not_created") {
			t.Fatalf("flag=%s exit=%d stdout=%s stderr=%s", flag, exit, &stdout, &stderr)
		}
	}
}

func TestExecuteIssueCreateRejectsInputBeforeAPI(t *testing.T) {
	cases := [][]string{
		{"--unknown"},
		{"extra"},
		{"--body=", "--body-file=-"},
		{"--body=", "--body-file="},
		{"--issue-type=x", "--issue-type-id=7"},
		{"--priority=x", "--priority-id=3"},
		{"--assignee=self", "--assignee-name=self"},
		{"--assignee=self", "--assignee-id=1"},
		{"--assignee-name=self", "--assignee-id=1"},
		{"--assignee=self", "--no-assignee"},
		{"--assignee-name=self", "--no-assignee"},
		{"--assignee-id=1", "--no-assignee"},
		{"--body-file=missing"},
		{"--body-file=invalid.txt"},
		{"--body=" + string([]byte{0xff})},
	}
	for _, flag := range []string{"project", "issue-type", "issue-type-id", "priority", "priority-id", "title", "assignee", "assignee-name", "assignee-id", "body", "body-file", "profile"} {
		cases = append(cases, []string{"--" + flag + "=x", "--" + flag + "=x"})
	}
	for _, pair := range [][2]string{{"project", "p"}, {"title", "t"}, {"body", "b"}, {"assignee", "a"}} {
		cases = append(cases, []string{"--" + pair[0] + "=x", "-" + pair[1] + "x"}, []string{"-" + pair[1] + "x", "--" + pair[0] + "=x"})
	}
	for _, flag := range []string{"issue-type-id", "priority-id", "assignee-id"} {
		for _, value := range []string{"", "0", "-1", "9223372036854775808", "1.0", "1,2", " 1"} {
			cases = append(cases, []string{"--" + flag + "=" + value})
		}
	}
	for _, flags := range cases {
		t.Run(strings.Join(flags, " "), func(t *testing.T) {
			server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { t.Error("invalid input called API") }))
			defer server.Close()
			configure(t, server.URL)
			if err := os.WriteFile("invalid.txt", []byte{0xff}, 0o600); err != nil {
				t.Fatal(err)
			}
			// Supply required fields unless the case tests that same flag.
			args := []string{"issue", "create", "--json"}
			for _, base := range []string{"-p42", "--issue-type-id=7", "--priority-id=3", "-t件名"} {
				item := strings.SplitN(base, "=", 2)[0]
				omit := false
				for _, flag := range flags {
					if item == "-p42" && (strings.HasPrefix(flag, "--project") || strings.HasPrefix(flag, "-p")) || item == "-t件名" && (strings.HasPrefix(flag, "--title") || strings.HasPrefix(flag, "-t")) || strings.HasPrefix(item, "--issue-type") && strings.HasPrefix(flag, "--issue-type") || strings.HasPrefix(item, "--priority") && strings.HasPrefix(flag, "--priority") {
						omit = true
					}
				}
				if !omit {
					args = append(args, base)
				}
			}
			args = append(args, flags...)
			var stdout, stderr bytes.Buffer
			exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, server.Client())
			if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"input_error"`) || !strings.Contains(stderr.String(), `"creation":{"state":"not_created","issueKey":null,"url":null}`) {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
		})
	}
}

func TestExecuteIssueCreateMissingRequiredFieldsDoesNotReadInput(t *testing.T) {
	for _, flags := range [][]string{nil, {"--issue-type-id=7", "--priority-id=3", "-t件名"}, {"-p42", "--priority-id=3", "-t件名"}, {"-p42", "--issue-type-id=7", "-t件名"}, {"-p42", "--issue-type-id=7", "--priority-id=3"}, {"-p42", "--issue-type-id=7", "--priority-id=3", "--title= \t\n"}} {
		server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { t.Error("missing input called API") }))
		configure(t, server.URL)
		var stdout, stderr bytes.Buffer
		exit := cli.ExecuteWithInput(context.Background(), append([]string{"issue", "create", "--json", "--body-file=-"}, flags...), unreadableInput{t}, &stdout, &stderr, server.Client())
		server.Close()
		if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"state":"not_created"`) || !strings.Contains(stderr.String(), "interactive create is a later issue") {
			t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
		}
	}
}

func TestExecuteIssueCreateInvalidUTF8AndUnreadableStdin(t *testing.T) {
	for _, input := range []io.Reader{strings.NewReader(string([]byte{0xff})), brokenBodyInput{}} {
		server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { t.Error("bad body called API") }))
		configure(t, server.URL)
		var stdout, stderr bytes.Buffer
		exit := cli.ExecuteWithInput(context.Background(), append(createArgs(), "--body-file=-"), input, &stdout, &stderr, server.Client())
		server.Close()
		if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"state":"not_created"`) {
			t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
		}
	}
}

func TestExecuteIssueCreateCancelsWhileReadingStdinWithoutEOF(t *testing.T) {
	var posts atomic.Int32
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost {
			posts.Add(1)
		}
	}))
	defer server.Close()
	configure(t, server.URL)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	reader, writer := io.Pipe()
	defer func() { _ = reader.Close() }()
	defer func() { _ = writer.Close() }()
	var stdout, stderr bytes.Buffer
	done := make(chan int, 1)
	go func() {
		done <- cli.ExecuteWithInput(ctx, append(createArgs(), "--body-file=-"), reader, &stdout, &stderr, server.Client())
	}()
	written := make(chan error, 1)
	go func() {
		_, err := io.WriteString(writer, "partial body")
		written <- err
		cancel() // The read has started, but stdin remains open without EOF.
	}()
	var exit int
	select {
	case exit = <-done:
	case <-time.After(time.Second):
		_ = reader.Close()
		_ = writer.Close()
		<-done
		<-written
		t.Fatal("create did not return on cancellation while stdin remained open")
	}
	if err := <-written; err != nil {
		t.Fatal(err)
	}
	if exit != 130 || posts.Load() != 0 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"cancelled"`) || !strings.Contains(stderr.String(), `"creation":{"state":"not_created","issueKey":null,"url":null}`) {
		t.Fatalf("exit=%d posts=%d stdout=%s stderr=%s", exit, posts.Load(), &stdout, &stderr)
	}
}

type brokenBodyInput struct{}

func (brokenBodyInput) Read([]byte) (int, error) { return 0, fmt.Errorf("test-secret+/?日本") }

func TestExecuteIssueCreateConfigAndParserErrorsHaveCreation(t *testing.T) {
	for _, args := range [][]string{createArgs(), {"--profile=x", "issue", "create", "--json"}, {"issue", "create", "--profile=x", "--json", "--unknown"}, {"--unknown", "issue", "create", "--json"}} {
		t.Setenv("XDG_CONFIG_HOME", t.TempDir())
		var stdout, stderr bytes.Buffer
		exit := cli.ExecuteWithInput(context.Background(), args, unreadableInput{t}, &stdout, &stderr, nil)
		if exit != 2 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"state":"not_created"`) {
			t.Fatalf("args=%v exit=%d stdout=%s stderr=%s", args, exit, &stdout, &stderr)
		}
	}
}
