package backlog

import (
	"backlog-cli/internal/output"
	"bytes"
	"context"
	"encoding/json"
	"net/url"
	"strings"
)

// Projects lists participating projects; the API's all=false default is unchanged.
func (c *Client) Projects(ctx context.Context) ([]json.RawMessage, error) {
	return c.list(ctx, "/api/v2/projects")
}

// Project retrieves one project by key or ID, retaining the API object.
func (c *Client) Project(ctx context.Context, project string) (json.RawMessage, error) {
	data, err := c.get(ctx, "/api/v2/projects/"+projectSegment(project))
	if err != nil {
		return nil, err
	}
	if len(bytes.TrimSpace(data)) == 0 || bytes.TrimSpace(data)[0] != '{' || !json.Valid(data) {
		return nil, output.NewFailure("response_error", "invalid API project JSON", 200)
	}
	return json.RawMessage(data), nil
}

func projectSegment(project string) string {
	// PathEscape leaves dot segments intact; encode them to avoid path normalization.
	if project == "." || project == ".." {
		return strings.ReplaceAll(project, ".", "%2E")
	}
	return url.PathEscape(project)
}
