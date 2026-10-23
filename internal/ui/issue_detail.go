package ui

import (
	"backlog-cli/internal/output"
	"context"
	"io"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/x/ansi"
)

// IssueDetail can be embedded in a parent that resumes its own view when Result reports closed.
type IssueDetail struct {
	viewport      viewport.Model
	render        func(int) (string, error)
	width, height int
	closed        bool
	err           error
}

// NewIssueDetail renders trusted display content, already sanitized by the caller, at the given width.
func NewIssueDetail(render func(width int) (string, error)) (*IssueDetail, error) {
	content, err := render(80)
	if err != nil {
		return nil, err
	}
	m := &IssueDetail{viewport: viewport.New(80, 23), render: render, width: 80, height: 24}
	m.viewport.SetContent(content)
	return m, nil
}

func (m *IssueDetail) Init() tea.Cmd { return nil }

func (m *IssueDetail) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	if size, ok := msg.(tea.WindowSizeMsg); ok {
		m.width, m.height = size.Width, size.Height
		width := max(20, size.Width)
		m.viewport.Height = max(4, size.Height) - 1
		if width != m.viewport.Width {
			content, err := m.render(width)
			if err != nil {
				m.closed, m.err = true, err
				return m, nil
			}
			m.viewport.Width = width
			m.viewport.SetContent(content)
		}
		m.viewport.SetYOffset(m.viewport.YOffset)
		return m, nil
	}
	if key, ok := msg.(tea.KeyMsg); ok {
		switch key.String() {
		case "q":
			m.closed = true
			return m, nil
		case "ctrl+c":
			m.closed, m.err = true, output.NewFailure("cancelled", "command cancelled", 0)
			return m, nil
		case "g", "home":
			m.viewport.GotoTop()
			return m, nil
		case "G", "end":
			m.viewport.GotoBottom()
			return m, nil
		}
	}
	var cmd tea.Cmd
	m.viewport, cmd = m.viewport.Update(msg)
	return m, cmd
}

func (m *IssueDetail) View() string {
	if m.width < 20 || m.height < 4 {
		return "Resize terminal (20x4 minimum; q closes)"
	}
	return m.viewport.View() + "\n" + ansi.Truncate("↑/↓ j/k • PgUp/PgDn b/Space • g/G Home/End • q close", m.width, "…")
}

// Result never quits the parent program; only the standalone runner turns closure into tea.Quit.
func (m *IssueDetail) Result() (closed bool, err error) { return m.closed, m.err }

// ShowIssueDetail runs a standalone viewer; parents use NewIssueDetail and Result instead.
func ShowIssueDetail(ctx context.Context, stdin io.Reader, stdout io.Writer, render func(int) (string, error)) error {
	m, err := NewIssueDetail(render)
	if err != nil {
		return err
	}
	_, err = tea.NewProgram(detailProgram{m}, tea.WithContext(ctx), tea.WithInput(stdin), tea.WithOutput(stdout), tea.WithAltScreen()).Run()
	if ctx.Err() != nil {
		return output.NewFailure("cancelled", "command cancelled", 0)
	}
	if m.err != nil {
		return m.err
	}
	if err != nil {
		return output.NewFailure("output_error", "could not run issue detail viewer", 0)
	}
	return nil
}

type detailProgram struct{ *IssueDetail }

func (m detailProgram) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	_, cmd := m.IssueDetail.Update(msg)
	if m.closed {
		return m, tea.Quit
	}
	return m, cmd
}
