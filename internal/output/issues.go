package output

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"regexp"
)

// Pagination records the requested page and the total observed before fetching it.
type Pagination struct {
	Offset int64 `json:"offset"`
	Limit  int64 `json:"limit"`
	Count  int64 `json:"count"`
	Total  int64 `json:"total"`
}

// NextOffset ends empty pages and compares before adding to avoid int64 overflow.
func (p Pagination) NextOffset() (int64, bool) {
	if p.Count == 0 || p.Offset >= p.Total || p.Count >= p.Total-p.Offset {
		return 0, false
	}
	return p.Offset + p.Count, true
}

var issueKeyPattern = regexp.MustCompile(`^[A-Za-z0-9_]+-[0-9]+$`)

// ValidIssueKey reports whether a key is safe for an issue API path and URL.
func ValidIssueKey(value string) bool { return issueKeyPattern.MatchString(value) }

// WriteIssueList preserves API objects, adding URLs built only from validated issue keys.
func WriteIssueList(w io.Writer, items []json.RawMessage, pagination Pagination, origin string, jsonMode bool, key string) error {
	safe := make([]json.RawMessage, len(items))
	for i, item := range items {
		var fields map[string]json.RawMessage
		var issueKey string
		if json.Unmarshal(item, &fields) != nil || json.Unmarshal(fields["issueKey"], &issueKey) != nil || !issueKeyPattern.MatchString(issueKey) {
			return NewFailure("response_error", "invalid API issue key", 200)
		}
		fields["url"], _ = json.Marshal(origin + "/view/" + issueKey)
		value, err := json.Marshal(fields)
		if err != nil {
			return NewFailure("output_error", "could not encode output", 200)
		}
		safe[i], err = RedactJSON(value, key)
		if err != nil {
			return NewFailure("output_error", "could not encode output", 200)
		}
	}
	var data []byte
	if jsonMode {
		encoded, err := json.Marshal(struct {
			Items      []json.RawMessage `json:"items"`
			Pagination Pagination        `json:"pagination"`
		}{safe, pagination})
		if err != nil {
			return NewFailure("output_error", "could not encode output", 200)
		}
		data = append(encoded, '\n')
	} else {
		var buffer bytes.Buffer
		buffer.WriteString("KEY\tSUMMARY\tSTATUS\tASSIGNEE\n")
		for _, item := range items {
			var fields struct {
				IssueKey string `json:"issueKey"`
				Summary  string `json:"summary"`
				Status   struct {
					Name string `json:"name"`
				} `json:"status"`
				Assignee *struct {
					Name string `json:"name"`
				} `json:"assignee"`
			}
			if json.Unmarshal(item, &fields) != nil {
				return NewFailure("response_error", "invalid API issue display fields", 200)
			}
			assignee := ""
			if fields.Assignee != nil {
				assignee = fields.Assignee.Name
			}
			text := func(value string) string { return Redact(SafeText(Redact(value, key)), key) }
			fmt.Fprintf(&buffer, "%s\t%s\t%s\t%s\n", text(fields.IssueKey), text(fields.Summary), text(fields.Status.Name), text(assignee))
		}
		if next, more := pagination.NextOffset(); more {
			fmt.Fprintf(&buffer, "Next offset: %d\n", next)
		}
		data = buffer.Bytes()
	}
	if err := write(w, data); err != nil {
		return NewFailure("output_error", "could not write output", 200)
	}
	return nil
}
