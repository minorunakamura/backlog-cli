package output

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
)

// WriteList buffers one complete result before writing, preserving API objects in JSON mode.
func WriteList(w io.Writer, items []json.RawMessage, kind string, jsonMode bool, key string) error {
	var data []byte
	if items == nil {
		items = []json.RawMessage{}
	}
	if jsonMode {
		safe := make([]json.RawMessage, len(items))
		for i, item := range items {
			value, err := RedactJSON(item, key)
			if err != nil {
				return NewFailure("output_error", "could not encode output", 200)
			}
			safe[i] = value
		}
		encoded, err := json.Marshal(struct {
			Items []json.RawMessage `json:"items"`
		}{safe})
		if err != nil {
			return NewFailure("output_error", "could not encode output", 200)
		}
		data = append(encoded, '\n')
	} else {
		var buffer bytes.Buffer
		if kind == "project" {
			buffer.WriteString("ID\tKEY\tNAME\n")
		} else {
			buffer.WriteString("ID\tNAME\n")
		}
		for _, item := range items {
			var fields struct {
				ID         json.Number `json:"id"`
				ProjectKey string      `json:"projectKey"`
				Name       string      `json:"name"`
			}
			if json.Unmarshal(item, &fields) != nil {
				return NewFailure("response_error", "invalid API display fields", 200)
			}
			safe := func(value string) string { return Redact(SafeText(Redact(value, key)), key) }
			if kind == "project" {
				fmt.Fprintf(&buffer, "%s\t%s\t%s\n", safe(fields.ID.String()), safe(fields.ProjectKey), safe(fields.Name))
			} else {
				fmt.Fprintf(&buffer, "%s\t%s\n", safe(fields.ID.String()), safe(fields.Name))
			}
		}
		data = buffer.Bytes()
	}
	if err := write(w, data); err != nil {
		return NewFailure("output_error", "could not write output", 200)
	}
	return nil
}
