package output_test

import (
	"backlog-cli/internal/output"
	"bytes"
	"encoding/json"
	"testing"
)

func TestSafeTextRemovesTerminalControlsWithoutBreakingUnicode(t *testing.T) {
	input := "日本語👩‍💻\r\n\t\x1b[31m赤\x1b[0m\x1b]52;c;secret\a\x1bPpayload\x1b\\\u009b2J\u009dtitle\u009c\x00\x7f\u0085\b\r終"
	if got := output.SafeText(input); got != "日本語👩‍💻\n\t赤終" {
		t.Fatalf("got %q", got)
	}
}

func TestWriteListRedactsSecretsButPreservesJSONValues(t *testing.T) {
	key := "secret+/?日本"
	item := json.RawMessage(`{"id":12345678901234567890,"name":"日本語\u0007\u001b[31m👩‍💻","secret":"secret+/?日本","nested":["secret%2B%2F%3F%E6%97%A5%E6%9C%AC","secret%2b%2f%3f%e6%97%a5%e6%9c%ac","secret+\/?日本"]}`)
	var stdout bytes.Buffer
	err := output.WriteList(&stdout, []json.RawMessage{item}, "project", true, key)
	if err != nil {
		t.Fatal(err)
	}
	decoder := json.NewDecoder(&stdout)
	decoder.UseNumber()
	var result struct {
		Items []map[string]any `json:"items"`
	}
	if err := decoder.Decode(&result); err != nil {
		t.Fatal(err)
	}
	got := result.Items[0]
	if got["id"] != json.Number("12345678901234567890") || got["name"] != "日本語\a\x1b[31m👩‍💻" || got["secret"] != "[REDACTED]" {
		t.Fatalf("lost JSON values: %#v", got)
	}
	for _, value := range got["nested"].([]any) {
		if value != "[REDACTED]" {
			t.Fatalf("secret not redacted: %q", value)
		}
	}
}

func TestWriteFailureSanitizesHumanExternalText(t *testing.T) {
	var stderr bytes.Buffer
	failure := output.NewFailure("api_error", "日本語\x1b]52;c;stolen\a\x00 secret\r\n\t👩‍💻", 403)
	if err := output.WriteFailure(&stderr, failure, false, "secret"); err != nil {
		t.Fatal(err)
	}
	if stderr.String() != "日本語 [REDACTED]\n\t👩‍💻\n" {
		t.Fatalf("unsafe error: %q", &stderr)
	}
}

func TestSafeTextHandlesRawC1AndUnterminatedCommands(t *testing.T) {
	for _, tc := range []struct{ input, want string }{
		{"日本\x9b31m赤\x9b0m", "日本赤"},
		{"前\x1b]52;c;payload", "前"},
		{"前\x1bPpayload", "前"},
		{"前\x1b[", "前"},
		{"\u009d52;c;payload\u009c日本語", "日本語"},
		{"\x1b[?25l日本\x1b[2J語", "日本語"},
	} {
		if got := output.SafeText(tc.input); got != tc.want {
			t.Errorf("SafeText(%q)=%q want=%q", tc.input, got, tc.want)
		}
	}
}
