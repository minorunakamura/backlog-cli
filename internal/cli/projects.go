package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/config"
	"backlog-cli/internal/output"
	"backlog-cli/internal/ui"
	"context"
	"encoding/json"
	"io"
	"strconv"

	"github.com/charmbracelet/x/term"
)

func projectForList(ctx context.Context, api *backlog.Client, project *singleValue, local string, jsonMode, noInput bool, stdin io.Reader, stdout io.Writer, checked *checkedWriter, key string) (string, error) {
	value := project.value
	if !project.seen {
		value = local
	}
	if value == "" && !project.seen {
		if jsonMode || noInput || !isTerminal(stdin) || !isTerminal(stdout) {
			return "", output.NewFailure("input_error", "project is required (--project/-p)", 0)
		}
		projects, err := api.Projects(ctx)
		if err != nil {
			return "", err
		}
		var screen io.Writer = checked
		if file, ok := stdout.(term.File); ok {
			screen = checkedTerminal{checked, file}
		}
		value, err = ui.SelectProject(ctx, stdin, screen, projects, key)
		if err != nil {
			return "", err
		}
	}
	return resolveProject(ctx, api, value)
}

func resolveProject(ctx context.Context, api *backlog.Client, value string) (string, error) {
	id, err := config.ProjectID(value)
	if err != nil {
		return "", output.NewFailure("input_error", err.Error(), 0)
	}
	if id != "" {
		return id, nil
	}
	data, err := api.Project(ctx, value)
	if err != nil {
		return "", err
	}
	var fields struct {
		ID int64 `json:"id"`
	}
	if json.Unmarshal(data, &fields) != nil || fields.ID <= 0 {
		return "", output.NewFailure("response_error", "invalid API project ID", 200)
	}
	return strconv.FormatInt(fields.ID, 10), nil
}
