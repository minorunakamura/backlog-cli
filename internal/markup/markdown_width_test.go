package markup_test

import (
	"backlog-cli/internal/markup"
	"backlog-cli/internal/output"
	"strings"
	"testing"

	"github.com/charmbracelet/x/ansi"
)

func TestRenderWidthReflowsBothFormatsWithoutWeakeningSafety(t *testing.T) {
	for _, render := range []func(string, string, bool, int) (string, error){markup.RenderWidth, markup.RenderBacklogWidth} {
		source := strings.Repeat("日本語👩‍💻 本文 ", 20) + "\n\n`&#27;` &#7; \x1b]52;c;stolen\a"
		narrow, err := render(source, "https://example.test", true, 30)
		if err != nil {
			t.Fatal(err)
		}
		wide, err := render(source, "https://example.test", true, 80)
		if err != nil {
			t.Fatal(err)
		}
		if strings.Count(narrow, "\n") <= strings.Count(wide, "\n") {
			t.Fatal("narrow rendering must reflow")
		}
		for _, content := range []string{narrow, wide} {
			if output.SafeContent(content, true) != content || strings.Contains(content, "stolen") || !strings.Contains(content, "日本語👩‍💻") {
				t.Fatalf("unsafe or damaged content %q", content)
			}
		}
		plain, err := render(source, "https://example.test", false, 30)
		if err != nil || strings.Contains(plain, "\x1b") {
			t.Fatalf("nonTTY: %q %v", plain, err)
		}
		for _, width := range []int{-1, 0, 1} {
			if _, err := render("短い本文", "https://example.test", true, width); err != nil {
				t.Fatal(err)
			}
		}
		for _, line := range strings.Split(ansi.Strip(narrow), "\n") {
			if ansi.StringWidth(line) > 30 {
				t.Fatalf("wide line: %q", line)
			}
		}
	}
}

func TestRenderDefaultWidthStaysCompatible(t *testing.T) {
	for _, pair := range []struct {
		fixed func(string, string, bool) (string, error)
		width func(string, string, bool, int) (string, error)
	}{
		{markup.Render, markup.RenderWidth}, {markup.RenderBacklog, markup.RenderBacklogWidth},
	} {
		old, err := pair.fixed("日本語 本文", "https://example.test", true)
		if err != nil {
			t.Fatal(err)
		}
		sized, err := pair.width("日本語 本文", "https://example.test", true, 80)
		if err != nil || old != sized {
			t.Fatalf("default changed: %v", err)
		}
	}
}
