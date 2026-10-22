package output

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"html"
	"net/url"
	"strings"
)

// Redact removes the key and common transport encodings without exposing raw diagnostics.
func Redact(value, key string) string {
	if key == "" {
		return value
	}
	escaped, _ := json.Marshal(key)
	for _, secret := range []string{key, url.QueryEscape(key), url.PathEscape(key), html.EscapeString(key), string(escaped[1 : len(escaped)-1]), base64.StdEncoding.EncodeToString([]byte(key)), base64.RawStdEncoding.EncodeToString([]byte(key)), base64.URLEncoding.EncodeToString([]byte(key))} {
		value = strings.ReplaceAll(value, secret, "[REDACTED]")
		if strings.Contains(secret, "%") {
			value = strings.ReplaceAll(value, lowerEscapes(secret), "[REDACTED]")
		}
	}
	return value
}

func lowerEscapes(value string) string {
	b := []byte(value)
	for i := 0; i+2 < len(b); i++ {
		if b[i] == '%' {
			copy(b[i+1:i+3], strings.ToLower(string(b[i+1:i+3])))
			i += 2
		}
	}
	return string(b)
}

// RedactJSON preserves JSON values and numbers, changing only occurrences of secrets.
func RedactJSON(data []byte, key string) ([]byte, error) {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	var value any
	if err := decoder.Decode(&value); err != nil {
		return nil, err
	}
	changed := false
	value = redactValue(value, key, &changed)
	if !changed {
		return data, nil
	}
	return json.Marshal(value)
}

func redactValue(value any, key string, changed *bool) any {
	switch v := value.(type) {
	case string:
		redacted := Redact(v, key)
		if redacted != v {
			*changed = true
		}
		return redacted
	case []any:
		for i, child := range v {
			v[i] = redactValue(child, key, changed)
		}
	case map[string]any:
		result := make(map[string]any, len(v))
		for name, child := range v {
			safeName := Redact(name, key)
			if safeName != name {
				*changed = true
			}
			result[safeName] = redactValue(child, key, changed)
		}
		return result
	}
	return value
}
