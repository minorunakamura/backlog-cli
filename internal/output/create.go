package output

import (
	"encoding/json"
	"fmt"
	"io"
	"net/url"
	"strings"
)

// Creation is command-scoped certainty, updated as soon as response headers arrive.
type Creation struct {
	State    string  `json:"state"`
	IssueKey *string `json:"issueKey"`
	URL      *string `json:"url"`
}

// Identify uses only validated keys or a Location on the configured origin.
func (c *Creation) Identify(origin, location string, item json.RawMessage) {
	base, _ := url.Parse(origin)
	if location != "" && base != nil {
		reference, err := url.Parse(location)
		if err == nil {
			target := base.ResolveReference(reference)
			if target.Scheme == base.Scheme && strings.EqualFold(target.Host, base.Host) && target.User == nil {
				value := target.String()
				c.URL = &value
				candidate := strings.TrimPrefix(target.Path, "/view/")
				if strings.HasPrefix(target.Path, "/view/") && issueKeyPattern.MatchString(candidate) {
					c.IssueKey = &candidate
				}
			}
		}
	}
	var fields struct {
		IssueKey string `json:"issueKey"`
	}
	if json.Unmarshal(item, &fields) == nil && issueKeyPattern.MatchString(fields.IssueKey) {
		c.IssueKey = &fields.IssueKey
		value := origin + "/view/" + fields.IssueKey
		c.URL = &value
	}
}

// WriteCreatedIssue preserves unknown API fields and adds the validated issue URL.
func WriteCreatedIssue(w io.Writer, item json.RawMessage, origin string, jsonMode bool, key string) error {
	var fields map[string]json.RawMessage
	var issueKey string
	if json.Unmarshal(item, &fields) != nil || json.Unmarshal(fields["issueKey"], &issueKey) != nil || !issueKeyPattern.MatchString(issueKey) {
		return NewFailure("response_error", "invalid API issue JSON or key", 201)
	}
	issueURL := origin + "/view/" + issueKey
	fields["url"], _ = json.Marshal(issueURL)
	data, err := json.Marshal(fields)
	if err != nil {
		return NewFailure("output_error", "could not encode output", 201)
	}
	data, err = RedactJSON(data, key)
	if err != nil {
		return NewFailure("output_error", "could not encode output", 201)
	}
	if jsonMode {
		data = append(data, '\n')
	} else {
		data = []byte(Redact(SafeText(Redact(fmt.Sprintf("Created %s\n%s\n", issueKey, issueURL), key)), key))
	}
	if err := write(w, data); err != nil {
		return NewFailure("output_error", "could not write output", 201)
	}
	return nil
}
