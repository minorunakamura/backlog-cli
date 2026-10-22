package main_test

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestBinaryStreamsExitAndInterrupt(t *testing.T) {
	binary := filepath.Join(t.TempDir(), "bl")
	build := exec.Command("go", "build", "-o", binary, ".")
	if data, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build: %v\n%s", err, data)
	}
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	for _, tc := range []struct {
		name string
		args []string
		exit int
		code string
	}{
		{"help", []string{"--help"}, 0, ""},
		{"parser", []string{"project", "list", "--unknown", "--json"}, 2, "input_error"},
		{"config", []string{"priority", "list", "--json"}, 2, "config_error"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
			defer cancel()
			command := exec.CommandContext(ctx, binary, tc.args...)
			var stdout, stderr bytes.Buffer
			command.Stdout = &stdout
			command.Stderr = &stderr
			err := command.Run()
			exit := 0
			if err != nil {
				if e, ok := err.(*exec.ExitError); ok {
					exit = e.ExitCode()
				} else {
					t.Fatal(err)
				}
			}
			if exit != tc.exit {
				t.Fatalf("exit=%d stdout=%s stderr=%s", exit, &stdout, &stderr)
			}
			if tc.exit == 0 {
				if !strings.Contains(stdout.String(), "Usage:") || stderr.Len() != 0 {
					t.Fatalf("stdout=%s stderr=%s", &stdout, &stderr)
				}
			} else if stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"`+tc.code+`"`) {
				t.Fatalf("stdout=%s stderr=%s", &stdout, &stderr)
			}
		})
	}
	t.Run("SIGINT cancels TLS handshake", func(t *testing.T) {
		started, release := make(chan struct{}), make(chan struct{})
		server := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { t.Error("API handler must not be reached") }))
		server.TLS = &tls.Config{GetConfigForClient: func(*tls.ClientHelloInfo) (*tls.Config, error) { close(started); <-release; return nil, nil }}
		server.StartTLS()
		defer server.Close()
		defer close(release)
		base := t.TempDir()
		t.Setenv("XDG_CONFIG_HOME", base)
		t.Setenv("BINARY_TEST_KEY", "not-a-real-api-key")
		if err := os.Mkdir(filepath.Join(base, "backlog"), 0o700); err != nil {
			t.Fatal(err)
		}
		data, err := json.Marshal(map[string]any{"profiles": map[string]any{"default": map[string]string{"spaceUrl": server.URL, "apiKeyEnv": "BINARY_TEST_KEY"}}})
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(base, "backlog", "config.json"), data, 0o600); err != nil {
			t.Fatal(err)
		}
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		command := exec.CommandContext(ctx, binary, "project", "list", "--json")
		command.Dir = t.TempDir()
		var stdout, stderr bytes.Buffer
		command.Stdout = &stdout
		command.Stderr = &stderr
		if err := command.Start(); err != nil {
			t.Fatal(err)
		}
		done := make(chan error, 1)
		go func() { done <- command.Wait() }()
		select {
		case <-started:
		case err := <-done:
			t.Fatalf("exited before handshake: %v stderr=%s", err, &stderr)
		case <-ctx.Done():
			t.Fatal("handshake did not start")
		}
		if err := command.Process.Signal(os.Interrupt); err != nil {
			t.Fatal(err)
		}
		err = <-done
		exitError, ok := err.(*exec.ExitError)
		if !ok || exitError.ExitCode() != 130 || stdout.Len() != 0 || !strings.Contains(stderr.String(), `"code":"cancelled"`) {
			t.Fatalf("error=%v stdout=%s stderr=%s", err, &stdout, &stderr)
		}
	})
}
