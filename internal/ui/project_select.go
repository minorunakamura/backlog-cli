package ui

import (
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
	"io"
	"strconv"
	"strings"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/huh"
	"github.com/charmbracelet/x/ansi"
)

// ProjectSelect selects one project for this invocation only.
type ProjectSelect struct {
	field         *huh.Select[string]
	viewport      viewport.Model
	options       []huh.Option[string]
	width, height int
	selected      string
	cancelled     bool
}

func NewProjectSelect(items []json.RawMessage, key string) (*ProjectSelect, error) {
	if len(items) == 0 {
		return nil, output.NewFailure("input_error", "no projects available to select", 0)
	}
	options := make([]huh.Option[string], 0, len(items))
	for _, item := range items {
		var fields struct {
			ID         int64  `json:"id"`
			ProjectKey string `json:"projectKey"`
			Name       string `json:"name"`
		}
		if json.Unmarshal(item, &fields) != nil || fields.ID <= 0 {
			return nil, output.NewFailure("response_error", "invalid API project candidate", 200)
		}
		label := fields.ProjectKey + "  " + fields.Name
		label = output.Redact(output.SafeText(output.Redact(label, key)), key)
		label = strings.NewReplacer("\n", " ", "\t", " ").Replace(label)
		options = append(options, huh.NewOption(label, strconv.FormatInt(fields.ID, 10)))
	}
	m := &ProjectSelect{options: options}
	m.resize(80, 24)
	return m, nil
}

func (m *ProjectSelect) Init() tea.Cmd { return m.field.Init() }

func (m *ProjectSelect) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	if size, ok := msg.(tea.WindowSizeMsg); ok {
		m.resize(size.Width, size.Height)
		return m, nil
	}
	if key, ok := msg.(tea.KeyMsg); ok {
		switch key.Type {
		case tea.KeyCtrlC, tea.KeyEsc:
			m.cancelled = true
			return m, tea.Quit
		case tea.KeyEnter:
			m.selected, _ = m.field.Hovered()
			return m, tea.Quit
		}
	}
	_, cmd := m.field.Update(msg)
	return m, cmd
}

func (m *ProjectSelect) View() string {
	if m.width < 20 || m.height < 5 {
		return "Resize terminal (20x5 minimum; Esc cancels)"
	}
	// Huh v0.8.0 resets bounded views to the selected row on every Update.
	// Keep its title fixed and scroll its unbounded option view in the parent.
	title, options, _ := strings.Cut(m.field.View(), "\n")
	m.viewport.SetContent(options)
	m.viewport.SetYOffset(m.viewport.YOffset)
	id, _ := m.field.Hovered()
	for i, option := range m.options {
		if option.Value != id {
			continue
		}
		if i < m.viewport.YOffset {
			m.viewport.SetYOffset(i)
		} else if i >= m.viewport.YOffset+m.viewport.Height {
			m.viewport.SetYOffset(i - m.viewport.Height + 1)
		}
		break
	}
	return title + "\n" + m.viewport.View() + "\n" + ansi.Truncate("↑/↓ select • Enter confirm • Esc cancel", m.width, "…")
}

func (m *ProjectSelect) resize(width, height int) {
	m.width, m.height = width, height
	width, height = max(20, width), max(5, height)
	id := m.options[0].Value
	if m.field != nil {
		id, _ = m.field.Hovered()
	}
	m.viewport.Width, m.viewport.Height = width, height-2
	options := make([]huh.Option[string], len(m.options))
	theme := huh.ThemeCharm()
	width -= theme.Focused.Base.GetHorizontalBorderSize()
	frame := theme.Focused.Base.GetHorizontalFrameSize() + ansi.StringWidth(theme.Focused.SelectSelector.String())
	for i, option := range m.options {
		options[i] = huh.NewOption(ansi.Truncate(option.Key, width-frame, "…"), option.Value)
	}
	// Restore hover after Options so Huh's unbounded view keeps its offset at zero.
	m.field = huh.NewSelect[string]().Title("Select project").Options(options...).Value(&id)
	keymap := huh.NewDefaultKeyMap()
	keymap.Select.Filter.SetEnabled(false)
	m.field.WithKeyMap(keymap)
	m.field.Focus()
	m.field.WithWidth(width)
}

func (m *ProjectSelect) Result() (string, bool) { return m.selected, m.cancelled }

// SelectProject runs the terminal UI using the caller's streams and cancellation context.
func SelectProject(ctx context.Context, stdin io.Reader, stdout io.Writer, items []json.RawMessage, key string) (string, error) {
	model, err := NewProjectSelect(items, key)
	if err != nil {
		return "", err
	}
	// The caller owns OS signals through ctx; Tea's handler can race cancellation at shutdown.
	_, err = tea.NewProgram(model, tea.WithContext(ctx), tea.WithoutSignalHandler(), tea.WithInput(stdin), tea.WithOutput(stdout), tea.WithAltScreen()).Run()
	if model.cancelled || ctx.Err() != nil {
		return "", output.NewFailure("cancelled", "command cancelled", 0)
	}
	if err != nil {
		return "", output.NewFailure("output_error", "could not run project selection", 0)
	}
	return model.selected, nil
}
