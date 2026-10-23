package markup

import (
	"backlog-cli/internal/output"
	"fmt"
	"regexp"
	"strings"

	"github.com/yuin/goldmark/ast"
	"github.com/yuin/goldmark/parser"
	"github.com/yuin/goldmark/text"
	"golang.org/x/net/html"
)

var (
	issueKey   = regexp.MustCompile(`^[A-Z][A-Z0-9_]*-[0-9]+`)
	macroStart = regexp.MustCompile(`^#([A-Za-z][A-Za-z0-9_]*)(\()`)
	htmlTag    = regexp.MustCompile(`(?s)^</?[A-Za-z](?:[^<>"']|"[^"]*"|'[^']*')*>`)
)

func nodeText(n ast.Node, source []byte) string {
	var out strings.Builder
	_ = ast.Walk(n, func(child ast.Node, entering bool) (ast.WalkStatus, error) {
		if entering {
			switch t := child.(type) {
			case *ast.Text:
				out.Write(t.Segment.Value(source))
			case *ast.String:
				out.Write(t.Value)
			}
		}
		return ast.WalkContinue, nil
	})
	return out.String()
}

func literal(value string) string {
	var out strings.Builder
	for _, r := range value {
		if strings.ContainsRune(`\`+"`*_{}[]()#+-!|<>", r) {
			out.WriteByte('\\')
		}
		if r == '&' {
			out.WriteString("&amp;")
		} else {
			out.WriteRune(r)
		}
	}
	return out.String()
}

func convert(source, origin string, protected, inline []bool, toc string, context parser.Context) string {
	var out strings.Builder
	// Reuse one reader to avoid rescanning the remaining line at every '<'.
	reader := text.NewReader([]byte(source))
	autolink := parser.NewAutoLinkParser()
	parent := ast.NewParagraph()
	lastParen := strings.LastIndexByte(source, ')')
	for i := 0; i < len(source); {
		if protected[i] {
			if inline[i] && source[i] == '&' {
				out.WriteString("&amp;")
			} else {
				out.WriteByte(source[i])
			}
			i++
			continue
		}
		s := source[i:]
		if s[0] == '\\' && len(s) > 1 {
			// Keep escaped constructs as literal Markdown, including their inner issue keys.
			end := 2
			if s[1] == '<' {
				if tag := htmlTag.FindString(s[1:]); tag != "" {
					end = 1 + len(tag)
				}
			}
			if strings.HasPrefix(s[1:], "[[") {
				if j := strings.Index(s[3:], "]]"); j >= 0 {
					end = j + 5
				}
			}
			if match := macroStart.FindString(s[1:]); match != "" && i < lastParen {
				if j := strings.IndexByte(s[1+len(match):], ')'); j >= 0 {
					end = 2 + len(match) + j
				}
			}
			out.WriteString(s[:end])
			i += end
			continue
		}
		if strings.HasPrefix(s, "[toc]") || strings.HasPrefix(s, "#contents") && (len(s) == 9 || s[9] == '\n' || s[9] == '\r') {
			n := 5
			if s[0] == '#' {
				n = 9
			}
			out.WriteString(toc)
			i += n
			continue
		}
		if strings.HasPrefix(s, "[[") {
			if j := strings.Index(s[2:], "]]"); j >= 0 {
				value := s[2 : j+2]
				if output.ValidIssueKey(value) {
					out.WriteString("[" + value + "](" + origin + "/view/" + value + ")")
				} else {
					out.WriteString("Wiki: " + literal(value))
				}
				i += j + 4
				continue
			}
		}
		if s[0] == '#' && i < lastParen {
			if match := macroStart.FindStringSubmatch(s); match != nil {
				if j := strings.IndexByte(s[len(match[0]):], ')'); j >= 0 {
					end := len(match[0]) + j
					value := s[len(match[0]):end]
					out.WriteString(macro(match[1], value))
					i += end + 1
					continue
				}
			}
		}
		if strings.HasPrefix(s, "<!--") {
			if j := strings.Index(s[4:], "-->"); j >= 0 {
				out.WriteString(literal(s[4 : 4+j]))
				i += j + 7
				continue
			}
		}
		if s[0] == '<' {
			// Let Goldmark distinguish mail/URI autolinks from unknown HTML tags.
			_, position := reader.Position()
			reader.Advance(i - position.Start)
			if autolink.Parse(parent, reader, context) != nil {
				_, position = reader.Position()
				out.WriteString(source[i:position.Start])
				i = position.Start
				continue
			}
			if tag := htmlTag.FindString(s); tag != "" {
				tokenizer := html.NewTokenizer(strings.NewReader(tag))
				kind := tokenizer.Next()
				token := tokenizer.Token()
				if kind == html.StartTagToken || kind == html.EndTagToken || kind == html.SelfClosingTagToken {
					switch token.Data {
					case "details", "summary", "p", "div", "br":
						out.WriteString("\n\n")
					case "img":
						out.WriteString("画像: ")
						for _, attr := range token.Attr {
							if attr.Key == "alt" || attr.Key == "src" {
								out.WriteString(literal(output.SafeText(attr.Val)) + " ")
							}
						}
					}
					i += len(tag)
					continue
				}
			}
		}
		// Preserve standard Markdown links; images become visible text without a fetch.
		image := strings.HasPrefix(s, "![")
		if s[0] == '[' || image {
			start := 1
			if image {
				start = 2
			}
			if j := strings.IndexByte(s[start:], ']'); j >= 0 && start+j+1 < len(s) && s[start+j+1] == '(' {
				close := start + j
				end := close + 2
				depth := 1
				for end < len(s) && depth > 0 {
					if s[end] == '\\' && end+1 < len(s) {
						end += 2
						continue
					}
					if s[end] == '(' {
						depth++
					}
					if s[end] == ')' {
						depth--
					}
					end++
				}
				if depth == 0 {
					if image {
						out.WriteString("画像: " + literal(s[start:close]) + "（" + literal(s[close+2:end-1]) + "）")
					} else {
						for j := 0; j < end; j++ {
							if inline[i+j] && s[j] == '&' {
								out.WriteString("&amp;")
							} else {
								out.WriteByte(s[j])
							}
						}
					}
					i += end
					continue
				}
			}
		}
		if image {
			if j := strings.IndexByte(s[2:], ']'); j >= 0 {
				end := j + 3
				label := s[2 : end-1]
				reference := label
				if end < len(s) && s[end] == '[' {
					if close := strings.IndexByte(s[end+1:], ']'); close >= 0 {
						reference = s[end+1 : end+1+close]
						end += close + 2
						if reference == "" {
							reference = label
						}
					}
				}
				if ref, ok := context.Reference(reference); ok {
					out.WriteString("画像: " + literal(label) + "（" + literal(safeEntities(string(ref.Destination()))) + "）")
					i += end
					continue
				}
			}
		}
		// A bare key must be a whole word, not part of a URL, identifier, or path.
		if s[0] >= 'A' && s[0] <= 'Z' && (i == 0 || !wordOrURL(source[i-1])) {
			if key := issueKey.FindString(s); key != "" && (len(s) == len(key) || !wordOrURL(s[len(key)]) || s[len(key)] == '.' && (len(s) == len(key)+1 || s[len(key)+1] == ' ' || s[len(key)+1] == '\n')) {
				out.WriteString("[" + key + "](" + origin + "/view/" + key + ")")
				i += len(key)
				continue
			}
		}
		out.WriteByte(s[0])
		i++
	}
	return out.String()
}

func wordOrURL(b byte) bool {
	return b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' || b >= '0' && b <= '9' || strings.ContainsRune("_-/.:?=&%#", rune(b))
}

func macro(name, value string) string {
	switch name {
	case "rev":
		if repo, revision, ok := strings.Cut(value, ":"); ok {
			return "Gitリビジョン: " + literal(repo+"@"+revision)
		}
		return "Subversionリビジョン: r" + literal(value)
	case "attach":
		if file, id, ok := strings.Cut(value, ":"); ok {
			return "添付ファイル: " + literal(file) + "（ID: " + literal(id) + "）"
		}
		return "添付ファイル: " + literal(value)
	case "image", "thumbnail":
		return "画像: ID: " + literal(value)
	case "color":
		if _, body, ok := strings.Cut(value, ","); ok {
			return literal(body)
		}
	}
	return fmt.Sprintf("未対応の記法（名前: %s、内容: %s）", literal(name), literal(value))
}
