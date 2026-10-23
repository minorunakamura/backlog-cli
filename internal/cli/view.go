package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/config"
	"backlog-cli/internal/output"
	"context"
	"net/http"
	"os"

	"github.com/spf13/cobra"
)

func issueViewCommand(ctx context.Context, profile *singleValue, jsonMode *bool, key *string, checked *checkedWriter, client *http.Client) *cobra.Command {
	return &cobra.Command{Use: "view <issueIdOrKey>", Short: "Fetch one issue as JSON", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		if ctx.Err() != nil {
			return output.NewFailure("cancelled", "command cancelled", 0)
		}
		if !output.ValidIssueKey(args[0]) {
			if _, err := integerValue(args[0], 1, 1<<63-1, "issue ID or key"); err != nil {
				return err
			}
		}
		if !*jsonMode {
			return output.NewFailure("input_error", "human-readable issue detail is not implemented; use --json", 0)
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
		return output.WriteIssue(checked, item, settings.SpaceURL, *key)
	}}
}
