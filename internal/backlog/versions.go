package backlog

import (
	"context"
	"encoding/json"
)

// Versions lists the milestones of one project.
func (c *Client) Versions(ctx context.Context, project string) ([]json.RawMessage, error) {
	return c.list(ctx, "/api/v2/projects/"+projectSegment(project)+"/versions")
}
