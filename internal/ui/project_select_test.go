package ui_test

import (
	"backlog-cli/internal/output"
	"backlog-cli/internal/ui"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/charmbracelet/x/ansi"

	tea "github.com/charmbracelet/bubbletea"
)

func TestProjectSelectKeepsAllCandidatesVisibleWhenTheyFit(t *testing.T) {
	model := newProjectCandidates(t, 5)
	model.Update(tea.WindowSizeMsg{Width: 80, Height: 24})
	want := []string{"APP_01", "APP_02", "APP_03", "APP_04", "APP_05"}
	assertProjectCandidates(t, model, "APP_01", want)
	for i := 2; i <= 5; i++ {
		model.Update(tea.KeyMsg{Type: tea.KeyDown})
		assertProjectCandidates(t, model, fmt.Sprintf("APP_%02d", i), want)
	}
	for i := 4; i >= 1; i-- {
		model.Update(tea.KeyMsg{Type: tea.KeyUp})
		assertProjectCandidates(t, model, fmt.Sprintf("APP_%02d", i), want)
	}
	model.Update(tea.KeyMsg{Type: tea.KeyEnd})
	assertProjectCandidates(t, model, "APP_05", want)
	model.Update(tea.WindowSizeMsg{Width: 40, Height: 10})
	assertProjectCandidates(t, model, "APP_05", want)
	model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if id, cancelled := model.Result(); id != "5" || cancelled {
		t.Fatalf("id=%q cancelled=%v", id, cancelled)
	}
}

func TestProjectSelectScrollsOnlyAtVisibleEdgesAndPreservesSelectionOnResize(t *testing.T) {
	model := newProjectCandidates(t, 30)
	model.Update(tea.WindowSizeMsg{Width: 40, Height: 8})
	first := []string{"APP_01", "APP_02", "APP_03", "APP_04", "APP_05", "APP_06"}
	last := []string{"APP_25", "APP_26", "APP_27", "APP_28", "APP_29", "APP_30"}
	assertProjectCandidates(t, model, "APP_01", first)
	for i := 2; i <= 30; i++ {
		model.Update(tea.KeyMsg{Type: tea.KeyDown})
		want := first
		if i > 6 {
			want = projectCandidateRange(i-5, i)
		}
		assertProjectCandidates(t, model, fmt.Sprintf("APP_%02d", i), want)
	}
	for i := 29; i >= 1; i-- {
		model.Update(tea.KeyMsg{Type: tea.KeyUp})
		want := last
		if i < 25 {
			want = projectCandidateRange(i, i+5)
		}
		assertProjectCandidates(t, model, fmt.Sprintf("APP_%02d", i), want)
	}
	model.Update(tea.KeyMsg{Type: tea.KeyEnd})
	assertProjectCandidates(t, model, "APP_30", last)
	model.Update(tea.KeyMsg{Type: tea.KeyDown})
	assertProjectCandidates(t, model, "APP_01", first)
	model.Update(tea.KeyMsg{Type: tea.KeyUp})
	assertProjectCandidates(t, model, "APP_30", last)
	model.Update(tea.KeyMsg{Type: tea.KeyHome})
	assertProjectCandidates(t, model, "APP_01", first)
	for range 14 {
		model.Update(tea.KeyMsg{Type: tea.KeyDown})
		model.View()
	}
	assertProjectCandidates(t, model, "APP_15", projectCandidateRange(10, 15))
	model.Update(tea.WindowSizeMsg{Width: 30, Height: 5})
	assertProjectCandidates(t, model, "APP_15", []string{"APP_13", "APP_14", "APP_15"})
	model.Update(tea.WindowSizeMsg{Width: 1, Height: 1})
	if view := model.View(); !strings.Contains(view, "Resize") {
		t.Fatalf("missing size guidance: %q", view)
	}
	model.Update(tea.WindowSizeMsg{Width: 80, Height: 40})
	assertProjectCandidates(t, model, "APP_15", projectCandidateRange(1, 30))
	model.Update(tea.WindowSizeMsg{Width: 40, Height: 8})
	assertProjectCandidates(t, model, "APP_15", projectCandidateRange(10, 15))
	model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	if id, cancelled := model.Result(); id != "15" || cancelled {
		t.Fatalf("id=%q cancelled=%v", id, cancelled)
	}
}

func projectCandidateRange(first, last int) []string {
	var labels []string
	for i := first; i <= last; i++ {
		labels = append(labels, fmt.Sprintf("APP_%02d", i))
	}
	return labels
}

func newProjectCandidates(t *testing.T, count int) *ui.ProjectSelect {
	t.Helper()
	items := make([]json.RawMessage, count)
	for i := range items {
		items[i] = json.RawMessage(fmt.Sprintf(`{"id":%d,"projectKey":"APP_%02d","name":"候補"}`, i+1, i+1))
	}
	model, err := ui.NewProjectSelect(items, "")
	if err != nil {
		t.Fatal(err)
	}
	return model
}

func assertProjectCandidates(t *testing.T, model *ui.ProjectSelect, selected string, want []string) {
	t.Helper()
	view := ansi.Strip(model.View())
	var got []string
	for _, line := range strings.Split(view, "\n") {
		if start := strings.Index(line, "APP_"); start >= 0 {
			got = append(got, strings.Fields(line[start:])[0])
		}
	}
	if strings.Join(got, ",") != strings.Join(want, ",") || !strings.Contains(view, "> "+selected+" ") {
		t.Fatalf("selected=%s candidates=%v, want %v; view:\n%s", selected, got, want, view)
	}
	if !strings.Contains(view, "Select project") || !strings.Contains(view, "Enter confirm") {
		t.Fatalf("missing title or help: %q", view)
	}
}

func TestProjectSelectChoosesHoveredProject(t *testing.T) {
	model, err := ui.NewProjectSelect([]json.RawMessage{
		json.RawMessage(`{"id":1,"projectKey":"APP","name":"最初"}`),
		json.RawMessage(`{"id":42,"projectKey":"OTHER","name":"次"}`),
	}, "secret")
	if err != nil {
		t.Fatal(err)
	}
	model.Update(tea.WindowSizeMsg{Width: 80, Height: 24})
	model.Update(tea.KeyMsg{Type: tea.KeyDown})
	model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	id, cancelled := model.Result()
	if id != "42" || cancelled {
		t.Fatalf("id=%q cancelled=%v", id, cancelled)
	}
}

func TestProjectSelectCancellation(t *testing.T) {
	for _, key := range []tea.KeyMsg{{Type: tea.KeyCtrlC}, {Type: tea.KeyEsc}} {
		model, err := ui.NewProjectSelect([]json.RawMessage{json.RawMessage(`{"id":42,"name":"候補"}`)}, "")
		if err != nil {
			t.Fatal(err)
		}
		_, cmd := model.Update(key)
		id, cancelled := model.Result()
		if id != "" || !cancelled || cmd == nil {
			t.Fatalf("key=%v id=%q cancelled=%v", key, id, cancelled)
		}
		if _, ok := cmd().(tea.QuitMsg); !ok {
			t.Fatal("cancellation did not quit")
		}
	}
}

func TestProjectSelectSanitizesCandidatesAndDisablesFiltering(t *testing.T) {
	model, err := ui.NewProjectSelect([]json.RawMessage{
		json.RawMessage(`{"id":1,"projectKey":"APP","name":"日本語👩‍💻\u001b[31m赤\u001b[0m\u009d52;c;bad\u0007\u0001\u0085\u007ftest-secret"}`),
		json.RawMessage(`{"id":2,"projectKey":"NEXT","name":"次"}`),
	}, "test-secret")
	if err != nil {
		t.Fatal(err)
	}
	model.Update(tea.WindowSizeMsg{Width: 80, Height: 24})
	view := ansi.Strip(model.View())
	if !strings.Contains(view, "日本語👩‍💻赤[REDACTED]") || strings.Contains(view, "bad") || strings.Contains(view, "test-secret") {
		t.Fatalf("unsafe view: %q", view)
	}
	for _, r := range view {
		if r < 32 && r != '\n' && r != '\t' || r >= 0x7f && r <= 0x9f {
			t.Fatalf("control %U in %q", r, view)
		}
	}
	model.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'/'}})
	model.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'z'}})
	model.Update(tea.KeyMsg{Type: tea.KeyDown})
	model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	id, cancelled := model.Result()
	if id != "2" || cancelled {
		t.Fatalf("filter captured input: id=%q cancelled=%v", id, cancelled)
	}
}

func TestProjectSelectResizesLongJapaneseByCellWidth(t *testing.T) {
	items := []json.RawMessage{}
	for i := 1; i <= 25; i++ {
		items = append(items, json.RawMessage(fmt.Sprintf(`{"id":%d,"projectKey":"APP","name":%q}`, i, strings.Repeat("長い日本語👩‍💻", 20))))
	}
	model, err := ui.NewProjectSelect(items, "")
	if err != nil {
		t.Fatal(err)
	}
	for _, size := range []tea.WindowSizeMsg{{Width: 32, Height: 8}, {Width: 0, Height: 0}, {Width: 1, Height: 1}, {Width: -1, Height: -1}, {Width: 80, Height: 24}, {Width: 20, Height: 5}} {
		model.Update(size)
		view := model.View()
		if size.Width >= 20 && size.Height >= 5 {
			for _, line := range strings.Split(view, "\n") {
				if ansi.StringWidth(line) > size.Width {
					t.Fatalf("width=%d line width=%d: %q", size.Width, ansi.StringWidth(line), line)
				}
			}
			if len(strings.Split(view, "\n")) > size.Height {
				t.Fatalf("view exceeds terminal height: %q", view)
			}
		} else if !strings.Contains(view, "Resize") {
			t.Fatalf("no size guidance: %q", view)
		}
	}
	model.Update(tea.KeyMsg{Type: tea.KeyEnd})
	model.Update(tea.WindowSizeMsg{Width: 40, Height: 10})
	model.Update(tea.KeyMsg{Type: tea.KeyEnter})
	id, cancelled := model.Result()
	if id != "25" || cancelled {
		t.Fatalf("resize lost hovered item: %q %v", id, cancelled)
	}
}

func TestSelectProjectRunsSelectionAndCancellation(t *testing.T) {
	for _, tc := range []struct{ input, want, code string }{
		{"\x1b[B\r", "2", ""}, {"\x03", "", "cancelled"},
	} {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		var screen bytes.Buffer
		id, err := ui.SelectProject(ctx, strings.NewReader(tc.input), &screen, []json.RawMessage{json.RawMessage(`{"id":1,"name":"最初"}`), json.RawMessage(`{"id":2,"name":"次"}`)}, "")
		cancel()
		if tc.code == "" {
			if err != nil || id != tc.want {
				t.Fatalf("id=%q err=%v screen=%q", id, err, &screen)
			}
		} else {
			var failure *output.Failure
			if !errors.As(err, &failure) || failure.Code != tc.code || failure.ExitCode() != 130 {
				t.Fatalf("id=%q err=%v", id, err)
			}
		}
	}
}
