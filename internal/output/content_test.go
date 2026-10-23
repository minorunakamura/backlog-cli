package output_test

import (
	"backlog-cli/internal/output"
	"strings"
	"testing"
)

func TestSafeContentAllowsOnlyGeneratedSGR(t *testing.T) {
	input := "日本語👩‍💻\r\n\t\x1b[31m赤\x1b[0m\x1b]8;;https://evil\x1b\\link\x1b]8;;\x1b\\\x1b]52;c;stolen\a\x1bPpayload\x1b\\\x1b[2J\u009b?25l\x9dtitle\x9c\x00\x7f終\x1b["
	if got := output.SafeContent(input, true); got != "日本語👩‍💻\n\t\x1b[31m赤\x1b[0mlink終" {
		t.Fatalf("unsafe content: %q", got)
	}
	if got := output.SafeContent(input, false); got != "日本語👩‍💻\n\t赤link終" {
		t.Fatalf("nonTTY: %q", got)
	}
	for _, tc := range []struct{ input, want string }{
		{"前\x1b[31日本語👩‍💻", "前日本語👩‍💻"},
		{"前\x1b日本語", "前日本語"},
		{"前\x1bP作secret\x1b\\後", "前後"},
		{"前\x1b]52;c;作secret\a後", "前後"},
	} {
		if got := output.SafeContent(tc.input, true); got != tc.want {
			t.Errorf("SafeContent(%q)=%q want=%q", tc.input, got, tc.want)
		}
	}
	for _, prefix := range []string{"\x1b]52;", "\x1bP", "\u009d", "\x9b", "\x1b["} {
		if got := output.SafeContent("前"+prefix+strings.Repeat(";", 100000), true); got != "前" {
			t.Fatalf("unterminated: %q", got)
		}
	}
}
