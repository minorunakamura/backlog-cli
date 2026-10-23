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
	"strings"

	"github.com/spf13/cobra"
)

func issueCommand(ctx context.Context, profile *singleValue, jsonMode, noInput *bool, key *string, stdin io.Reader, stdout io.Writer, checked *checkedWriter, client *http.Client, creation *output.Creation) *cobra.Command {
	group := &cobra.Command{Use: "issue", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
		return output.NewFailure("input_error", "subcommand required (see --help)", 0)
	}}
	for _, name := range []string{"list", "mine"} {
		group.AddCommand(issueSearchCommand(name, ctx, profile, jsonMode, noInput, key, stdin, stdout, checked, client))
	}
	group.AddCommand(issueCreateCommand(ctx, profile, jsonMode, key, stdin, checked, client, creation))
	group.AddCommand(issueViewCommand(ctx, profile, jsonMode, noInput, key, stdin, checked, client))
	return group
}

func issueSearchCommand(name string, ctx context.Context, profile *singleValue, jsonMode, noInput *bool, key *string, stdin io.Reader, stdout io.Writer, checked *checkedWriter, client *http.Client) *cobra.Command {
	project := &singleValue{}
	limit, offset := &singleValue{value: "20"}, &singleValue{value: "0"}
	var statuses, assignees, milestones, statusNames, milestoneNames, assigneeNames, literalAssigneeNames []string
	list := &cobra.Command{Use: name + " [keyword]", Short: "Search one page of issues", Args: cobra.MaximumNArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		if ctx.Err() != nil {
			return output.NewFailure("cancelled", "command cancelled", 0)
		}
		if name == "mine" {
			if cmd.Flags().Changed("assignee") || cmd.Flags().Changed("assignee-name") || cmd.Flags().Changed("assignee-id") {
				return output.NewFailure("input_error", "mine does not accept assignee flags; use issue list", 0)
			}
			assigneeNames = []string{"self"}
		}
		pageLimit, err := integerValue(limit.value, 1, 100, "limit")
		if err != nil {
			return err
		}
		pageOffset, err := integerValue(offset.value, 0, 1<<63-1, "offset")
		if err != nil {
			return err
		}
		if len(statusNames) > 0 && len(statuses) > 0 {
			return output.NewFailure("input_error", "status names and IDs cannot be combined", 0)
		}
		if len(milestoneNames) > 0 && len(milestones) > 0 {
			return output.NewFailure("input_error", "milestone names and IDs cannot be combined", 0)
		}
		if (len(assigneeNames) > 0 || len(literalAssigneeNames) > 0) && len(assignees) > 0 {
			return output.NewFailure("input_error", "assignee names and IDs cannot be combined", 0)
		}
		statusAll := false
		for _, name := range statusNames {
			if name == "all" {
				if len(statusNames) != 1 {
					return output.NewFailure("input_error", "status all must be specified alone", 0)
				}
				statusAll = true
			}
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
		keyword := ""
		if len(args) == 1 {
			keyword = args[0]
		}
		crossProject := project.seen && project.value == "all"
		if crossProject {
			if len(statusNames) > 0 || len(statuses) > 0 || len(milestoneNames) > 0 || len(milestones) > 0 || len(literalAssigneeNames) > 0 || len(assignees) > 0 {
				return output.NewFailure("input_error", "project all only accepts keyword and assignee self search conditions", 0)
			}
			for _, name := range assigneeNames {
				if name != "self" {
					return output.NewFailure("input_error", "project all only accepts assignee self", 0)
				}
			}
			if strings.TrimSpace(keyword) == "" && len(assigneeNames) == 0 {
				return output.NewFailure("input_error", "project all requires a keyword or assignee self", 0)
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
		if !crossProject && len(statusNames) == 0 && len(statuses) == 0 {
			statusNames = settings.Local.Issue.List.Status
		}
		if statusAll {
			statusNames = nil
		}
		id := ""
		if !crossProject {
			id, err = projectForList(ctx, api, project, settings.Local.Project, *jsonMode, *noInput, stdin, stdout, checked, *key)
			if err != nil {
				return err
			}
		}
		if len(statusNames) > 0 {
			items, err := api.Statuses(ctx, id)
			if err != nil {
				return err
			}
			statusIDs, err = resolveNames(statusNames, items)
			if err != nil {
				return err
			}
		}
		if len(milestoneNames) > 0 {
			items, err := api.Versions(ctx, id)
			if err != nil {
				return err
			}
			milestoneIDs, err = resolveNames(milestoneNames, items)
			if err != nil {
				return err
			}
		}
		if len(assigneeNames) > 0 || len(literalAssigneeNames) > 0 {
			assigneeIDs, err = resolveAssignees(ctx, api, id, assigneeNames, literalAssigneeNames)
			if err != nil {
				return err
			}
		}
		filter := backlog.IssueFilter{StatusIDs: statusIDs, AssigneeIDs: assigneeIDs, MilestoneIDs: milestoneIDs, Keyword: keyword}
		if !crossProject {
			projectID, _ := strconv.ParseInt(id, 10, 64)
			filter.ProjectIDs = []int64{projectID}
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
	list.Flags().VarP(project, "project", "p", "Project key, positive integer ID, or all")
	list.Flags().Var(limit, "limit", "Page size (1-100)")
	list.Flags().Var(offset, "offset", "Starting offset (nonnegative int64)")
	list.Flags().StringArrayVarP(&statusNames, "status", "s", nil, "Exact status name (repeat for OR)")
	list.Flags().StringArrayVar(&statuses, "status-id", nil, "Status ID (repeat for OR)")
	list.Flags().StringArrayVarP(&assigneeNames, "assignee", "a", nil, "Exact assignee name or self (repeat for OR)")
	list.Flags().StringArrayVar(&literalAssigneeNames, "assignee-name", nil, "Exact assignee name, including literal self (repeat for OR)")
	list.Flags().StringArrayVar(&assignees, "assignee-id", nil, "Assignee ID (repeat for OR)")
	list.Flags().StringArrayVarP(&milestoneNames, "milestone", "m", nil, "Exact milestone name (repeat for OR)")
	list.Flags().StringArrayVar(&milestones, "milestone-id", nil, "Milestone ID (repeat for OR)")
	return list
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
