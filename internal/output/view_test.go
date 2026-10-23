package output_test

import (
	"backlog-cli/internal/markup"
	"backlog-cli/internal/output"
	"bytes"
	"encoding/base64"
	"encoding/json"
	"io"
	"strings"
	"testing"
)

func TestWriteIssueDetailRedactsSecretsAcrossRenderedStyles(t *testing.T) {
	encodedKey := base64.StdEncoding.EncodeToString([]byte("mock-only-secret"))
	for _, tc := range []struct {
		name, source, key, visibleSecret string
	}{
		{"raw key", "mock-only-**secret**", "mock-only-secret", "mock-only-secret"},
		{"URL encoded key", "mock%2Fkey%2B**secret**", "mock/key+secret", "mock%2Fkey%2Bsecret"},
		{"base64 key", "bW9jay1vbmx5**LXNlY3JldA==**", "mock-only-secret", encodedKey},
		{"ordinary styles", "ordinary **styled** content", "mock-only-secret", ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			for _, tty := range []bool{false, true} {
				rendered, err := markup.Render(tc.source, "https://example.test", tty)
				if err != nil {
					t.Fatal(err)
				}
				var stdout bytes.Buffer
				raw := json.RawMessage(`{"issueKey":"APP-12","summary":"Summary"}`)
				if err := output.WriteIssueDetail(&stdout, raw, "https://example.test", rendered, tty, tc.key); err != nil {
					t.Fatal(err)
				}
				plain := output.SafeText(stdout.String())
				if tc.visibleSecret != "" {
					if strings.Contains(plain, tc.key) || strings.Contains(plain, tc.visibleSecret) || !strings.Contains(plain, "[REDACTED]") {
						t.Errorf("TTY=%v: rendered secret not redacted: %q", tty, plain)
					}
				} else if !strings.Contains(stdout.String(), rendered) || tty && !strings.Contains(stdout.String(), "\x1b[") {
					t.Errorf("TTY=%v: ordinary styles changed: %q", tty, stdout.String())
				}
				if !tty && strings.ContainsRune(stdout.String(), '\x1b') {
					t.Errorf("ANSI on pipe: %q", stdout.String())
				}
			}
		})
	}
}

func TestWriteIssuePreservesAPIObjectAndAddsURL(t *testing.T) {
	raw := json.RawMessage(`{"issueKey":"APP-12","description":"* 本文\n日本語👩‍💻\r\n\t\u001b[31m\u0007 SECRET","future":{"number":12345678901234567890,"nested":[null,true,{"keep":"値"}]},"url":"https://untrusted.test"}`)
	var stdout bytes.Buffer
	if err := output.WriteIssue(&stdout, raw, "https://example.test", "SECRET"); err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(stdout.Bytes(), &fields); err != nil {
		t.Fatal(err)
	}
	var body, issueURL string
	if err := json.Unmarshal(fields["description"], &body); err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(fields["url"], &issueURL); err != nil {
		t.Fatal(err)
	}
	if body != "* 本文\n日本語👩‍💻\r\n\t\x1b[31m\a [REDACTED]" || issueURL != "https://example.test/view/APP-12" || string(fields["future"]) != `{"nested":[null,true,{"keep":"値"}],"number":12345678901234567890}` || !strings.HasSuffix(stdout.String(), "\n") {
		t.Fatalf("lost API values: %s", &stdout)
	}
	decoder := json.NewDecoder(&stdout)
	var document any
	if err := decoder.Decode(&document); err != nil {
		t.Fatal(err)
	}
	if err := decoder.Decode(&document); err != io.EOF {
		t.Fatalf("extra output: %v", err)
	}
}
