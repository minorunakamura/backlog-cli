package backlog

import (
	"context"
	"encoding/json"
)

// IssueTypes lists the types for one project without fetching unrelated candidates.
func (c *Client) IssueTypes(ctx context.Context, project string) ([]json.RawMessage, error) {
	return c.list(ctx, "/api/v2/projects/"+projectSegment(project)+"/issueTypes")
}
