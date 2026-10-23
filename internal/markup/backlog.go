package markup

import (
	"backlog-cli/internal/output"
	"net/url"
	"regexp"
	"strings"

	"github.com/yuin/goldmark/ast"
	"github.com/yuin/goldmark/parser"
	"github.com/yuin/goldmark/text"
	"golang.org/x/net/html"
)

var backlogNumericEntity = regexp.MustCompile("^" + numericEntity.String())

// Backlog prepares Backlog notation as Markdown, quoting Markdown-only syntax as text.
func Backlog(source, origin string) string {
	lines := strings.Split(backlogHTML(output.SafeText(source), false), "\n")
	var out strings.Builder
	quote := false
	toc := backlogContents(lines, origin)
	for index := 0; index < len(lines); index++ {
		line := lines[index]
		if lang, start, open, ok := backlogBlockStart(line); ok {
			out.WriteString(backlogInline(line[:start], origin, toc))
			var bodyLines []string
			remainder := line[open:]
			if remainder == "" {
				index++
				if index < len(lines) {
					remainder = lines[index]
				}
			}
			for index < len(lines) {
				if close := strings.Index(remainder, "{/code}"); close >= 0 {
					if close > 0 {
						bodyLines = append(bodyLines, remainder[:close])
					}
					if tail := remainder[close+7:]; tail != "" {
						lines[index] = tail
						index--
					}
					break
				}
				bodyLines = append(bodyLines, remainder)
				index++
				if index < len(lines) {
					remainder = lines[index]
				}
			}
			body := strings.Join(bodyLines, "\n")
			fence := strings.Repeat("~", max(3, longestRun(body, '~')+1))
			out.WriteString("\n\n" + fence + " " + lang + "\n" + body + "\n" + fence + "\n\n")
			continue
		}
		if line == "{quote}" {
			quote = true
			continue
		}
		if line == "{/quote}" {
			quote = false
			continue
		}
		if strings.HasPrefix(line, "|") {
			for index < len(lines) && strings.HasPrefix(lines[index], "|") {
				out.WriteString(backlogTableRow(lines[index], origin))
				out.WriteByte('\n')
				index++
			}
			out.WriteByte('\n')
			index--
			continue
		}
		prefix, body := backlogLine(line)
		if quote {
			out.WriteString("> ")
		}
		out.WriteString(prefix)
		if prefix == "" && len(body) > 0 && (body[0] == ' ' || body[0] == '\t') {
			if body[0] == ' ' {
				out.WriteString("&#32;")
			} else {
				out.WriteString("&#9;")
			}
			body = body[1:]
		}
		out.WriteString(backlogInline(body, origin, toc))
		out.WriteByte('\n')
	}
	return simplifyTables(strings.TrimSuffix(out.String(), "\n"))
}

func backlogLine(line string) (string, string) {
	if strings.HasPrefix(line, ">") {
		return "> ", strings.TrimPrefix(line[1:], " ")
	}
	for _, marker := range []string{"*", "-", "+"} {
		n := 0
		for n < len(line) && line[n:n+1] == marker {
			n++
		}
		if n == 0 {
			continue
		}
		body := strings.TrimPrefix(line[n:], " ")
		switch marker {
		case "*":
			if n <= 3 {
				return strings.Repeat("#", n) + " ", body
			}
		case "-", "+":
			bullet := "- "
			if marker == "+" {
				bullet = "1. "
			}
			prefix := strings.Repeat(" ", len(bullet)*(n-1)) + bullet
			if marker == "-" && (strings.HasPrefix(body, "[ ] ") || strings.HasPrefix(body, "[x] ")) {
				return prefix + body[:4], body[4:]
			}
			return prefix, body
		}
	}
	return "", line
}

func backlogInline(source, origin, toc string) string {
	var out strings.Builder
	ends := backlogDelimiters(source)
	hidden := make(map[int]bool)
	closing := make(map[int][2]string)
	macroClosing := make(map[int]bool)
	lastCodeClose := strings.LastIndex(source, "{/code}")
	for i := 0; i < len(source); {
		s := source[i:]
		if macroClosing[i] {
			out.WriteString("）")
			i++
			continue
		}
		if pair, ok := closing[i]; ok {
			out.WriteString(pair[1])
			i += len(pair[0])
			continue
		}
		if strings.HasPrefix(s, "[toc]") || strings.HasPrefix(s, "#contents") && (len(s) == 9 || s[9] == ' ') {
			out.WriteString(toc)
			if s[0] == '[' {
				i += 5
			} else {
				i += 9
			}
			continue
		}
		if start, end := backlogCodeSpan(source, i, ends, lastCodeClose); end >= 0 {
			body := source[start:end]
			fence := strings.Repeat("`", max(1, longestRun(body, '`')+1))
			out.WriteString(fence + " " + strings.ReplaceAll(body, "&", "&amp;") + " " + fence)
			i = end + 7
			continue
		}
		if hidden[i] {
			i++
			continue
		}
		if s[0] == '\\' && len(s) > 1 {
			end := backlogEscapeEnd(source, i, ends)
			out.WriteString(literal(s[1:end]))
			i += end
			continue
		}
		if s[0] == '&' {
			if match := backlogNumericEntity.FindStringIndex(s); match != nil && match[0] == 0 {
				out.WriteString(literal(safeEntities(s[:match[1]])))
				i += match[1]
				continue
			}
		}
		if strings.HasPrefix(s, "&br;") {
			out.WriteByte('\n')
			i += 4
			continue
		}
		if strings.HasPrefix(s, "&color(") && ends[i+6] >= 0 {
			end := ends[i+6] + 1
			for end < len(source) && source[end] == ' ' {
				end++
			}
			if end < len(source) && source[end] == '{' && ends[end] >= 0 {
				close := ends[end]
				hidden[close] = true
				for j := close - 1; j > end && source[j] == ' '; j-- {
					hidden[j] = true
				}
				i = end + 1
				for i < close && source[i] == ' ' {
					i++
				}
				continue
			}
		}
		if strings.HasPrefix(s, "[[") && ends[i] > i+2 && source[ends[i]-1] == ']' {
			end := ends[i]
			out.WriteString(backlogLink(source[i+2:end-1], origin))
			i = end + 1
			continue
		}
		if s[0] == '#' {
			if match := macroStart.FindStringSubmatch(s); match != nil {
				open := i + len(match[0]) - 1
				if end := ends[open]; end >= 0 {
					switch match[1] {
					case "rev", "attach", "image", "thumbnail", "color":
						out.WriteString(backlogMacro(match[1], safeEntities(source[open+1:end])))
						i = end + 1
					default:
						out.WriteString("未対応の記法（名前: " + literal(match[1]) + "、内容: ")
						macroClosing[end] = true
						i = open + 1
					}
					continue
				}
			}
		}
		if strings.HasPrefix(s, "<!--") {
			if end := strings.Index(s[4:], "-->"); end >= 0 {
				out.WriteString(literal(safeEntities(s[4 : 4+end])))
				i += end + 7
				continue
			}
		}
		if s[0] == '<' {
			if tag := htmlTag.FindString(s); tag != "" {
				mask := make([]bool, len(tag))
				out.WriteString(convert(tag, origin, mask, mask, "", parser.NewContext()))
				i += len(tag)
				continue
			}
		}
		if s[0] >= 'A' && s[0] <= 'Z' && (i == 0 || !wordOrURL(source[i-1])) {
			if key := issueKey.FindString(s); key != "" && (len(s) == len(key) || !wordOrURL(s[len(key)]) || s[len(key)] == '.' && (len(s) == len(key)+1 || s[len(key)+1] == ' ')) {
				out.WriteString("[" + key + "](" + origin + "/view/" + key + ")")
				i += len(key)
				continue
			}
		}
		matched := false
		for _, pair := range [][2]string{{"'''", "*"}, {"''", "**"}, {"%%", "~~"}} {
			if strings.HasPrefix(s, pair[0]) {
				if end := backlogEmphasisEnd(source, i+len(pair[0]), pair[0], ends, lastCodeClose); end >= 0 {
					closing[end] = pair
					out.WriteString(pair[1])
					i += len(pair[0])
					matched = true
					break
				}
			}
		}
		if matched {
			continue
		}
		end := i + 1
		// literal works on runes; keep a complete UTF-8 sequence in each chunk.
		for end < len(source) && source[end]&0xc0 == 0x80 {
			end++
		}
		if s[0] == '~' || s[0] == '=' {
			if s[0] == '~' {
				out.WriteString("&#126;")
			} else {
				out.WriteString("&#61;")
			}
			i++
			continue
		}
		if s[0] == '.' {
			out.WriteByte('\\')
		}
		out.WriteString(literal(source[i:end]))
		i = end
	}
	return out.String()
}

func backlogEmphasisEnd(source string, start int, delimiter string, ends []int, lastCodeClose int) int {
	for i := start; i < len(source); i++ {
		if source[i] == '\\' && i+1 < len(source) {
			i += backlogEscapeEnd(source, i, ends) - 1
			continue
		}
		if _, end := backlogCodeSpan(source, i, ends, lastCodeClose); end >= 0 {
			i = end + 6
			continue
		}
		if strings.HasPrefix(source[i:], delimiter) {
			return i
		}
	}
	return -1
}

// Normalize tags before physical-line scanning, without touching code or escapes.
func backlogHTML(source string, reference bool) string {
	ends := backlogDelimiters(source)
	lastClose := strings.LastIndex(source, "{/code}")
	reader := text.NewReader([]byte(source))
	autolink := parser.NewAutoLinkParser()
	parent := ast.NewParagraph()
	context := parser.NewContext()
	var out strings.Builder
	for i := 0; i < len(source); {
		end := i + 1
		if source[i] == '\\' && end < len(source) {
			end = i + backlogEscapeEnd(source, i, ends)
		} else if _, close := backlogCodeSpan(source, i, ends, lastClose); close >= 0 {
			end = min(len(source), close+7)
		} else if source[i] == '<' {
			// Reuse Goldmark's discriminator and one reader, as convert does.
			_, position := reader.Position()
			reader.Advance(i - position.Start)
			if autolink.Parse(parent, reader, context) != nil {
				_, position = reader.Position()
				out.WriteString(source[i:position.Start])
				i = position.Start
				continue
			}
			if tag := htmlTag.FindString(source[i:]); tag != "" {
				tokenizer := html.NewTokenizer(strings.NewReader(tag))
				kind := tokenizer.Next()
				if kind == html.StartTagToken || kind == html.EndTagToken || kind == html.SelfClosingTagToken {
					if !reference {
						switch tokenizer.Token().Data {
						case "details", "summary", "p", "div", "br", "img":
							out.WriteString(strings.ReplaceAll(tag, "\n", " "))
						}
					}
					i += len(tag)
					continue
				}
			}
		}
		out.WriteString(source[i:end])
		i = end
	}
	return out.String()
}

func backlogContents(lines []string, origin string) string {
	var toc strings.Builder
	toc.WriteString("\n\n目次\n\n")
	code := false
	for _, line := range lines {
		if code {
			if strings.Contains(line, "{/code}") {
				code = false
			}
			continue
		}
		if _, _, _, ok := backlogBlockStart(line); ok {
			code = true
			continue
		}
		prefix, body := backlogLine(line)
		if strings.HasPrefix(prefix, "#") {
			toc.WriteString("- " + backlogInline(body, origin, "") + "\n")
		}
	}
	return toc.String() + "\n"
}

func backlogTableRow(line, origin string) string {
	cells := backlogTableCells(line)
	header := len(cells) > 1 && cells[len(cells)-1] == "h" && strings.HasSuffix(line, "|h")
	caption := len(cells) > 1 && cells[len(cells)-1] == "c" && strings.HasSuffix(line, "|c")
	if header || caption {
		cells = cells[:len(cells)-1]
	}
	var out strings.Builder
	if !caption {
		out.WriteByte('|')
	}
	for _, cell := range cells {
		cellHeader := strings.HasPrefix(cell, "~")
		if cellHeader {
			cell = cell[1:]
		}
		body := backlogInline(cell, origin, "")
		body = strings.ReplaceAll(strings.ReplaceAll(body, "\n\n", " "), "\n", " ")
		if header || cellHeader {
			body = "**" + body + "**"
		}
		out.WriteString(body)
		if !caption {
			out.WriteByte('|')
		} else {
			out.WriteByte(' ')
		}
	}
	return out.String()
}

func backlogTableCells(line string) []string {
	var cells []string
	start := 1
	ends := backlogDelimiters(line)
	lastCodeClose := strings.LastIndex(line, "{/code}")
	for i := 1; i < len(line); i++ {
		if line[i] == '\\' && i+1 < len(line) {
			i++
			continue
		}
		if _, end := backlogCodeSpan(line, i, ends, lastCodeClose); end >= 0 {
			i = end + 6
			continue
		}
		if line[i] == '|' {
			cells = append(cells, line[start:i])
			start = i + 1
		}
	}
	if start < len(line) {
		cells = append(cells, line[start:])
	}
	return cells
}

func backlogCodeSpan(source string, start int, ends []int, lastClose int) (int, int) {
	if strings.HasPrefix(source[start:], "{code") && ends[start] >= 0 {
		open := ends[start]
		if _, ok := backlogCode(source[start : open+1]); ok {
			if open < lastClose {
				if close := strings.Index(source[open+1:], "{/code}"); close >= 0 {
					return open + 1, open + 1 + close
				}
			}
			return open + 1, len(source)
		}
	}
	return 0, -1
}

func backlogBlockStart(line string) (string, int, int, bool) {
	ends := backlogDelimiters(line)
	lastClose := strings.LastIndex(line, "{/code}")
	for i := 0; i < len(line); i++ {
		if line[i] == '\\' && i+1 < len(line) {
			i += backlogEscapeEnd(line, i, ends) - 1
			continue
		}
		if open, close := backlogCodeSpan(line, i, ends, lastClose); close >= 0 {
			if close == len(line) {
				lang, _ := backlogCode(line[i:open])
				return lang, i, open, true
			}
			i = close + 6
		}
	}
	return "", 0, 0, false
}

func backlogCode(line string) (string, bool) {
	if line == "{code}" {
		return "", true
	}
	if strings.HasPrefix(line, "{code:") && strings.HasSuffix(line, "}") {
		lang := line[6 : len(line)-1]
		if !strings.ContainsAny(lang, "{}") {
			return lang, true
		}
	}
	return "", false
}

func longestRun(value string, char byte) int {
	longest, run := 0, 0
	for i := 0; i < len(value); i++ {
		if value[i] == char {
			run++
			longest = max(longest, run)
		} else {
			run = 0
		}
	}
	return longest
}

func backlogEscapeEnd(source string, start int, ends []int) int {
	s := source[start:]
	tail := s[1:]
	if (strings.HasPrefix(tail, "[[") || tail[0] == '{') && ends[start+1] >= 0 {
		return ends[start+1] - start + 1
	}
	for _, delimiter := range []string{"'''", "''", "%%"} {
		if strings.HasPrefix(tail, delimiter) {
			return 1 + len(delimiter)
		}
	}
	if tag := htmlTag.FindString(tail); tag != "" {
		return 1 + len(tag)
	}
	if match := macroStart.FindString(tail); match != "" {
		if end := ends[start+len(match)]; end >= 0 {
			return end - start + 1
		}
	}
	if tail[0] == '&' {
		if end := strings.IndexByte(tail, ';'); end >= 0 {
			return end + 2
		}
	}
	end := 2
	for end < len(s) && s[end]&0xc0 == 0x80 {
		end++
	}
	return end
}

// Pair delimiters once so deeply nested or unterminated macros do not rescan suffixes.
func backlogDelimiters(source string) []int {
	ends := make([]int, len(source))
	for i := range ends {
		ends[i] = -1
	}
	stacks := map[byte][]int{}
	for i := 0; i < len(source); i++ {
		if source[i] == '\\' && i+1 < len(source) && (source[i+1] == '(' || source[i+1] == ')') {
			i++
			continue
		}
		switch c := source[i]; c {
		case '(', '[', '{':
			stacks[c] = append(stacks[c], i)
		case ')', ']', '}':
			open := map[byte]byte{')': '(', ']': '[', '}': '{'}[c]
			stack := stacks[open]
			if len(stack) > 0 {
				ends[stack[len(stack)-1]] = i
				stacks[open] = stack[:len(stack)-1]
			}
		}
	}
	return ends
}

func knownURL(value string) bool {
	u, err := url.Parse(value)
	return err == nil && (u.Scheme == "https" || u.Scheme == "http") && u.Host != ""
}

func backlogLink(value, origin string) string {
	value = backlogHTML(value, true)
	if output.ValidIssueKey(value) {
		return "[" + value + "](" + origin + "/view/" + value + ")"
	}
	label, target := value, value
	if !knownURL(value) {
		if before, after, ok := strings.Cut(value, ">"); ok {
			label, target = before, after
		} else {
			for i := 0; i < len(value); i++ {
				if value[i] == ':' && knownURL(value[i+1:]) {
					label, target = value[:i], value[i+1:]
					break
				}
			}
		}
	}
	if knownURL(target) {
		target = strings.NewReplacer("<", "%3C", ">", "%3E", "\\", "%5C", "(", "%28", ")", "%29", " ", "%20").Replace(target)
		return "[" + literal(safeEntities(label)) + "](" + safeEntities(target) + ")"
	}
	return "Wiki: " + literal(safeEntities(value))
}

func backlogMacro(name, value string) string {
	value = backlogHTML(value, true)
	if name == "image" || name == "thumbnail" {
		target, alt, _ := strings.Cut(value, ",")
		if knownURL(target) {
			return "画像: " + literal(alt) + "（" + literal(target) + "）"
		}
	}
	return macro(name, value)
}

// RenderBacklog uses the same fixed renderer and output safety checks as Markdown.
func RenderBacklog(source, origin string, tty bool) (string, error) {
	return render(Backlog(source, origin), tty)
}
