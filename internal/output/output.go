package output

import (
	"strings"
	"unicode/utf8"

	"github.com/charmbracelet/x/ansi"
)

// SafeText removes external terminal commands and controls, retaining LF, TAB and Unicode text.
func SafeText(value string) string {
	value = strings.ReplaceAll(value, "\r\n", "\n")
	var normalized strings.Builder
	// ANSI parsers recognize byte C1 controls, not their UTF-8 encodings.
	for len(value) > 0 {
		r, n := utf8.DecodeRuneInString(value)
		if r >= 0x80 && r <= 0x9f {
			normalized.WriteByte(byte(r))
		} else {
			normalized.WriteString(value[:n])
		}
		value = value[n:]
	}
	return strings.Map(func(r rune) rune {
		if r == '\n' || r == '\t' {
			return r
		}
		if r < 0x20 || r >= 0x7f && r <= 0x9f {
			return -1
		}
		return r
	}, strings.ToValidUTF8(ansi.Strip(normalized.String()), ""))
}
