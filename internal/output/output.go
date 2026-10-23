package output

import (
	"strings"
	"unicode/utf8"
)

// SafeText removes external terminal commands and controls, retaining LF, TAB and Unicode text.
func SafeText(value string) string { return SafeContent(value, false) }

// SafeContent permits only SGR from a trusted renderer, never terminal commands or string sequences.
func SafeContent(value string, allowSGR bool) string {
	value = strings.ReplaceAll(value, "\r\n", "\n")
	var normalized strings.Builder
	// Normalize both raw and UTF-8 C1 without splitting Japanese UTF-8 continuation bytes.
	for len(value) > 0 {
		if value[0] >= 0x80 && value[0] <= 0x9f {
			normalized.WriteByte(value[0])
			value = value[1:]
			continue
		}
		r, n := utf8.DecodeRuneInString(value)
		if r >= 0x80 && r <= 0x9f {
			normalized.WriteByte(byte(r))
		} else if r != utf8.RuneError || n != 1 {
			normalized.WriteString(value[:n])
		}
		value = value[n:]
	}
	value = normalized.String()
	var out strings.Builder
	for i := 0; i < len(value); {
		if value[i] >= 0xc0 {
			_, n := utf8.DecodeRuneInString(value[i:])
			out.WriteString(value[i : i+n])
			i += n
			continue
		}
		start := i
		c := value[i]
		i++
		if c == 0x1b {
			if i == len(value) {
				break
			}
			c = value[i]
			i++
			switch c {
			case '[':
				c = 0x9b
			case ']':
				c = 0x9d
			case 'P':
				c = 0x90
			case 'X':
				c = 0x98
			case '^':
				c = 0x9e
			case '_':
				c = 0x9f
			default:
				for c >= 0x20 && c <= 0x2f && i < len(value) {
					c = value[i]
					i++
				}
				if c >= 0x80 || c < ' ' {
					i--
				}
				continue
			}
		}
		switch c {
		case 0x9b:
			params := i
			for i < len(value) && value[i] >= 0x20 && value[i] <= 0x3f {
				i++
			}
			if i == len(value) {
				break
			}
			final := value[i]
			if final < 0x40 || final > 0x7e {
				continue
			}
			i++
			sgr := allowSGR && value[start] == 0x1b && final == 'm'
			for j := params; j < i-1; j++ {
				if (value[j] < '0' || value[j] > '9') && value[j] != ';' && value[j] != ':' {
					sgr = false
				}
			}
			if sgr {
				out.WriteString(value[start:i])
			}
		case 0x90, 0x98, 0x9d, 0x9e, 0x9f:
			for i < len(value) {
				if value[i] >= 0xc0 {
					_, n := utf8.DecodeRuneInString(value[i:])
					i += n
					continue
				}
				b := value[i]
				i++
				if b == 0x9c || c == 0x9d && b == 7 {
					break
				}
				if b == 0x1b && i < len(value) && value[i] == '\\' {
					i++
					break
				}
			}
		default:
			if c == '\n' || c == '\t' || c >= 0x20 && (c < 0x7f || c > 0x9f) {
				out.WriteByte(c)
			}
		}
	}
	return out.String()
}
