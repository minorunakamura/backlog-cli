// Package markup converts display copies only; API/JSON strings never pass through it.
package markup

import (
	"backlog-cli/internal/output"
	"strings"

	"github.com/charmbracelet/glamour"
	"github.com/yuin/goldmark"
	"github.com/yuin/goldmark/ast"
	"github.com/yuin/goldmark/extension"
	"github.com/yuin/goldmark/parser"
	"github.com/yuin/goldmark/text"
)

// Markdown prepares Backlog's Markdown for Glamour without changing code or escaped syntax.
func Markdown(source, origin string) string {
	source = output.SafeText(source)
	data := []byte(source)
	context := parser.NewContext()
	markdown := goldmark.New(goldmark.WithExtensions(extension.GFM))
	document := markdown.Parser().Parse(text.NewReader(data), parser.WithContext(context))
	// Expanded HTML must not hide code from the ordinary Markdown AST; byte offsets stay unchanged.
	analysis := []byte(strings.ReplaceAll(source, "<", "x"))
	codeDocument := markdown.Parser().Parse(text.NewReader(analysis))
	protected := make([]bool, len(source))
	inline := make([]bool, len(source))
	_ = ast.Walk(codeDocument, func(n ast.Node, entering bool) (ast.WalkStatus, error) {
		if !entering {
			return ast.WalkContinue, nil
		}
		switch n.Kind() {
		case ast.KindCodeBlock, ast.KindFencedCodeBlock:
			for j := 0; j < n.Lines().Len(); j++ {
				segment := n.Lines().At(j)
				for i := segment.Start; i < segment.Stop; i++ {
					protected[i] = true
				}
			}
			if block, ok := n.(*ast.FencedCodeBlock); ok && block.Info != nil {
				for i := block.Info.Segment.Start; i < block.Info.Segment.Stop; i++ {
					protected[i] = true
				}
			}
		case ast.KindCodeSpan:
			for child := n.FirstChild(); child != nil; child = child.NextSibling() {
				if t, ok := child.(*ast.Text); ok {
					for i := t.Segment.Start; i < t.Segment.Stop; i++ {
						protected[i] = true
						inline[i] = true
					}
				}
			}
		}
		return ast.WalkContinue, nil
	})
	var toc strings.Builder
	_ = ast.Walk(document, func(n ast.Node, entering bool) (ast.WalkStatus, error) {
		if entering && n.Kind() == ast.KindHeading {
			toc.WriteString("- ")
			toc.WriteString(literal(safeEntities(nodeText(n, data))))
			toc.WriteByte('\n')
		}
		return ast.WalkContinue, nil
	})
	source, protected, inline = safeEntityCopy(source, protected, inline)
	return simplifyTables(convert(source, origin, protected, inline, "\n\n目次\n\n"+toc.String()+"\n", context))
}

// Render uses a fixed built-in style (no terminal queries, environment styles, or resource fetches).
func Render(source, origin string, tty bool) (string, error) {
	return RenderWidth(source, origin, tty, 80)
}

// RenderWidth uses the same safe display pipeline at the viewer's current width.
func RenderWidth(source, origin string, tty bool, width int) (string, error) {
	return render(Markdown(source, origin), tty, width)
}

func render(markdown string, tty bool, width int) (string, error) {
	renderer, err := glamour.NewTermRenderer(glamour.WithStandardStyle("dark"), glamour.WithWordWrap(max(20, width)), glamour.WithPreservedNewLines())
	if err != nil {
		return "", err
	}
	rendered, err := renderer.Render(markdown)
	if err != nil {
		return "", err
	}
	return output.SafeContent(rendered, tty), nil
}
