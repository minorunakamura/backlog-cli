package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/config"
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
	"strconv"
)

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
