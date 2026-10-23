package output

import (
	"encoding/json"
	"io"
)

// WriteIssue preserves the fetched API object and adds its validated issue URL.
func WriteIssue(w io.Writer, item json.RawMessage, origin, key string) error {
	return writeIssue(w, item, origin, true, key, 200)
}
