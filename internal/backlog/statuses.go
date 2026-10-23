package backlog

import (
	"context"
	"encoding/json"
)

// Statuses lists the statuses of one project.
func (c *Client) Statuses(ctx context.Context, project string) ([]json.RawMessage, error) {
	return c.list(ctx, "/api/v2/projects/"+projectSegment(project)+"/statuses")
}
