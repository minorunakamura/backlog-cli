package output_test

import (
	"backlog-cli/internal/output"
	"bytes"
	"encoding/json"
	"io"
	"strings"
	"testing"
)

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
