package backlog

import (
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
)

// Users lists the users of one project.
func (c *Client) Users(ctx context.Context, project string) ([]json.RawMessage, error) {
	return c.list(ctx, "/api/v2/projects/"+projectSegment(project)+"/users")
}

// Myself retrieves the user authenticated by this client's API key.
func (c *Client) Myself(ctx context.Context) (json.RawMessage, error) {
	data, err := c.get(ctx, "/api/v2/users/myself")
	if err != nil {
		return nil, err
	}
	var user struct {
		ID int64 `json:"id"`
	}
	if json.Unmarshal(data, &user) != nil || user.ID <= 0 {
		return nil, output.NewFailure("response_error", "invalid API authenticated user ID", 200)
	}
	return json.RawMessage(data), nil
}
