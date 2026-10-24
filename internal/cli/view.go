package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/config"
	"backlog-cli/internal/markup"
	"backlog-cli/internal/output"
	"backlog-cli/internal/ui"
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"strconv"

	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/term"
	"github.com/spf13/cobra"
)

func issueViewCommand(ctx context.Context, profile *singleValue, jsonMode, noInput *bool, key *string, stdin io.Reader, checked *checkedWriter, client *http.Client) *cobra.Command {
	return &cobra.Command{Use: "view <issueIdOrKey>", Short: "Show one issue", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		if ctx.Err() != nil {
			return output.NewFailure("cancelled", "command cancelled", 0)
		}
		if !output.ValidIssueKey(args[0]) {
			if _, err := integerValue(args[0], 1, 1<<63-1, "issue ID or key"); err != nil {
				return err
			}
		}
		cwd, err := os.Getwd()
		if err != nil {
			return output.NewFailure("config_error", "working directory is unavailable", 0)
		}
		settings, err := config.Load(profile.value, cwd)
		if err != nil {
			return output.NewFailure("config_error", err.Error(), 0)
		}
		*key = settings.APIKey
		api, err := backlog.New(settings.SpaceURL, *key, client)
		if err != nil {
			return err
		}
		item, err := api.Issue(ctx, args[0])
		if err != nil {
			return err
		}
		if *jsonMode {
			return output.WriteIssue(checked, item, settings.SpaceURL, *key)
		}

		tty := isTerminal(checked.Writer)
		render, err := prepareIssueDetail(ctx, api, item, settings.SpaceURL, *key, tty)
		if err != nil {
			return err
		}
		if tty && isTerminal(stdin) && !*noInput {
			uiCtx, cancel := context.WithCancel(ctx)
			defer cancel()
			var screen io.Writer = checked
			if file, ok := checked.Writer.(term.File); ok {
				screen = checkedTerminal{checked, file}
			}
			return ui.ShowIssueDetail(uiCtx, stdin, screen, func(width int) (string, error) {
				content, err := render(width)
				return ansi.Wrap(content, width, ""), err
			}, func() (func(int) (string, error), error) {
				return loadIssueDetail(uiCtx, api, args[0], settings.SpaceURL, *key)
			}, *key)
		}
		content, err := render(80)
		if err != nil {
			return err
		}
		if _, err := io.WriteString(checked, content); err != nil {
			return output.NewFailure("output_error", "could not write output", 200)
		}
		return nil
	}}
}

func loadIssueDetail(ctx context.Context, api *backlog.Client, issueKey, origin, key string) (func(int) (string, error), error) {
	item, err := api.Issue(ctx, issueKey)
	if err != nil {
		return nil, err
	}
	render, err := prepareIssueDetail(ctx, api, item, origin, key, true)
	if err != nil {
		return nil, err
	}
	return func(width int) (string, error) {
		content, err := render(width)
		return ansi.Wrap(content, width, ""), err
	}, nil
}

// prepareIssueDetail shares API validation, notation selection and safe rendering for both entry points.
func prepareIssueDetail(ctx context.Context, api *backlog.Client, item json.RawMessage, origin, key string, tty bool) (func(int) (string, error), error) {
	var issue struct {
		IssueKey    string `json:"issueKey"`
		ProjectID   int64  `json:"projectId"`
		Description string `json:"description"`
	}
	if json.Unmarshal(item, &issue) != nil || !output.ValidIssueKey(issue.IssueKey) || issue.ProjectID <= 0 {
		return nil, output.NewFailure("response_error", "invalid API issue detail", 200)
	}
	project, err := api.Project(ctx, strconv.FormatInt(issue.ProjectID, 10))
	if err != nil {
		return nil, err
	}
	var formatting struct {
		Rule string `json:"textFormattingRule"`
	}
	if json.Unmarshal(project, &formatting) != nil {
		return nil, output.NewFailure("response_error", "invalid project textFormattingRule; use --json", 200)
	}
	render := markup.RenderWidth
	switch formatting.Rule {
	case "markdown":
	case "backlog":
		render = markup.RenderBacklogWidth
	default:
		return nil, output.NewFailure("response_error", "unknown project textFormattingRule; use --json", 200)
	}

	source := output.Redact(output.SafeText(output.Redact(issue.Description, key)), key)
	return func(width int) (string, error) {
		rendered, err := render(source, origin, tty, width)
		if err != nil {
			return "", output.NewFailure("output_error", "could not render issue detail; use --json", 200)
		}
		var content bytes.Buffer
		if err := output.WriteIssueDetail(&content, item, origin, rendered, tty, key); err != nil {
			return "", err
		}
		return content.String(), nil
	}, nil
}
