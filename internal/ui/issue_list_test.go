package ui_test

import (
	"backlog-cli/internal/output"
	"backlog-cli/internal/ui"
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

func issueRows(n int) []json.RawMessage {
	rows := make([]json.RawMessage, n)
	for i := range rows {
		rows[i] = json.RawMessage(fmt.Sprintf(`{"issueKey":"APP-%d","summary":"件名 日本語👩‍💻","status":{"name":"処理中"},"assignee":{"name":"担当者"}}`, i+1))
	}
	return rows
}

func listKey(m *ui.IssueList, key string) tea.Cmd {
	var msg tea.KeyMsg
	switch key {
	case "enter":
		msg.Type = tea.KeyEnter
	case "down":
		msg.Type = tea.KeyDown
	default:
		msg = tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune(key)}
	}
	_, cmd := m.Update(msg)
	return cmd
}

func TestIssueListDetailReturnsToSameSelectionAndScroll(t *testing.T) {
	var selected []string
	m, err := ui.NewIssueList(issueRows(25), output.Pagination{Limit: 25, Count: 25, Total: 25}, "", nil, func(key string) (func(int) (string, error), error) {
		selected = append(selected, key)
		return func(int) (string, error) { return "detail " + key, nil }, nil
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 80, Height: 10})
	for range 23 {
		listKey(m, "down")
	}
	before := m.View()
	if !strings.Contains(before, "APP-24") || strings.Contains(before, "APP-1 ") {
		t.Fatalf("list did not scroll: %s", before)
	}
	cmd := listKey(m, "enter")
	if cmd == nil {
		t.Fatal("no detail request")
	}
	m.Update(cmd())
	if !strings.Contains(m.View(), "detail APP-24") {
		t.Fatal(m.View())
	}
	if cmd := listKey(m, "q"); cmd != nil {
		t.Fatal("detail close quit parent")
	}
	if m.View() != before {
		t.Fatalf("list changed on return:\n%s\nwas:\n%s", m.View(), before)
	}
	// g changes hover immediately; Enter must not read the old GetValue.
	listKey(m, "g")
	cmd = listKey(m, "enter")
	m.Update(cmd())
	listKey(m, "q")
	if strings.Join(selected, ",") != "APP-24,APP-1" {
		t.Fatal(selected)
	}
	if cmd := listKey(m, "q"); cmd == nil {
		t.Fatal("list q did not quit")
	}
}

func TestIssueListNextUsesActualCountAndStopsOnEmptyOrFinalPage(t *testing.T) {
	for _, tc := range []struct {
		name  string
		next  []json.RawMessage
		total int64
	}{
		{"empty", nil, 100}, {"final", issueRows(1), 13},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var offsets []int64
			m, err := ui.NewIssueList(issueRows(2), output.Pagination{Offset: 10, Limit: 20, Count: 2, Total: tc.total}, "", func(offset int64) ([]json.RawMessage, error) { offsets = append(offsets, offset); return tc.next, nil }, nil, nil)
			if err != nil {
				t.Fatal(err)
			}
			if !strings.Contains(m.View(), "次ページ") {
				t.Fatal(m.View())
			}
			// Slash must not activate Huh filtering and swallow immediate n.
			if listKey(m, "p") != nil || strings.Contains(m.View(), "前ページ") {
				t.Fatal("p went before initial offset 10")
			}
			listKey(m, "/")
			cmd := listKey(m, "n")
			if cmd == nil {
				t.Fatal("n did not immediately fetch")
			}
			m.Update(cmd())
			if len(offsets) != 1 || offsets[0] != 12 {
				t.Fatal(offsets)
			}
			if strings.Contains(m.View(), "次ページ") || !strings.Contains(m.View(), "offset 12") {
				t.Fatal(m.View())
			}
			if listKey(m, "n") != nil {
				t.Fatal("next enabled at end")
			}
			listKey(m, "p")
			if !strings.Contains(m.View(), "offset 10") || len(offsets) != 1 {
				t.Fatal(m.View(), offsets)
			}
		})
	}
}

func TestIssueListFailedRequestsKeepPageUsableAndSafe(t *testing.T) {
	for _, mode := range []string{"detail", "render", "next", "invalid page"} {
		t.Run(mode, func(t *testing.T) {
			calls := 0
			var selected []string
			failure := fmt.Errorf("failed secret\x1b]52;c;stolen\a\n日本語")
			m, err := ui.NewIssueList(issueRows(2), output.Pagination{Limit: 20, Count: 2, Total: 50}, "secret",
				func(int64) ([]json.RawMessage, error) {
					calls++
					if mode == "invalid page" {
						return []json.RawMessage{json.RawMessage(`{"issueKey":"../bad"}`)}, nil
					}
					return nil, failure
				},
				func(key string) (func(int) (string, error), error) {
					calls++
					selected = append(selected, key)
					if mode == "render" {
						return func(int) (string, error) { return "", failure }, nil
					}
					return nil, failure
				}, nil)
			if err != nil {
				t.Fatal(err)
			}
			listKey(m, "down")
			key := "enter"
			if mode == "next" || mode == "invalid page" {
				key = "n"
			}
			cmd := listKey(m, key)
			if cmd == nil {
				t.Fatal("no request")
			}
			if listKey(m, key) != nil {
				t.Fatal("duplicate request while loading")
			}
			m.Update(cmd())
			view := m.View()
			if calls != 1 || !strings.Contains(view, "APP-2") || !strings.Contains(view, "次ページ") || strings.Contains(view, "stolen") || strings.Contains(view, "secret") {
				t.Fatalf("calls=%d view=%q", calls, view)
			}
			if mode != "invalid page" && !strings.Contains(view, "failed [REDACTED] 日本語") {
				t.Fatal(view)
			}
			// Retained hover, and no Form completion after failed Enter.
			cmd = listKey(m, key)
			m.Update(cmd())
			if calls != 2 {
				t.Fatal(calls)
			}
			if key == "enter" {
				if strings.Join(selected, ",") != "APP-2,APP-2" {
					t.Fatal(selected)
				}
			} else {
				cmd = listKey(m, "enter")
				m.Update(cmd())
				if strings.Join(selected, ",") != "APP-2" {
					t.Fatal(selected)
				}
			}
		})
	}
}

func TestIssueListResizeKeepsHoverAndTruncatesCells(t *testing.T) {
	rows := []json.RawMessage{json.RawMessage(`{"issueKey":"APP-1","summary":"日本語👩‍💻日本語👩‍💻日本語👩‍💻日本語👩‍💻\u001b]52;c;stolen\u0007 secret","status":{"name":"処理中"},"assignee":{"name":"担当者"}}`)}
	m, err := ui.NewIssueList(rows, output.Pagination{Count: 1, Total: 1}, "secret", nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	for _, size := range []tea.WindowSizeMsg{{Width: 20, Height: 5}, {Width: 0, Height: 0}, {Width: 80, Height: 24}} {
		m.Update(size)
		view := m.View()
		if strings.Contains(view, "stolen") || strings.Contains(view, "secret") {
			t.Fatal(view)
		}
		if size.Width == 0 {
			if !strings.Contains(view, "Resize") {
				t.Fatal(view)
			}
			continue
		}
		for _, line := range strings.Split(view, "\n") {
			if ansi.StringWidth(line) > size.Width {
				t.Fatalf("width=%d line=%q", size.Width, line)
			}
		}
	}
	if !strings.Contains(m.View(), "処理中") || !strings.Contains(m.View(), "担当者") {
		t.Fatal(m.View())
	}
	listKey(m, "G")
	if cmd := listKey(m, "enter"); cmd == nil {
		t.Fatal("G then Enter did not select 終了")
	} else if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Fatal("G then Enter read stale selection")
	}
}

func TestIssueListNarrowResizeShowsHoveredAction(t *testing.T) {
	m, err := ui.NewIssueList(issueRows(25), output.Pagination{Offset: 123456789012345, Limit: 25, Count: 25, Total: 123456789012399}, "", nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 20, Height: 5})
	listKey(m, "G")
	if view := m.View(); !strings.Contains(view, "終了") {
		t.Fatalf("hovered action disappeared: %q", view)
	}
}

func TestIssueListDetailResizeFailureReturnsToRetainedPage(t *testing.T) {
	m, err := ui.NewIssueList(issueRows(25), output.Pagination{Count: 25, Total: 25}, "", nil, func(key string) (func(int) (string, error), error) {
		return func(width int) (string, error) {
			if width == 36 {
				return "", fmt.Errorf("render failed")
			}
			return "detail " + key, nil
		}, nil
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	for range 23 {
		listKey(m, "down")
	}
	cmd := listKey(m, "enter")
	m.Update(cmd())
	m.Update(tea.WindowSizeMsg{Width: 36, Height: 10})
	if view := m.View(); !strings.Contains(view, "render failed") || !strings.Contains(view, "APP-24") {
		t.Fatal(view)
	}
	m.Update(tea.WindowSizeMsg{Width: 80, Height: 24})
	cmd = listKey(m, "enter")
	m.Update(cmd())
	if !strings.Contains(m.View(), "detail APP-24") {
		t.Fatal(m.View())
	}
	listKey(m, "q")
	if cmd := listKey(m, "q"); cmd == nil {
		t.Fatal("list not usable after render error")
	}
}

func pageRows(start, n int) []json.RawMessage {
	rows := issueRows(n)
	for i := range rows {
		rows[i] = json.RawMessage(fmt.Sprintf(`{"issueKey":"APP-%d","summary":"page row"}`, start+i))
	}
	return rows
}

func TestIssueListVisitedPagesRestoreHoverScrollAndActions(t *testing.T) {
	var offsets []int64
	m, err := ui.NewIssueList(pageRows(1, 25), output.Pagination{Limit: 25, Count: 25, Total: 55}, "", func(offset int64) ([]json.RawMessage, error) {
		offsets = append(offsets, offset)
		if offset == 25 {
			return pageRows(26, 25), nil
		}
		return pageRows(51, 5), nil
	}, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 80, Height: 10})
	if listKey(m, "p") != nil || strings.Contains(m.View(), "前ページ") {
		t.Fatal("previous before initial page")
	}
	for range 23 {
		listKey(m, "j")
	}
	first := m.View()
	m.Update(listKey(m, "n")())
	for range 9 {
		listKey(m, "j")
	}
	second := m.View()
	if cmd := listKey(m, "p"); cmd != nil {
		t.Fatal("cached previous fetched")
	}
	if m.View() != first {
		t.Fatalf("first page not restored: %s", m.View())
	}
	if cmd := listKey(m, "n"); cmd != nil {
		t.Fatal("cached next fetched")
	}
	if m.View() != second {
		t.Fatalf("second page not restored: %s", m.View())
	}
	listKey(m, "G")
	listKey(m, "k")
	m.Update(listKey(m, "enter")())
	if !strings.Contains(m.View(), "APP-51") || strings.Contains(m.View(), "次ページ") {
		t.Fatal(m.View())
	}
	listKey(m, "p")
	if !strings.Contains(m.View(), "次ページ") {
		t.Fatal(m.View())
	}
	listKey(m, "enter") // retained next action reuses cache
	if !strings.Contains(m.View(), "APP-51") || fmt.Sprint(offsets) != "[25 50]" {
		t.Fatalf("offsets=%v view=%s", offsets, m.View())
	}
}

func TestIssueListLOnlyOpensHoveredIssueAndHReturns(t *testing.T) {
	var selected []string
	m, err := ui.NewIssueList(issueRows(2), output.Pagination{Count: 2, Total: 3}, "", nil, func(key string) (func(int) (string, error), error) {
		selected = append(selected, key)
		return func(int) (string, error) { return "detail " + key, nil }, nil
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	listKey(m, "G")
	if listKey(m, "l") != nil {
		t.Fatal("l acted on quit")
	}
	listKey(m, "k")
	if listKey(m, "l") != nil {
		t.Fatal("l acted on next")
	}
	listKey(m, "g")
	listKey(m, "j")
	before := m.View()
	cmd := listKey(m, "l")
	if cmd == nil {
		t.Fatal("l did not open issue")
	}
	m.Update(cmd())
	if !strings.Contains(m.View(), "h") || !strings.Contains(m.View(), "detail APP-2") {
		t.Fatal(m.View())
	}
	listKey(m, "h")
	if m.View() != before {
		t.Fatal(m.View())
	}
	cmd = listKey(m, "enter")
	m.Update(cmd())
	listKey(m, "q")
	if fmt.Sprint(selected) != "[APP-2 APP-2]" {
		t.Fatal(selected)
	}
}

func TestIssueListRefreshAtomicallyResetsHistoryAndTotal(t *testing.T) {
	var offsets []int64
	refreshes := 0
	m, err := ui.NewIssueList(pageRows(11, 2), output.Pagination{Offset: 10, Limit: 20, Count: 2, Total: 100}, "secret", func(offset int64) ([]json.RawMessage, error) {
		offsets = append(offsets, offset)
		return pageRows(int(offset)+1, 2), nil
	}, nil, func() ([]json.RawMessage, int64, error) {
		refreshes++
		if refreshes == 1 {
			return []json.RawMessage{json.RawMessage(`{"issueKey":"bad"}`)}, 999, nil
		}
		return pageRows(101, 1), 3, nil
	})
	if err != nil {
		t.Fatal(err)
	}
	m.Update(listKey(m, "n")())
	listKey(m, "j")
	before := m.View()
	cmd := listKey(m, "R")
	if cmd == nil {
		t.Fatal("no refresh")
	}
	m.Update(cmd())
	if !strings.Contains(m.View(), "offset 12") || !strings.Contains(m.View(), "total 100") {
		t.Fatal(m.View())
	}
	listKey(m, "p")
	listKey(m, "n")
	if !strings.Contains(m.View(), "APP-14") || !strings.Contains(before, "APP-14") {
		t.Fatal(m.View())
	}
	cmd = listKey(m, "R")
	m.Update(cmd())
	if !strings.Contains(m.View(), "offset 0") || !strings.Contains(m.View(), "total 3") || !strings.Contains(m.View(), "APP-101") || strings.Contains(m.View(), "前ページ") {
		t.Fatal(m.View())
	}
	if listKey(m, "p") != nil {
		t.Fatal("history survived refresh")
	}
	m.Update(listKey(m, "n")())
	if fmt.Sprint(offsets) != "[12 1]" {
		t.Fatal(offsets)
	}
}

func TestIssueListLateDetailRefreshCannotMutateAnotherDetail(t *testing.T) {
	calls := map[string]int{}
	m, err := ui.NewIssueList(issueRows(2), output.Pagination{Count: 2, Total: 2}, "", nil, func(key string) (func(int) (string, error), error) {
		calls[key]++
		content := fmt.Sprintf("%s version %d", key, calls[key])
		return func(int) (string, error) { return content, nil }, nil
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	m.Update(listKey(m, "l")())
	cmd := listKey(m, "R")
	if cmd == nil {
		t.Fatal("nested refresh missing")
	}
	lateA := cmd()
	listKey(m, "h")
	listKey(m, "j")
	m.Update(listKey(m, "l")())
	cmd = listKey(m, "R")
	if cmd == nil {
		t.Fatal("B refresh missing")
	}
	replyB := cmd()
	before := m.View()
	m.Update(lateA)
	if m.View() != before || !strings.Contains(m.View(), "Loading") {
		t.Fatalf("A changed B: %s", m.View())
	}
	m.Update(replyB)
	if !strings.Contains(m.View(), "APP-2 version 2") || strings.Contains(m.View(), "Loading") {
		t.Fatal(m.View())
	}
	listKey(m, "q")
	m.Update(lateA)
	if !strings.Contains(m.View(), "APP-2") || strings.Contains(m.View(), "version") {
		t.Fatal(m.View())
	}
}

func TestIssueListQuitRejectsLateInitialRequests(t *testing.T) {
	for _, key := range []string{"n", "l"} {
		t.Run(key, func(t *testing.T) {
			m, err := ui.NewIssueList(issueRows(2), output.Pagination{Count: 2, Total: 5}, "", func(int64) ([]json.RawMessage, error) { return pageRows(3, 2), nil }, func(string) (func(int) (string, error), error) {
				return func(int) (string, error) { return "late detail", nil }, nil
			}, nil)
			if err != nil {
				t.Fatal(err)
			}
			cmd := listKey(m, key)
			reply := cmd()
			before := m.View()
			listKey(m, "q")
			m.Update(reply)
			if m.View() != before {
				t.Fatalf("late request changed closed list: %s", m.View())
			}
		})
	}
}

func TestIssueListInitialDetailRendersAtCurrentWidth(t *testing.T) {
	m, err := ui.NewIssueList(issueRows(1), output.Pagination{Count: 1, Total: 1}, "", nil, func(string) (func(int) (string, error), error) {
		return func(width int) (string, error) {
			if width != 36 {
				return "", fmt.Errorf("unexpected width %d", width)
			}
			return "detail at 36", nil
		}, nil
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 36, Height: 10})
	m.Update(listKey(m, "l")())
	if !strings.Contains(m.View(), "detail at 36") {
		t.Fatal(m.View())
	}
}

func TestIssueListRefreshFailureKeepsAllPagesAndEmptySuccessResets(t *testing.T) {
	for _, mode := range []string{"count", "list", "display"} {
		t.Run(mode, func(t *testing.T) {
			calls := 0
			fail := true
			m, err := ui.NewIssueList(pageRows(11, 2), output.Pagination{Offset: 10, Limit: 20, Count: 2, Total: 100}, "secret", func(int64) ([]json.RawMessage, error) { calls++; return pageRows(13, 2), nil }, nil, func() ([]json.RawMessage, int64, error) {
				if !fail {
					return nil, 0, nil
				}
				switch mode {
				case "count":
					return nil, 0, fmt.Errorf("count secret\x1b]52;c;stolen\a")
				case "list":
					return nil, 999, fmt.Errorf("list secret\x1b]52;c;stolen\a")
				default:
					return []json.RawMessage{json.RawMessage(`{"issueKey":"APP-99","summary":42}`)}, 999, nil
				}
			})
			if err != nil {
				t.Fatal(err)
			}
			listKey(m, "j")
			m.Update(listKey(m, "n")())
			listKey(m, "G")
			before := m.View()
			m.Update(listKey(m, "R")())
			if !strings.Contains(m.View(), "offset 12") || !strings.Contains(m.View(), "total 100") || strings.Contains(m.View(), "stolen") || strings.Contains(m.View(), "secret") {
				t.Fatal(m.View())
			}
			listKey(m, "p")
			listKey(m, "n")
			if strings.Split(m.View(), "\n")[0] != strings.Split(before, "\n")[0] || calls != 1 {
				t.Fatal(m.View(), calls)
			}
			// Selection of the quit action survives a failed refresh and a cache round trip.
			if cmd := listKey(m, "l"); cmd != nil {
				t.Fatal("lost selected action")
			}
			fail = false
			m.Update(listKey(m, "R")())
			if !strings.Contains(m.View(), "offset 0") || !strings.Contains(m.View(), "total 0") || strings.Contains(m.View(), "APP-") || strings.Contains(m.View(), "前ページ") || strings.Contains(m.View(), "次ページ") {
				t.Fatal(m.View())
			}
		})
	}
}

func TestIssueListCachedPagesRetainSelectionAcrossDetailAndTinyResize(t *testing.T) {
	var opened string
	m, err := ui.NewIssueList(pageRows(1, 25), output.Pagination{Count: 25, Total: 50}, "", func(int64) ([]json.RawMessage, error) { return pageRows(26, 25), nil }, func(key string) (func(int) (string, error), error) {
		opened = key
		return func(int) (string, error) { return "detail", nil }, nil
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	m.Update(tea.WindowSizeMsg{Width: 80, Height: 10})
	for range 23 {
		listKey(m, "j")
	}
	m.View()
	m.Update(listKey(m, "n")())
	for range 9 {
		listKey(m, "j")
	}
	m.View()
	listKey(m, "p")
	m.Update(listKey(m, "l")())
	m.Update(tea.WindowSizeMsg{Width: 10, Height: 3})
	m.Update(tea.WindowSizeMsg{Width: 36, Height: 8})
	listKey(m, "h")
	if !strings.Contains(m.View(), "APP-24") || opened != "APP-24" {
		t.Fatal(m.View(), opened)
	}
	listKey(m, "n")
	m.Update(listKey(m, "l")())
	if opened != "APP-35" {
		t.Fatal(opened)
	}
}
