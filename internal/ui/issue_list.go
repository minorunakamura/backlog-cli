package ui

import (
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"strings"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/charmbracelet/x/ansi"
)

// IssueList keeps the page's Select alive while the shared detail viewer is open.
type IssueList struct {
	*issueListPage
	pages         []*issueListPage
	pageIndex     int
	key           string
	refresh       func() ([]json.RawMessage, int64, error)
	loadPage      func(int64) ([]json.RawMessage, error)
	loadDetail    func(string) (func(int) (string, error), error)
	detail        *IssueDetail
	width, height int
	busy          bool
	closed        bool
	notice        string
	err           error
}

type issueListPage struct {
	field      *huh.Select[string]
	viewport   viewport.Model
	options    []huh.Option[string]
	pagination output.Pagination
}

type listPageMsg struct {
	items   []json.RawMessage
	offset  int64
	total   int64
	refresh bool
	err     error
}

type listDetailMsg struct {
	issueKey string
	render   func(int) (string, error)
	err      error
}

func NewIssueList(items []json.RawMessage, pagination output.Pagination, key string, loadPage func(int64) ([]json.RawMessage, error), loadDetail func(string) (func(int) (string, error), error), refresh func() ([]json.RawMessage, int64, error)) (*IssueList, error) {
	m := &IssueList{key: key, loadPage: loadPage, loadDetail: loadDetail, refresh: refresh}
	if err := m.setPage(items, pagination, false); err != nil {
		return nil, err
	}
	m.pages = []*issueListPage{m.issueListPage}
	m.resize(80, 24)
	return m, nil
}

func (m *IssueList) setPage(items []json.RawMessage, pagination output.Pagination, previous bool) error {
	options := make([]huh.Option[string], 0, len(items)+3)
	for _, item := range items {
		var fields struct {
			IssueKey string `json:"issueKey"`
			Summary  string `json:"summary"`
			Status   struct {
				Name string `json:"name"`
			} `json:"status"`
			Assignee *struct {
				Name string `json:"name"`
			} `json:"assignee"`
		}
		if json.Unmarshal(item, &fields) != nil || !output.ValidIssueKey(fields.IssueKey) {
			return output.NewFailure("response_error", "invalid API issue display fields", 200)
		}
		assignee := ""
		if fields.Assignee != nil {
			assignee = fields.Assignee.Name
		}
		label := fmt.Sprintf("%s  %s  %s  %s", fields.IssueKey, fields.Summary, fields.Status.Name, assignee)
		options = append(options, huh.NewOption(m.safeLine(label), fields.IssueKey))
	}
	if previous {
		options = append(options, huh.NewOption("前ページ", "previous"))
	}
	if _, more := pagination.NextOffset(); more {
		options = append(options, huh.NewOption("次ページ", "next"))
	}
	options = append(options, huh.NewOption("終了", "quit"))
	m.issueListPage = &issueListPage{options: options, pagination: pagination}
	return nil
}

func (m *IssueList) Init() tea.Cmd { return m.field.Init() }

func (m *IssueList) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	if m.closed {
		return m, nil
	}
	if size, ok := msg.(tea.WindowSizeMsg); ok {
		m.resize(size.Width, size.Height)
		if m.detail != nil {
			m.detail.Update(msg)
			return m, m.closeDetail()
		}
		return m, nil
	}
	if result, ok := msg.(listPageMsg); ok {
		m.busy = false
		if result.err != nil {
			m.notice = m.safeLine(result.err.Error())
			return m, nil
		}
		page := m.pagination
		page.Offset, page.Count = result.offset, int64(len(result.items))
		if result.refresh {
			page.Total = result.total
		}
		if err := m.setPage(result.items, page, !result.refresh); err != nil {
			m.notice = m.safeLine(err.Error())
			return m, nil
		}
		if result.refresh {
			m.pages = []*issueListPage{m.issueListPage}
			m.pageIndex = 0
		} else {
			m.pages = append(m.pages, m.issueListPage)
			m.pageIndex++
		}
		m.resize(m.width, m.height)
		return m, nil
	}
	if result, ok := msg.(listDetailMsg); ok {
		m.busy = false
		if result.err != nil {
			m.notice = m.safeLine(result.err.Error())
			return m, nil
		}
		detail, err := newIssueDetail(result.render, func() (func(int) (string, error), error) { return m.loadDetail(result.issueKey) }, m.key, true, m.width, m.height)
		if err != nil {
			m.notice = m.safeLine(err.Error())
			return m, nil
		}
		m.detail = detail
		return m, m.closeDetail()
	}
	if _, ok := msg.(detailRefreshMsg); ok && m.detail == nil {
		return m, nil
	}
	if m.detail != nil {
		_, cmd := m.detail.Update(msg)
		if closeCmd := m.closeDetail(); closeCmd != nil {
			return m, closeCmd
		}
		return m, cmd
	}
	if key, ok := msg.(tea.KeyMsg); ok {
		switch key.String() {
		case "ctrl+c":
			m.closed = true
			m.err = output.NewFailure("cancelled", "command cancelled", 0)
			return m, tea.Quit
		case "q":
			m.closed = true
			return m, tea.Quit
		}
		if m.busy {
			return m, nil
		}
		if key.String() == "R" && m.refresh != nil {
			m.busy = true
			m.notice = ""
			return m, func() tea.Msg {
				items, total, err := m.refresh()
				return listPageMsg{items: items, total: total, refresh: true, err: err}
			}
		}
		if key.String() == "p" {
			m.previousPage()
			return m, nil
		}
		if key.String() == "n" {
			return m, m.nextPage()
		}
		if key.Type == tea.KeyEnter || key.String() == "l" {
			selected, _ := m.field.Hovered()
			if key.String() == "l" && (selected == "quit" || selected == "next" || selected == "previous") {
				return m, nil
			}
			if selected == "quit" {
				m.closed = true
				return m, tea.Quit
			}
			if selected == "previous" {
				m.previousPage()
				return m, nil
			}
			if selected == "next" {
				return m, m.nextPage()
			}
			m.busy = true
			m.notice = ""
			return m, func() tea.Msg {
				render, err := m.loadDetail(selected)
				return listDetailMsg{issueKey: selected, render: render, err: err}
			}
		}
	}
	_, cmd := m.field.Update(msg)
	return m, cmd
}

func (m *IssueList) previousPage() {
	if m.pageIndex == 0 {
		return
	}
	m.pageIndex--
	m.issueListPage = m.pages[m.pageIndex]
	m.resize(m.width, m.height)
}

func (m *IssueList) nextPage() tea.Cmd {
	if m.pageIndex+1 < len(m.pages) {
		m.pageIndex++
		m.issueListPage = m.pages[m.pageIndex]
		m.resize(m.width, m.height)
		return nil
	}

	offset, more := m.pagination.NextOffset()
	if !more {
		return nil
	}
	m.busy = true
	m.notice = ""
	return func() tea.Msg {
		items, err := m.loadPage(offset)
		return listPageMsg{items: items, offset: offset, err: err}
	}
}

func (m *IssueList) closeDetail() tea.Cmd {
	if closed, err := m.detail.Result(); closed {
		m.detail = nil
		if err != nil {
			if failure, ok := err.(*output.Failure); ok && failure.Code == "cancelled" {
				m.closed = true
				m.err = err
				return tea.Quit
			}
			m.notice = m.safeLine(err.Error())
		}
	}
	return nil
}

func (m *IssueList) safeLine(value string) string {
	return strings.NewReplacer("\n", " ", "\t", " ").Replace(output.Redact(output.SafeText(output.Redact(value, m.key)), m.key))
}

func (m *IssueList) View() string {
	if m.detail != nil {
		return m.detail.View()
	}
	if m.width < 20 || m.height < 5 {
		return "Resize terminal (20x5 minimum; q quits)"
	}
	// Huh's bounded view resets scroll on Update; scroll its unbounded options in the parent.
	title, options, _ := strings.Cut(m.field.View(), "\n")
	m.viewport.SetContent(options)
	m.viewport.SetYOffset(m.viewport.YOffset)
	selected, _ := m.field.Hovered()
	for i, option := range m.options {
		if option.Value != selected {
			continue
		}
		if i < m.viewport.YOffset {
			m.viewport.SetYOffset(i)
		} else if i >= m.viewport.YOffset+m.viewport.Height {
			m.viewport.SetYOffset(i - m.viewport.Height + 1)
		}
		break
	}
	status := m.notice
	if m.busy {
		status = "Loading…"
	}
	return title + "\n" + m.viewport.View() + "\n" + ansi.Truncate(status, m.width, "…") + "\n" + ansi.Truncate("q quit • R refresh • p/n page • Enter/l open • ↑/↓ j/k", m.width, "…")
}

// ShowIssueList runs the list and its nested detail without exiting between views.
func ShowIssueList(ctx context.Context, stdin io.Reader, stdout io.Writer, model *IssueList) error {
	// The caller owns OS signals through ctx; Tea's handler can race cancellation at shutdown.
	_, err := tea.NewProgram(model, tea.WithContext(ctx), tea.WithoutSignalHandler(), tea.WithInput(stdin), tea.WithOutput(stdout), tea.WithAltScreen()).Run()
	if ctx.Err() != nil {
		return output.NewFailure("cancelled", "command cancelled", 0)
	}
	if model.err != nil {
		return model.err
	}
	if err != nil {
		return output.NewFailure("output_error", "could not run issue list", 0)
	}
	return nil
}

func (m *IssueList) resize(width, height int) {
	m.width, m.height = width, height
	width, height = max(20, width), max(5, height)
	if m.field != nil && m.viewport.Width == width && m.viewport.Height == height-3 {
		return
	}
	selected := m.options[0].Value
	if m.field != nil {
		selected, _ = m.field.Hovered()
	}
	m.viewport.Width, m.viewport.Height = width, height-3
	options := make([]huh.Option[string], len(m.options))
	theme := huh.ThemeCharm()
	width -= theme.Focused.Base.GetHorizontalBorderSize()
	frame := theme.Focused.Base.GetHorizontalFrameSize() + ansi.StringWidth(theme.Focused.SelectSelector.String())
	for i, option := range m.options {
		options[i] = huh.NewOption(ansi.Truncate(option.Key, width-frame, "…"), option.Value)
	}
	title := ansi.Truncate(fmt.Sprintf("Issues • offset %d • total %d", m.pagination.Offset, m.pagination.Total), width-frame, "…")
	m.field = huh.NewSelect[string]().Title(title).Options(options...).Value(&selected)
	keymap := huh.NewDefaultKeyMap()
	keymap.Select.Filter.SetEnabled(false)
	keymap.Select.Next.SetEnabled(false)
	keymap.Select.Submit.SetEnabled(false)
	keymap.Select.Prev.SetEnabled(false)
	m.field.WithKeyMap(keymap)
	m.field.Focus()
	m.field.WithWidth(width)
}
