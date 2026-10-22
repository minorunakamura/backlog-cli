package backlog

import (
	"context"
	"encoding/json"
)

func (c *Client) Priorities(ctx context.Context) ([]json.RawMessage, error) {
	return c.list(ctx, "/api/v2/priorities")
}
