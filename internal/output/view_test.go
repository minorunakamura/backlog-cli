package output_test

import (
	"backlog-cli/internal/markup"
	"backlog-cli/internal/output"
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"strings"
	"testing"
)

func TestWriteIssueDetailRedactsSecretsAcrossRenderedStyles(t *testing.T) {
	encodedKey := base64.StdEncoding.EncodeToString([]byte("mock-only-secret"))
	for _, tc := range []struct {
		name, source, backlogSource, key, visibleSecret string
	}{
		{"raw key", "mock-only-**secret**", "mock-only-''secret''", "mock-only-secret", "mock-only-secret"},
		{"URL encoded key", "mock%2Fkey%2B**secret**", "mock%2Fkey%2B''secret''", "mock/key+secret", "mock%2Fkey%2Bsecret"},
		{"base64 key", "bW9jay1vbmx5**LXNlY3JldA==**", "bW9jay1vbmx5''LXNlY3JldA==''", "mock-only-secret", encodedKey},
		{"ordinary styles", "ordinary **styled** content", "ordinary ''styled'' content", "mock-only-secret", ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			for _, format := range []struct {
				name, source string
				render       func(string, string, bool, int) (string, error)
			}{
				{"markdown", tc.source, markup.RenderWidth},
				{"backlog", tc.backlogSource, markup.RenderBacklogWidth},
			} {
				for _, prefix := range []string{"", "xxxxxxxx "} {
					for _, width := range []int{20, 36, 80} {
						for _, tty := range []bool{false, true} {
							t.Run(fmt.Sprintf("%s/prefix=%q/width=%d/tty=%v", format.name, prefix, width, tty), func(t *testing.T) {
								source := output.Redact(prefix+format.source, tc.key)
								rendered, err := format.render(source, "https://example.test", tty, width)
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
									unwrapped := strings.Join(strings.Fields(plain), "")
									if strings.Contains(unwrapped, tc.key) || strings.Contains(unwrapped, tc.visibleSecret) || !strings.Contains(plain, "[REDACTED]") {
										t.Errorf("TTY=%v: rendered secret not redacted: %q", tty, plain)
									}
								} else if !strings.Contains(stdout.String(), rendered) || tty && !strings.Contains(stdout.String(), "\x1b[") {
									t.Errorf("TTY=%v: ordinary styles changed: %q", tty, stdout.String())
								}
								if !tty && strings.ContainsRune(stdout.String(), '\x1b') {
									t.Errorf("ANSI on pipe: %q", stdout.String())
								}
							})
						}
					}
				}
			}
		})
	}
}

func TestWriteIssueDetailPreservesWhitespaceOutsideWrappedSecrets(t *testing.T) {
	const rendered = "  before\n\n\tcode &amp; &#27;\n  xxxxxxxx mock-  \n  only-\x1b[1msecret\x1b[0m\n\n  after\n  mock-only-\x1b[1msecret\x1b[0m\n"
	const want = "APP-12  Summary\nhttps://example.test/view/APP-12\n  before\n\n\tcode &amp; &#27;\n  xxxxxxxx [REDACTED]\n\n  after\n  [REDACTED]\n\n"
	for _, tty := range []bool{false, true} {
		var stdout bytes.Buffer
		if err := output.WriteIssueDetail(&stdout, json.RawMessage(`{"issueKey":"APP-12","summary":"Summary"}`), "https://example.test", rendered, tty, "mock-only-secret"); err != nil {
			t.Fatal(err)
		}
		if got := stdout.String(); got != want {
			t.Errorf("TTY=%v: got %q want %q", tty, got, want)
		}
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
