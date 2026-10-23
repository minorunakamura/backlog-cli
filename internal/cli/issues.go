package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/config"
	"backlog-cli/internal/output"
	"context"
	"io"
	"net/http"
	"os"
	"strconv"

	"github.com/spf13/cobra"
)

func issueCommand(ctx context.Context, profile *singleValue, jsonMode, noInput *bool, key *string, stdin io.Reader, stdout io.Writer, checked *checkedWriter, client *http.Client) *cobra.Command {
	group := &cobra.Command{Use: "issue", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
		return output.NewFailure("input_error", "subcommand required (see --help)", 0)
	}}
	project := &singleValue{}
	limit, offset := &singleValue{value: "20"}, &singleValue{value: "0"}
	var statuses, assignees, milestones []string
	list := &cobra.Command{Use: "list [keyword]", Short: "Search one page of issues", Args: cobra.MaximumNArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		if ctx.Err() != nil {
			return output.NewFailure("cancelled", "command cancelled", 0)
		}
		pageLimit, err := integerValue(limit.value, 1, 100, "limit")
		if err != nil {
			return err
		}
		pageOffset, err := integerValue(offset.value, 0, 1<<63-1, "offset")
		if err != nil {
			return err
		}
		statusIDs, err := searchIDs(statuses)
		if err != nil {
			return err
		}
		assigneeIDs, err := searchIDs(assignees)
		if err != nil {
			return err
		}
		milestoneIDs, err := searchIDs(milestones)
		if err != nil {
			return err
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
		if len(settings.Local.Issue.List.Status) > 0 && len(statuses) == 0 {
			return output.NewFailure("input_error", "local issue.list.status names are not supported yet; specify --status-id", 0)
		}
		id, err := projectForList(ctx, api, project, settings.Local.Project, *jsonMode, *noInput, stdin, stdout, checked, *key)
		if err != nil {
			return err
		}
		projectID, _ := strconv.ParseInt(id, 10, 64)
		filter := backlog.IssueFilter{ProjectIDs: []int64{projectID}, StatusIDs: statusIDs, AssigneeIDs: assigneeIDs, MilestoneIDs: milestoneIDs}
		if len(args) == 1 {
			filter.Keyword = args[0]
		}
		total, err := api.IssueCount(ctx, filter)
		if err != nil {
			return err
		}
		items, err := api.Issues(ctx, filter, pageLimit, pageOffset)
		if err != nil {
			return err
		}
		return output.WriteIssueList(checked, items, output.Pagination{Offset: pageOffset, Limit: pageLimit, Count: int64(len(items)), Total: total}, settings.SpaceURL, *jsonMode, *key)
	}}
	list.Flags().VarP(project, "project", "p", "Project key or positive integer ID")
	list.Flags().Var(limit, "limit", "Page size (1-100)")
	list.Flags().Var(offset, "offset", "Starting offset (nonnegative int64)")
	list.Flags().StringArrayVar(&statuses, "status-id", nil, "Status ID (repeat for OR)")
	list.Flags().StringArrayVar(&assignees, "assignee-id", nil, "Assignee ID (repeat for OR)")
	list.Flags().StringArrayVar(&milestones, "milestone-id", nil, "Milestone ID (repeat for OR)")
	group.AddCommand(list)
	return group
}

func integerValue(value string, min, max int64, name string) (int64, error) {
	parsed, err := strconv.ParseInt(value, 10, 64)
	if err != nil || parsed < min || parsed > max {
		return 0, output.NewFailure("input_error", "invalid "+name+" (integer out of range)", 0)
	}
	return parsed, nil
}

func searchIDs(values []string) ([]int64, error) {
	var ids []int64
	seen := map[int64]bool{}
	for _, value := range values {
		id, err := integerValue(value, 1, 1<<63-1, "search ID")
		if err != nil {
			return nil, err
		}
		if !seen[id] {
			ids = append(ids, id)
			seen[id] = true
		}
	}
	return ids, nil
}
