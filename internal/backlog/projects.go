package backlog

import (
	"context"
	"encoding/json"
)

// Projects lists participating projects; the API's all=false default is unchanged.
func (c *Client) Projects(ctx context.Context) ([]json.RawMessage, error) {
	return c.list(ctx, "/api/v2/projects")
}
