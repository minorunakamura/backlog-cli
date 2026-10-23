package markup

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

// Numeric control references are made visible before Glamour can decode them.
// Match the final numeric suffix too, so nested ampersand encodings cannot revive controls.
var numericEntity = regexp.MustCompile(`(?i)&(?:amp;?|#(?:0*38|x0*26);?)*#(?:x[0-9a-f]+|[0-9]+);?`)

func safeEntities(value string) string {
	var out strings.Builder
	previous := 0
	for _, match := range numericEntity.FindAllStringIndex(value, -1) {
		out.WriteString(value[previous:match[0]])
		entity := value[match[0]:match[1]]
		slashes := 0
		for i := match[0] - 1; i >= 0 && value[i] == '\\'; i-- {
			slashes++
		}
		if slashes%2 == 1 {
			out.WriteString("&amp;" + entity[1:])
			previous = match[1]
			continue
		}
		digits := strings.TrimSuffix(entity[strings.LastIndexByte(entity, '#')+1:], ";")
		base := 10
		if digits[0] == 'x' || digits[0] == 'X' {
			base = 16
			digits = digits[1:]
		}
		code, err := strconv.ParseUint(digits, base, 32)
		if err == nil && (code < 32 && code != 9 && code != 10 || code >= 127 && code <= 159) {
			out.WriteString(fmt.Sprintf("U+%04X", code))
		} else {
			out.WriteString(entity)
		}
		previous = match[1]
	}
	out.WriteString(value[previous:])
	return out.String()
}

func safeEntityCopy(source string, protected, inline []bool) (string, []bool, []bool) {
	var out strings.Builder
	var codeMask, inlineMask []bool
	for i := 0; i < len(source); {
		end := i + 1
		for end < len(source) && protected[end] == protected[i] {
			end++
		}
		chunk := source[i:end]
		if !protected[i] {
			chunk = safeEntities(chunk)
		}
		out.WriteString(chunk)
		if protected[i] {
			codeMask = append(codeMask, protected[i:end]...)
			inlineMask = append(inlineMask, inline[i:end]...)
		} else {
			codeMask = append(codeMask, make([]bool, len(chunk))...)
			inlineMask = append(inlineMask, make([]bool, len(chunk))...)
		}
		i = end
	}
	return out.String(), codeMask, inlineMask
}
