package output

import (
	"encoding/json"
	"fmt"
	"io"
)

// WriteIssue preserves the fetched API object and adds its validated issue URL.
func WriteIssue(w io.Writer, item json.RawMessage, origin, key string) error {
	return writeIssue(w, item, origin, true, key, 200)
}

// WriteIssueDetail writes one fixed detail; rendered content may contain trusted SGR only.
func WriteIssueDetail(w io.Writer, item json.RawMessage, origin, rendered string, tty bool, key string) error {
	var fields struct {
		IssueKey string `json:"issueKey"`
		Summary  string `json:"summary"`
	}
	if json.Unmarshal(item, &fields) != nil || !ValidIssueKey(fields.IssueKey) {
		return NewFailure("response_error", "invalid API issue detail", 200)
	}
	header := Redact(SafeText(Redact(fmt.Sprintf("%s  %s\n%s/view/%s\n", fields.IssueKey, fields.Summary, origin, fields.IssueKey), key)), key)
	content := Redact(SafeContent(Redact(rendered, key), tty), key)
	plain := SafeText(content)
	if redacted := Redact(plain, key); redacted != plain {
		// SGR can split a secret; discard styling only when plain-text redaction finds it.
		content = redacted
	}
	if err := write(w, []byte(header+content+"\n")); err != nil {
		return NewFailure("output_error", "could not write output", 200)
	}
	return nil
}
