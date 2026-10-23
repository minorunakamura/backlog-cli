package markup

import (
	"strings"

	"github.com/yuin/goldmark"
	"github.com/yuin/goldmark/ast"
	"github.com/yuin/goldmark/text"
)

// simplifyTables retains overflow/merged cells instead of letting GFM discard them.
func simplifyTables(source string) string {
	data := []byte(source)
	doc := goldmark.New().Parser().Parse(text.NewReader(data))
	codeLines := map[int]bool{}
	_ = ast.Walk(doc, func(n ast.Node, entering bool) (ast.WalkStatus, error) {
		if entering && (n.Kind() == ast.KindCodeBlock || n.Kind() == ast.KindFencedCodeBlock) {
			for j := 0; j < n.Lines().Len(); j++ {
				start := n.Lines().At(j).Start
				codeLines[strings.LastIndexByte(source[:start], '\n')+1] = true
			}
		}
		return ast.WalkContinue, nil
	})
	lines := strings.Split(source, "\n")
	var out strings.Builder
	offset := 0
	for i := 0; i < len(lines); {
		line := lines[i]
		if !strings.HasPrefix(strings.TrimSpace(line), "|") || codeLines[offset] {
			out.WriteString(line)
			out.WriteByte('\n')
			offset += len(line) + 1
			i++
			continue
		}
		var rows [][]string
		width := 0
		for i < len(lines) && strings.HasPrefix(strings.TrimSpace(lines[i]), "|") && !codeLines[offset] {
			cells := tableCells(strings.TrimSpace(lines[i]))
			if !delimiter(cells) {
				rows = append(rows, cells)
				width = max(width, len(cells))
			}
			offset += len(lines[i]) + 1
			i++
		}
		for j, cells := range rows {
			out.WriteByte('|')
			for k := 0; k < width; k++ {
				if k < len(cells) {
					out.WriteString(escapeCodePipes(cells[k]))
				}
				out.WriteByte('|')
			}
			out.WriteByte('\n')
			if j == 0 {
				out.WriteString("|" + strings.Repeat("---|", width) + "\n")
			}
		}
		out.WriteByte('\n')
	}
	return strings.TrimSuffix(out.String(), "\n")
}

func tableCells(line string) []string {
	var cells []string
	start := 1
	for i := 1; i < len(line); i++ {
		if line[i] == '\\' && i+1 < len(line) {
			i++
			continue
		}
		if line[i] == '`' {
			end := i
			for end < len(line) && line[end] == '`' {
				end++
			}
			if j := strings.Index(line[end:], line[i:end]); j >= 0 {
				i = end + j + (end - i) - 1
				continue
			}
			i = end - 1
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

func delimiter(cells []string) bool {
	if len(cells) == 0 {
		return false
	}
	for _, cell := range cells {
		cell = strings.Trim(strings.TrimSpace(cell), ":")
		if len(cell) < 3 || strings.Trim(cell, "-") != "" {
			return false
		}
	}
	return true
}

func escapeCodePipes(cell string) string {
	var out strings.Builder
	for i := 0; i < len(cell); {
		if cell[i] == '`' {
			end := i
			for end < len(cell) && cell[end] == '`' {
				end++
			}
			if j := strings.Index(cell[end:], cell[i:end]); j >= 0 {
				stop := end + j + (end - i)
				out.WriteString(strings.ReplaceAll(cell[i:stop], "|", `\|`))
				i = stop
				continue
			}
		}
		out.WriteByte(cell[i])
		i++
	}
	return out.String()
}
