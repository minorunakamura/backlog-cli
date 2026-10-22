package cli

import (
	"backlog-cli/internal/backlog"
	"backlog-cli/internal/config"
	"backlog-cli/internal/output"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"

	"github.com/spf13/cobra"
)

// Execute runs a command with caller-owned streams. A nil client uses verified system TLS.
func Execute(ctx context.Context, args []string, stdout, stderr io.Writer, client *http.Client) int {
	root := &cobra.Command{Use: "bl", Short: "Backlog CLI", SilenceErrors: true, SilenceUsage: true}
	checked := &checkedWriter{Writer: stdout}
	root.SetOut(checked)
	root.SetErr(stderr)
	root.SetArgs(args)
	root.CompletionOptions.DisableDefaultCmd = true
	profile := &singleProfile{value: "default"}
	var jsonMode, noInput bool
	var key string
	root.PersistentFlags().Var(profile, "profile", "Configuration profile")
	root.PersistentFlags().BoolVar(&jsonMode, "json", false, "Output JSON")
	root.PersistentFlags().BoolVar(&noInput, "no-input", false, "Disable interactive input")
	for _, name := range []string{"project", "priority"} {
		group := &cobra.Command{Use: name, Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
			return output.NewFailure("input_error", "subcommand required (see --help)", 0)
		}}
		group.AddCommand(&cobra.Command{Use: "list", Short: "List " + name, Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
			if ctx.Err() != nil {
				return output.NewFailure("cancelled", "command cancelled", 0)
			}
			cwd, err := os.Getwd()
			if err != nil {
				return output.NewFailure("config_error", "working directory is unavailable", 0)
			}
			settings, err := config.Load(profile.value, cwd)
			if err != nil {
				return output.NewFailure("config_error", err.Error(), 0)
			}
			key = settings.APIKey
			api, err := backlog.New(settings.SpaceURL, key, client)
			if err != nil {
				return err
			}
			var items []json.RawMessage
			if name == "project" {
				items, err = api.Projects(ctx)
			} else {
				items, err = api.Priorities(ctx)
			}
			if err != nil {
				return err
			}
			return output.WriteList(checked, items, name, jsonMode, key)
		}})
		root.AddCommand(group)
	}
	err := root.ExecuteContext(ctx)
	if err == nil && checked.err != nil {
		err = output.NewFailure("output_error", "could not write output", 0)
	}
	if err != nil {
		var failure *output.Failure
		if !errors.As(err, &failure) {
			failure = output.NewFailure("input_error", "invalid command, arguments or flags (see --help)", 0)
		}
		if errors.Is(ctx.Err(), context.Canceled) {
			failure.Code = "cancelled"
			failure.Message = "command cancelled"
		}
		if err := output.WriteFailure(stderr, failure, jsonRequested(args), key); err != nil && failure.Code != "cancelled" {
			return 1
		}
		return failure.ExitCode()
	}
	return 0
}

// Cobra's help renderer does not propagate writer errors.
type checkedWriter struct {
	io.Writer
	err error
}

func (w *checkedWriter) Write(data []byte) (int, error) {
	n, err := w.Writer.Write(data)
	if err == nil && n != len(data) {
		err = io.ErrShortWrite
	}
	if err != nil {
		w.err = err
	}
	return n, err
}

type singleProfile struct {
	value string
	seen  bool
}

func (v *singleProfile) String() string { return v.value }
func (v *singleProfile) Type() string   { return "string" }
func (v *singleProfile) Set(value string) error {
	if v.seen {
		return errors.New("profile specified more than once")
	}
	v.seen = true
	v.value = value
	return nil
}

// Parser failures can occur before Cobra reaches a later --json flag.
func jsonRequested(args []string) bool {
	enabled := false
	for i := 0; i < len(args); i++ {
		arg := args[i]
		if arg == "--" {
			break
		}
		if arg == "--profile" {
			i++
			continue
		}
		if arg == "--json" {
			enabled = true
		}
		if value, ok := strings.CutPrefix(arg, "--json="); ok {
			if parsed, err := strconv.ParseBool(value); err == nil {
				enabled = parsed
			}
		}
	}
	return enabled
}
