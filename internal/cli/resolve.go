package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
)

func resolveNames(names []string, items []json.RawMessage) ([]int64, error) {
	candidates := make([]struct {
		ID   int64  `json:"id"`
		Name string `json:"name"`
	}, len(items))
	for i, item := range items {
		if json.Unmarshal(item, &candidates[i]) != nil || candidates[i].ID <= 0 || candidates[i].Name == "" {
			return nil, output.NewFailure("response_error", "invalid API name candidate", 200)
		}
	}
	var ids []int64
	for _, name := range names {
		var id int64
		matches := 0
		for _, candidate := range candidates {
			if candidate.Name == name {
				id = candidate.ID
				matches++
			}
		}
		if matches != 1 {
			return nil, output.NewFailure("input_error", "name must match exactly one API candidate: "+name, 0)
		}
		ids = appendUniqueID(ids, id)
	}
	return ids, nil
}

func resolveAssignees(ctx context.Context, api *backlog.Client, project string, names, literalNames []string) ([]int64, error) {
	var ids []int64
	var ordinary []string
	self := false
	for _, name := range names {
		if name == "self" {
			self = true
		} else {
			ordinary = append(ordinary, name)
		}
	}
	ordinary = append(ordinary, literalNames...)
	if self {
		data, err := api.Myself(ctx)
		if err != nil {
			return nil, err
		}
		var user struct {
			ID int64 `json:"id"`
		}
		if err := json.Unmarshal(data, &user); err != nil {
			return nil, output.NewFailure("response_error", "invalid API authenticated user JSON", 200)
		}
		ids = append(ids, user.ID)
	}
	if len(ordinary) > 0 {
		items, err := api.Users(ctx, project)
		if err != nil {
			return nil, err
		}
		resolved, err := resolveNames(ordinary, items)
		if err != nil {
			return nil, err
		}
		for _, id := range resolved {
			ids = appendUniqueID(ids, id)
		}
	}
	return ids, nil
}

func appendUniqueID(ids []int64, id int64) []int64 {
	for _, existing := range ids {
		if existing == id {
			return ids
		}
	}
	return append(ids, id)
}
