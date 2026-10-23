package backlog

import (
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/url"
	"strings"
)

// CreateIssue sends one form request, never retries, and returns the POST response itself.
func (c *Client) CreateIssue(ctx context.Context, form url.Values, creation *output.Creation) (json.RawMessage, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, c.origin+"/api/v2/issues", strings.NewReader(form.Encode()))
	if err != nil {
		return nil, output.NewFailure("input_error", "could not construct API request", 0)
	}
	request.Header.Set("Backlog-API-Key", c.key)
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	if ctx.Err() != nil {
		return nil, transportFailure(ctx, 0)
	}
	creation.State = "unknown"
	response, err := c.http.Do(request)
	if err != nil {
		return nil, transportFailure(ctx, 0)
	}
	if response.StatusCode == http.StatusCreated {
		creation.State = "created"
	} else if response.StatusCode >= 400 && response.StatusCode < 500 {
		creation.State = "not_created"
	}
	creation.Identify(c.origin, response.Header.Get("Location"), nil)
	data, err := io.ReadAll(response.Body)
	closeErr := response.Body.Close()
	creation.Identify(c.origin, "", data)
	if err != nil || closeErr != nil {
		return nil, transportFailure(ctx, response.StatusCode)
	}
	if response.StatusCode != http.StatusCreated {
		code, message := "api_error", "API request rejected"
		if response.StatusCode < 400 {
			code, message = "response_error", "unexpected API HTTP status"
		}
		failure := output.NewFailure(code, message, response.StatusCode)
		var body struct {
			Errors []json.RawMessage `json:"errors"`
		}
		if json.Unmarshal(data, &body) == nil && body.Errors != nil {
			failure.BacklogErrors = body.Errors
		}
		return nil, failure
	}
	return data, nil
}
