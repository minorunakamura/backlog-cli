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
	"reflect"
	"strings"
	"syscall"
	"testing"
	"time"
	"unsafe"
)

// A real PTY exercises Execute's TTY gate without a production UI injection hook.
func issueTerminal(t *testing.T) (*os.File, *os.File) {
	t.Helper()
	master, err := os.OpenFile("/dev/ptmx", os.O_RDWR, 0)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = master.Close() })
	var name [128]byte
	for _, op := range []struct{ request, argument uintptr }{{syscall.TIOCPTYGRANT, 0}, {syscall.TIOCPTYUNLK, 0}, {syscall.TIOCPTYGNAME, uintptr(unsafe.Pointer(&name[0]))}} {
		if _, _, errno := syscall.Syscall(syscall.SYS_IOCTL, master.Fd(), op.request, op.argument); errno != 0 {
			t.Fatal(errno)
		}
	}
	slave, err := os.OpenFile(strings.TrimRight(string(name[:]), "\x00"), os.O_RDWR, 0)
	if err != nil {
		t.Fatal(err)
	}
	size := [4]uint16{24, 80, 0, 0}
	if _, _, errno := syscall.Syscall(syscall.SYS_IOCTL, slave.Fd(), syscall.TIOCSWINSZ, uintptr(unsafe.Pointer(&size[0]))); errno != 0 {
		t.Fatal(errno)
	}
	t.Cleanup(func() { _ = slave.Close() })
	return master, slave
}

func TestExecuteIssueListTerminalRefreshReusesResolvedSearch(t *testing.T) {
	requests := make(chan string, 32)
	count := 0
	details := 0
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		path := r.URL.Path
		switch path {
		case "/api/v2/projects/42/statuses":
			_, _ = fmt.Fprint(w, `[{"id":2,"name":"処理中"}]`)
		case "/api/v2/issues/count":
			count++
			if r.URL.Query().Encode() != "keyword=needle&projectId%5B%5D=42&statusId%5B%5D=2" {
				t.Errorf("count query=%s", r.URL.RawQuery)
			}
			_, _ = fmt.Fprint(w, `{"count":100}`)
		case "/api/v2/issues":
			query := r.URL.Query()
			offset := query.Get("offset")
			query.Del("offset")
			if query.Encode() != "count=2&keyword=needle&order=desc&projectId%5B%5D=42&sort=updated&statusId%5B%5D=2" {
				t.Errorf("list query=%v", query)
			}
			path += "?offset=" + offset
			_, _ = fmt.Fprintf(w, `[{"issueKey":"APP-%d","summary":"page %s"}]`, count, offset)
		case "/api/v2/issues/APP-1":
			details++
			_, _ = fmt.Fprintf(w, `{"issueKey":"APP-1","projectId":42,"description":"version%d"}`, details)
		case "/api/v2/projects/42":
			_, _ = fmt.Fprint(w, `{"textFormattingRule":"markdown"}`)
		default:
			t.Errorf("unexpected request %s", path)
		}
		requests <- path
	}))
	defer server.Close()
	configure(t, server.URL)
	master, slave := issueTerminal(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	var stderr bytes.Buffer
	done := make(chan int, 1)
	go func() {
		done <- cli.ExecuteWithInput(ctx, []string{"issue", "list", "needle", "-p42", "-s処理中", "--limit=2", "--offset=10"}, slave, slave, &stderr, server.Client())
	}()
	screen := make(chan string, 128)
	go func() {
		buf := make([]byte, 32768)
		for {
			n, err := master.Read(buf)
			if n > 0 {
				select {
				case screen <- string(buf[:n]):
				case <-ctx.Done():
					return
				}
			}
			if err != nil {
				return
			}
		}
	}()
	waitScreen := func(needle string) {
		t.Helper()
		var content string
		for {
			select {
			case chunk := <-screen:
				content += chunk
				if strings.Contains(content, needle) {
					return
				}
			case <-ctx.Done():
				t.Fatalf("waiting for %q: %q", needle, content)
			}
		}
	}
	send := func(key string) {
		t.Helper()
		if _, err := io.WriteString(master, key); err != nil {
			t.Fatal(err)
		}
	}
	waitScreen("Issues")
	send("n")
	waitScreen("offset 11")
	send("p")
	waitScreen("offset 10")
	send("l")
	waitScreen("h/q back")
	send("R")
	waitScreen("version2")
	send("h")
	waitScreen("Issues")
	send("R")
	waitScreen("offset 0")
	send("q")
	select {
	case exit := <-done:
		if exit != 0 || stderr.Len() != 0 {
			t.Fatalf("exit=%d stderr=%s", exit, &stderr)
		}
	case <-ctx.Done():
		t.Fatal("UI did not exit")
	}
	close(requests)
	var got []string
	for request := range requests {
		got = append(got, request)
	}
	want := []string{"/api/v2/projects/42/statuses", "/api/v2/issues/count", "/api/v2/issues?offset=10", "/api/v2/issues?offset=11", "/api/v2/issues/APP-1", "/api/v2/projects/42", "/api/v2/issues/APP-1", "/api/v2/projects/42", "/api/v2/issues/count", "/api/v2/issues?offset=0"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("requests=%v want=%v", got, want)
	}
}

func TestExecuteIssueViewTerminalRefreshFetchesSameIssueAndNotation(t *testing.T) {
	requests := make(chan string, 16)
	details := 0
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requests <- r.URL.Path
		switch r.URL.Path {
		case "/api/v2/issues/123":
			details++
			_, _ = fmt.Fprintf(w, `{"issueKey":"APP-1","projectId":42,"description":"version%d"}`, details)
		case "/api/v2/projects/42":
			if details == 2 {
				_, _ = fmt.Fprint(w, `{"textFormattingRule":"unknown"}`)
			} else {
				_, _ = fmt.Fprint(w, `{"textFormattingRule":"markdown"}`)
			}
		default:
			t.Errorf("unexpected request %s", r.URL.Path)
		}
	}))
	defer server.Close()
	configure(t, server.URL)
	master, slave := issueTerminal(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	var stderr bytes.Buffer
	done := make(chan int, 1)
	go func() {
		done <- cli.ExecuteWithInput(ctx, []string{"issue", "view", "123"}, slave, slave, &stderr, server.Client())
	}()
	screen := make(chan string, 128)
	go func() {
		buf := make([]byte, 32768)
		for {
			n, err := master.Read(buf)
			if n > 0 {
				select {
				case screen <- string(buf[:n]):
				case <-ctx.Done():
					return
				}
			}
			if err != nil {
				return
			}
		}
	}()
	waitScreen := func(needle string) string {
		t.Helper()
		var content string
		for {
			select {
			case chunk := <-screen:
				content += chunk
				if strings.Contains(content, needle) {
					return content
				}
			case <-ctx.Done():
				t.Fatalf("waiting for %q: %q", needle, content)
			}
		}
	}
	send := func(key string) {
		t.Helper()
		if _, err := io.WriteString(master, key); err != nil {
			t.Fatal(err)
		}
	}
	initial := waitScreen("q close")
	if strings.Contains(initial, "h/q") {
		t.Fatal(initial)
	}
	send("R")
	waitScreen("unknown project")
	send("R")
	waitScreen("version3")
	send("q")
	select {
	case exit := <-done:
		if exit != 0 || stderr.Len() != 0 {
			t.Fatalf("exit=%d stderr=%s", exit, &stderr)
		}
	case <-ctx.Done():
		t.Fatal("UI did not exit")
	}
	close(requests)
	var got []string
	for request := range requests {
		got = append(got, request)
	}
	want := []string{"/api/v2/issues/123", "/api/v2/projects/42", "/api/v2/issues/123", "/api/v2/projects/42", "/api/v2/issues/123", "/api/v2/projects/42"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("requests=%v", got)
	}
}
