package ui_test

import (
	"backlog-cli/internal/markup"
	"backlog-cli/internal/output"
	"backlog-cli/internal/ui"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func TestIssueDetailShortBodyWaitsForQWithoutQuittingParent(t *testing.T) {
	m, err := ui.NewIssueDetail(func(int) (string, error) { return "短い本文👩‍💻", nil })
	if err != nil {
		t.Fatal(err)
	}
	if closed, err := m.Result(); closed || err != nil || !strings.Contains(m.View(), "短い本文👩‍💻") {
		t.Fatalf("initial closed=%v err=%v view=%q", closed, err, m.View())
	}
	if cmd := m.Init(); cmd != nil {
		t.Fatal("short detail must wait for input")
	}
	_, cmd := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("q")})
	if closed, err := m.Result(); !closed || err != nil {
		t.Fatalf("q: closed=%v err=%v", closed, err)
	}
	if cmd != nil {
		if _, quit := cmd().(tea.QuitMsg); quit {
			t.Fatal("detail must not quit its parent")
		}
	}
}

func TestIssueDetailNavigation(t *testing.T) {
	var lines []string
	for i := range 50 {
		lines = append(lines, fmt.Sprintf("行%02d", i))
	}
	for _, tc := range []struct {
		name  string
		keys  []tea.KeyMsg
		first string
	}{
		{"down", []tea.KeyMsg{{Type: tea.KeyDown}}, "行01"},
		{"j and k", []tea.KeyMsg{runeKey('j'), runeKey('j'), runeKey('k')}, "行01"},
		{"up", []tea.KeyMsg{{Type: tea.KeyDown}, {Type: tea.KeyUp}}, "行00"},
		{"page down", []tea.KeyMsg{{Type: tea.KeyPgDown}}, "行23"},
		{"space", []tea.KeyMsg{{Type: tea.KeySpace}}, "行23"},
		{"page up", []tea.KeyMsg{{Type: tea.KeyPgDown}, {Type: tea.KeyPgUp}}, "行00"},
		{"b", []tea.KeyMsg{{Type: tea.KeySpace}, runeKey('b')}, "行00"},
		{"G", []tea.KeyMsg{runeKey('G')}, "行27"},
		{"End", []tea.KeyMsg{{Type: tea.KeyEnd}}, "行27"},
		{"g", []tea.KeyMsg{runeKey('G'), runeKey('g')}, "行00"},
		{"Home", []tea.KeyMsg{{Type: tea.KeyEnd}, {Type: tea.KeyHome}}, "行00"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			m, err := ui.NewIssueDetail(func(int) (string, error) { return strings.Join(lines, "\n"), nil })
			if err != nil {
				t.Fatal(err)
			}
			for _, key := range tc.keys {
				m.Update(key)
			}
			first, _, _ := strings.Cut(m.View(), "\n")
			if !strings.HasPrefix(first, tc.first) {
				t.Fatalf("first=%q want=%q", first, tc.first)
			}
		})
	}
}

func runeKey(r rune) tea.KeyMsg { return tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}} }

func TestIssueDetailResizeReflowsAndClamps(t *testing.T) {
	m, err := ui.NewIssueDetail(func(width int) (string, error) {
		if width >= 100 {
			return "広い本文\n末尾", nil
		}
		var lines []string
		for i := range 50 {
			lines = append(lines, fmt.Sprintf("行%02d", i))
		}
		return strings.Join(lines, "\n"), nil
	})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(runeKey('G'))
	m.Update(tea.WindowSizeMsg{Width: 80, Height: 49})
	if !strings.HasPrefix(m.View(), "行02") {
		t.Fatalf("height clamp: %q", m.View())
	}
	m.Update(tea.WindowSizeMsg{Width: 100, Height: 24})
	if !strings.HasPrefix(m.View(), "広い本文") || !strings.Contains(m.View(), "末尾") {
		t.Fatalf("reflow clamp: %q", m.View())
	}
	for _, size := range []tea.WindowSizeMsg{{Width: 1, Height: 1}, {Width: 0, Height: 0}, {Width: -10, Height: -4}} {
		m.Update(size)
		if !strings.Contains(m.View(), "Resize terminal") {
			t.Fatalf("tiny: %q", m.View())
		}
	}
	m.Update(runeKey('q'))
	if closed, err := m.Result(); !closed || err != nil {
		t.Fatalf("tiny q: %v %v", closed, err)
	}
}

func TestIssueDetailResizeKeepsStyledSecretsRedacted(t *testing.T) {
	for _, format := range []struct {
		name, source string
		render       func(string, string, bool, int) (string, error)
	}{
		{"markdown", "xxxxxxxx mock-only-**secret**", markup.RenderWidth},
		{"backlog", "xxxxxxxx mock-only-''secret''", markup.RenderBacklogWidth},
	} {
		t.Run(format.name, func(t *testing.T) {
			m, err := ui.NewIssueDetail(func(width int) (string, error) {
				rendered, err := format.render(output.Redact(format.source, "mock-only-secret"), "https://example.test", true, width)
				if err != nil {
					return "", err
				}
				var content bytes.Buffer
				err = output.WriteIssueDetail(&content, json.RawMessage(`{"issueKey":"APP-12","summary":"Summary"}`), "https://example.test", rendered, true, "mock-only-secret")
				return ansi.Wrap(content.String(), width, ""), err
			})
			if err != nil {
				t.Fatal(err)
			}
			for _, width := range []int{80, 20, 36, 80, 20, 80} {
				m.Update(tea.WindowSizeMsg{Width: width, Height: 24})
				plain := output.SafeText(m.View())
				if strings.Contains(plain, "mock-") || strings.Contains(plain, "only-secret") || !strings.Contains(plain, "[REDACTED]") {
					t.Fatalf("width=%d: secret not redacted: %q", width, plain)
				}
				if closed, err := m.Result(); closed || err != nil {
					t.Fatalf("width=%d: closed=%v err=%v", width, closed, err)
				}
			}
		})
	}
}

func TestIssueDetailReportsCancellationAndRenderErrorsToParent(t *testing.T) {
	t.Run("ctrl-c", func(t *testing.T) {
		m, err := ui.NewIssueDetail(func(int) (string, error) { return "本文", nil })
		if err != nil {
			t.Fatal(err)
		}
		m.Update(tea.KeyMsg{Type: tea.KeyCtrlC})
		closed, err := m.Result()
		var failure *output.Failure
		if !closed || !errors.As(err, &failure) || failure.Code != "cancelled" {
			t.Fatalf("closed=%v err=%v", closed, err)
		}
	})
	t.Run("initial render", func(t *testing.T) {
		expected := errors.New("render failed")
		_, err := ui.NewIssueDetail(func(int) (string, error) { return "", expected })
		if !errors.Is(err, expected) {
			t.Fatalf("err=%v", err)
		}
	})
	t.Run("resize render", func(t *testing.T) {
		expected := errors.New("resize failed")
		m, err := ui.NewIssueDetail(func(width int) (string, error) {
			if width == 80 {
				return "本文", nil
			}
			return "", expected
		})
		if err != nil {
			t.Fatal(err)
		}
		m.Update(tea.WindowSizeMsg{Width: 40, Height: 10})
		closed, err := m.Result()
		if !closed || !errors.Is(err, expected) {
			t.Fatalf("closed=%v err=%v", closed, err)
		}
	})
}
