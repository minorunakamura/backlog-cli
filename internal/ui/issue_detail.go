package ui

import (
	"backlog-cli/internal/output"
	"context"
	"io"
	"strings"

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
	fromList      bool
	load          func() (func(int) (string, error), error)
	key           string
	busy          bool
	notice        string
	err           error
}

// NewIssueDetail renders trusted display content, already sanitized by the caller, at the given width.
func NewIssueDetail(render func(width int) (string, error), load func() (func(int) (string, error), error), key string, fromList bool) (*IssueDetail, error) {
	return newIssueDetail(render, load, key, fromList, 80, 24)
}

func newIssueDetail(render func(int) (string, error), load func() (func(int) (string, error), error), key string, fromList bool, width, height int) (*IssueDetail, error) {
	content, err := render(max(20, width))
	if err != nil {
		return nil, err
	}
	m := &IssueDetail{viewport: viewport.New(max(20, width), max(4, height)-1), render: render, width: width, height: height, load: load, key: key, fromList: fromList}
	m.viewport.SetContent(content)
	return m, nil
}

type detailRefreshMsg struct {
	owner  *IssueDetail
	render func(int) (string, error)
	err    error
}

func (m *IssueDetail) Init() tea.Cmd { return nil }

func (m *IssueDetail) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	if m.closed {
		return m, nil
	}
	if result, ok := msg.(detailRefreshMsg); ok {
		if result.owner != m {
			return m, nil
		}
		m.busy = false
		err := result.err
		if err == nil {
			var content string
			content, err = result.render(max(20, m.width))
			if err == nil {
				m.render = result.render
				m.viewport.Width = max(20, m.width)
				m.viewport.SetContent(content)
				m.viewport.GotoTop()
				m.notice = ""
				return m, nil
			}
		}
		m.notice = strings.NewReplacer("\n", " ", "\t", " ").Replace(output.Redact(output.SafeText(output.Redact(err.Error(), m.key)), m.key))
		return m, nil
	}

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
		case "h":
			if m.fromList {
				m.closed = true
			}
			return m, nil
		case "q":
			m.closed = true
			return m, nil
		case "ctrl+c":
			m.closed, m.err = true, output.NewFailure("cancelled", "command cancelled", 0)
			return m, nil
		}
		if m.busy {
			return m, nil
		}
		switch key.String() {
		case "R":
			if m.load == nil {
				return m, nil
			}
			m.busy = true
			m.notice = ""
			return m, func() tea.Msg { render, err := m.load(); return detailRefreshMsg{m, render, err} }
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
		if m.fromList {
			return "Resize terminal (20x4 minimum; h/q back, R refresh)"
		}
		return "Resize terminal (20x4 minimum; q closes, R refresh)"
	}
	help := "q close"
	if m.fromList {
		help = "h/q back"
	}
	help += " • R refresh • ↑/↓ j/k • PgUp/PgDn b/Space • g/G Home/End"
	if m.notice != "" {
		help = m.notice + " • " + help
	}
	if m.busy {
		help = "Loading… • " + help
	}
	return m.viewport.View() + "\n" + ansi.Truncate(help, m.width, "…")
}

// Result never quits the parent program; only the standalone runner turns closure into tea.Quit.
func (m *IssueDetail) Result() (closed bool, err error) { return m.closed, m.err }

// ShowIssueDetail runs a standalone viewer; parents use NewIssueDetail and Result instead.
func ShowIssueDetail(ctx context.Context, stdin io.Reader, stdout io.Writer, render func(int) (string, error), load func() (func(int) (string, error), error), key string) error {
	m, err := NewIssueDetail(render, load, key, false)
	if err != nil {
		return err
	}
	// The caller owns OS signals through ctx; Tea's handler can race cancellation at shutdown.
	_, err = tea.NewProgram(detailProgram{m}, tea.WithContext(ctx), tea.WithoutSignalHandler(), tea.WithInput(stdin), tea.WithOutput(stdout), tea.WithAltScreen()).Run()
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
