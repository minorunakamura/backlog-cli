package backlog

import (
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
	"net/url"
	"strconv"
)

// IssueFilter is the resolved search shared by the count and page requests.
type IssueFilter struct {
	ProjectIDs   []int64
	StatusIDs    []int64
	AssigneeIDs  []int64
	MilestoneIDs []int64
	Keyword      string
}

func (f IssueFilter) query() url.Values {
	query := url.Values{}
	for name, ids := range map[string][]int64{"projectId[]": f.ProjectIDs, "statusId[]": f.StatusIDs, "assigneeId[]": f.AssigneeIDs, "milestoneId[]": f.MilestoneIDs} {
		for _, id := range ids {
			query.Add(name, strconv.FormatInt(id, 10))
		}
	}
	if f.Keyword != "" {
		query.Set("keyword", f.Keyword)
	}
	return query
}

// IssueCount retrieves the starting total without pagination or sort parameters.
func (c *Client) IssueCount(ctx context.Context, filter IssueFilter) (int64, error) {
	data, err := c.get(ctx, "/api/v2/issues/count?"+filter.query().Encode())
	if err != nil {
		return 0, err
	}
	var result struct {
		Count *int64 `json:"count"`
	}
	if json.Unmarshal(data, &result) != nil || result.Count == nil || *result.Count < 0 {
		return 0, output.NewFailure("response_error", "invalid API issue count JSON", 200)
	}
	return *result.Count, nil
}

// Issues retrieves only the requested page in updated-descending order.
func (c *Client) Issues(ctx context.Context, filter IssueFilter, limit, offset int64) ([]json.RawMessage, error) {
	query := filter.query()
	query.Set("count", strconv.FormatInt(limit, 10))
	query.Set("offset", strconv.FormatInt(offset, 10))
	query.Set("sort", "updated")
	query.Set("order", "desc")
	return c.list(ctx, "/api/v2/issues?"+query.Encode())
}
