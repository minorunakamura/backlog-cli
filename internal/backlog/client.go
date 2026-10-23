package backlog

import (
	"backlog-cli/internal/config"
	"backlog-cli/internal/output"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"time"
)

type Client struct {
	origin string
	key    string
	http   *http.Client
}

// New retains an injected client's trusted transport, but enforces request safety policy.
func New(origin, key string, client *http.Client) (*Client, error) {
	origin, err := config.Origin(origin)
	if err != nil {
		return nil, output.NewFailure("config_error", err.Error(), 0)
	}
	if client == nil {
		client = http.DefaultClient
	}
	copyClient := *client
	copyClient.Timeout = 30 * time.Second
	copyClient.CheckRedirect = func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
	return &Client{origin: origin, key: key, http: &copyClient}, nil
}

func (c *Client) get(ctx context.Context, path string) ([]byte, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, c.origin+path, nil)
	if err != nil {
		return nil, output.NewFailure("config_error", "could not construct API request", 0)
	}
	request.Header.Set("Backlog-API-Key", c.key)
	response, err := c.http.Do(request)
	if err != nil {
		return nil, transportFailure(ctx, 0)
	}
	data, err := io.ReadAll(response.Body)
	closeErr := response.Body.Close()
	if err != nil || closeErr != nil {
		return nil, transportFailure(ctx, response.StatusCode)
	}
	if response.StatusCode != http.StatusOK {
		code, message := "api_error", "API request rejected"
		if response.StatusCode >= 200 && response.StatusCode < 400 {
			code, message = "response_error", "unexpected API HTTP status"
		}
		failure := output.NewFailure(code, message, response.StatusCode)
		var body struct {
			Errors []json.RawMessage `json:"errors"`
		}
		if json.Unmarshal(data, &body) == nil && body.Errors != nil {
			for _, item := range body.Errors {
				safe, err := output.RedactJSON(item, c.key)
				if err == nil {
					failure.BacklogErrors = append(failure.BacklogErrors, safe)
				}
			}
		}
		return nil, failure
	}
	return data, nil
}

func (c *Client) list(ctx context.Context, path string) ([]json.RawMessage, error) {
	data, err := c.get(ctx, path)
	if err != nil {
		return nil, err
	}
	var items []json.RawMessage
	if len(bytes.TrimSpace(data)) == 0 || bytes.TrimSpace(data)[0] != '[' || json.Unmarshal(data, &items) != nil {
		return nil, output.NewFailure("response_error", "invalid API list JSON", http.StatusOK)
	}
	for _, item := range items {
		if len(item) == 0 || bytes.TrimSpace(item)[0] != '{' {
			return nil, output.NewFailure("response_error", "API list item is not an object", http.StatusOK)
		}
	}
	if items == nil {
		items = []json.RawMessage{}
	}
	return items, nil
}

func transportFailure(ctx context.Context, status int) *output.Failure {
	if errors.Is(ctx.Err(), context.Canceled) {
		return output.NewFailure("cancelled", "command cancelled", status)
	}
	return output.NewFailure("transport_error", "API communication failed", status)
}
