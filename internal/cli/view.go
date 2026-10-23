package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/config"
	"backlog-cli/internal/markup"
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
	"net/http"
	"os"
	"strconv"

	"github.com/spf13/cobra"
)

func issueViewCommand(ctx context.Context, profile *singleValue, jsonMode *bool, key *string, checked *checkedWriter, client *http.Client) *cobra.Command {
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
		var issue struct {
			IssueKey    string `json:"issueKey"`
			ProjectID   int64  `json:"projectId"`
			Description string `json:"description"`
		}
		if json.Unmarshal(item, &issue) != nil || !output.ValidIssueKey(issue.IssueKey) || issue.ProjectID <= 0 {
			return output.NewFailure("response_error", "invalid API issue detail", 200)
		}
		project, err := api.Project(ctx, strconv.FormatInt(issue.ProjectID, 10))
		if err != nil {
			return err
		}
		var formatting struct {
			Rule string `json:"textFormattingRule"`
		}
		if json.Unmarshal(project, &formatting) != nil {
			return output.NewFailure("response_error", "invalid project textFormattingRule; use --json", 200)
		}
		switch formatting.Rule {
		case "markdown":
		case "backlog":
			return output.NewFailure("response_error", "Backlog formatting is not supported yet; use --json", 200)
		default:
			return output.NewFailure("response_error", "unknown project textFormattingRule; use --json", 200)
		}
		tty := isTerminal(checked.Writer)
		rendered, err := markup.Render(output.Redact(output.SafeText(output.Redact(issue.Description, *key)), *key), settings.SpaceURL, tty)
		if err != nil {
			return output.NewFailure("output_error", "could not render issue detail; use --json", 200)
		}
		return output.WriteIssueDetail(checked, item, settings.SpaceURL, rendered, tty, *key)
	}}
}
