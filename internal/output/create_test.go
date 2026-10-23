package output_test

import (
	"backlog-cli/internal/output"
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

func TestWriteCreatedIssuePreservesJSONAndSanitizesHumanOutput(t *testing.T) {
	raw := json.RawMessage(`{"issueKey":"APP-1","description":"日本語👩‍💻\u001b[31m\u0007 SECRET","future":{"number":12345678901234567890}}`)
	for _, jsonMode := range []bool{false, true} {
		var stdout bytes.Buffer
		if err := output.WriteCreatedIssue(&stdout, raw, "https://example.test", jsonMode, "SECRET"); err != nil {
			t.Fatal(err)
		}
		text := stdout.String()
		if strings.Contains(text, "SECRET") || !strings.Contains(text, "APP-1") || !strings.Contains(text, "https://example.test/view/APP-1") || !strings.HasSuffix(text, "\n") {
			t.Fatalf("mode=%v stdout=%q", jsonMode, text)
		}
		if jsonMode {
			if !json.Valid(stdout.Bytes()) || !strings.Contains(text, `日本語👩‍💻\u001b[31m\u0007`) || !strings.Contains(text, "12345678901234567890") {
				t.Fatalf("JSON lost original fields: %s", text)
			}
		} else if text != "Created APP-1\nhttps://example.test/view/APP-1\n" {
			t.Fatalf("human output=%q", text)
		}
	}
}

func TestWriteFailureCreationIsRedactedAndWarnsAgainstRecreation(t *testing.T) {
	for _, state := range []string{"created", "unknown"} {
		for _, jsonMode := range []bool{false, true} {
			issueKey, issueURL := "SECRET-1", "https://SECRET.example/view/SECRET-1?key=SECRET"
			failure := output.NewFailure("response_error", "bad\x1b[31m SECRET", 201)
			failure.Creation = &output.Creation{State: state, IssueKey: &issueKey, URL: &issueURL}
			var stderr bytes.Buffer
			if err := output.WriteFailure(&stderr, failure, jsonMode, "SECRET"); err != nil {
				t.Fatal(err)
			}
			text := stderr.String()
			if strings.Contains(text, "SECRET") || !strings.Contains(text, "do not recreat") && !strings.Contains(text, "do not create it again") || !strings.Contains(text, state) {
				t.Fatalf("mode=%v state=%s stderr=%s", jsonMode, state, text)
			}
			if !jsonMode && strings.Contains(text, "\x1b") {
				t.Fatalf("unsafe terminal error %q", text)
			}
			if jsonMode && !json.Valid(stderr.Bytes()) {
				t.Fatalf("invalid JSON: %s", text)
			}
			if *failure.Creation.IssueKey != "SECRET-1" {
				t.Error("redaction mutated caller's creation")
			}
		}
	}
}
