package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/config"
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"unicode/utf8"

	"github.com/spf13/cobra"
)

func issueCreateCommand(ctx context.Context, profile *singleValue, jsonMode *bool, key *string, stdin io.Reader, checked *checkedWriter, client *http.Client, creation *output.Creation) *cobra.Command {
	project, issueType, issueTypeID, priority, priorityID, title := &singleValue{}, &singleValue{}, &singleValue{}, &singleValue{}, &singleValue{}, &singleValue{}
	assignee, assigneeName, assigneeID := &singleValue{}, &singleValue{}, &singleValue{}
	body, bodyFile := &singleValue{}, &singleValue{}
	var noAssignee bool
	command := &cobra.Command{Use: "create", Short: "Create an issue without interactive input", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
		if ctx.Err() != nil {
			return output.NewFailure("cancelled", "command cancelled", 0)
		}
		if issueType.seen && issueTypeID.seen || priority.seen && priorityID.seen {
			return output.NewFailure("input_error", "names and IDs cannot be combined", 0)
		}
		if body.seen && bodyFile.seen {
			return output.NewFailure("input_error", "body and body-file are mutually exclusive", 0)
		}
		assigneeForms := 0
		for _, value := range []*singleValue{assignee, assigneeName, assigneeID} {
			if value.seen {
				assigneeForms++
			}
		}
		if assigneeForms > 1 || noAssignee && assigneeForms > 0 {
			return output.NewFailure("input_error", "assignee flags and no-assignee are mutually exclusive", 0)
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
		if !project.seen {
			project.value = settings.Local.Project
		}
		if !issueType.seen && !issueTypeID.seen {
			issueType.value = settings.Local.Issue.Create.IssueType
		}
		if !priority.seen && !priorityID.seen {
			priority.value = settings.Local.Issue.Create.Priority
		}
		if assigneeForms == 0 && !noAssignee {
			assignee.value = settings.Local.Issue.Create.Assignee
		}
		if strings.TrimSpace(title.value) == "" || project.value == "" || issueType.value == "" && !issueTypeID.seen || priority.value == "" && !priorityID.seen {
			return output.NewFailure("input_error", "project, issue type, priority and nonblank title are required; interactive create is a later issue", 0)
		}
		for _, value := range []*singleValue{issueTypeID, priorityID, assigneeID} {
			if value.seen {
				id, err := integerValue(value.value, 1, 1<<63-1, "creation ID")
				if err != nil {
					return err
				}
				value.value = strconv.FormatInt(id, 10)
			}
		}
		for _, value := range []*singleValue{assignee, assigneeName} {
			if value.seen && value.value == "" {
				return output.NewFailure("input_error", "assignee name must not be empty", 0)
			}
		}
		if bodyFile.seen {
			var data []byte
			if bodyFile.value == "-" {
				type bodyReadResult struct {
					data []byte
					err  error
				}
				read := make(chan bodyReadResult, 1)
				// shortcut: caller-owned readers may block until EOF after cancellation; callers must unblock them to reclaim the goroutine.
				go func() {
					data, err := io.ReadAll(stdin)
					read <- bodyReadResult{data, err}
				}()
				select {
				case <-ctx.Done():
					return output.NewFailure("cancelled", "command cancelled", 0)
				case result := <-read:
					data, err = result.data, result.err
				}
			} else {
				data, err = os.ReadFile(bodyFile.value)
			}
			if err != nil {
				return output.NewFailure("input_error", "could not read body-file", 0)
			}
			body.value = string(data)
		}
		if !utf8.ValidString(body.value) {
			return output.NewFailure("input_error", "body must be valid UTF-8", 0)
		}
		api, err := backlog.New(settings.SpaceURL, *key, client)
		if err != nil {
			return err
		}
		id, err := resolveProject(ctx, api, project.value)
		if err != nil {
			return err
		}
		resolveChoice := func(name, direct *singleValue, fetch func() ([]json.RawMessage, error)) (string, error) {
			if direct.seen {
				return direct.value, nil
			}
			items, err := fetch()
			if err != nil {
				return "", err
			}
			ids, err := resolveNames([]string{name.value}, items)
			if err != nil {
				return "", err
			}
			return strconv.FormatInt(ids[0], 10), nil
		}
		typeID, err := resolveChoice(issueType, issueTypeID, func() ([]json.RawMessage, error) { return api.IssueTypes(ctx, id) })
		if err != nil {
			return err
		}
		prioID, err := resolveChoice(priority, priorityID, func() ([]json.RawMessage, error) { return api.Priorities(ctx) })
		if err != nil {
			return err
		}
		form := url.Values{"projectId": {id}, "issueTypeId": {typeID}, "priorityId": {prioID}, "summary": {title.value}}
		if assigneeID.seen {
			form.Set("assigneeId", assigneeID.value)
		} else if assignee.value != "" || assigneeName.seen {
			var names, literalNames []string
			if assigneeName.seen {
				literalNames = []string{assigneeName.value}
			} else {
				names = []string{assignee.value}
			}
			ids, err := resolveAssignees(ctx, api, id, names, literalNames)
			if err != nil {
				return err
			}
			form.Set("assigneeId", strconv.FormatInt(ids[0], 10))
		}
		if body.seen || bodyFile.seen {
			form.Set("description", body.value)
		}
		data, err := api.CreateIssue(ctx, form, creation)
		if err != nil {
			return err
		}
		return output.WriteCreatedIssue(checked, data, settings.SpaceURL, *jsonMode, *key)
	}}
	command.Flags().VarP(project, "project", "p", "Project key or positive integer ID")
	command.Flags().Var(issueType, "issue-type", "Exact issue type name")
	command.Flags().Var(issueTypeID, "issue-type-id", "Issue type ID")
	command.Flags().Var(priority, "priority", "Exact priority name")
	command.Flags().Var(priorityID, "priority-id", "Priority ID")
	command.Flags().VarP(title, "title", "t", "Issue title")
	command.Flags().VarP(body, "body", "b", "Issue body (not trimmed)")
	command.Flags().Var(bodyFile, "body-file", "UTF-8 body file; - explicitly reads stdin")
	command.Flags().VarP(assignee, "assignee", "a", "Exact assignee name or self")
	command.Flags().Var(assigneeName, "assignee-name", "Exact assignee name, including literal self")
	command.Flags().Var(assigneeID, "assignee-id", "Assignee ID")
	command.Flags().BoolVar(&noAssignee, "no-assignee", false, "Clear the configured default assignee")
	return command
}
