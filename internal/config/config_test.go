package config_test

import (
	"backlog-cli/internal/config"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestLoadSelectedProfileAndLocalDefaults(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("XDG_CONFIG_HOME", "relative-is-ignored")
	t.Setenv("TEST_BACKLOG_KEY", "test-key")
	dir := filepath.Join(home, ".config", "backlog")
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "config.json"), []byte(`{"profiles":{"default":{"spaceUrl":"https://example.test/","apiKeyEnv":"TEST_BACKLOG_KEY"}}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	cwd := t.TempDir()
	if err := os.Mkdir(filepath.Join(cwd, ".backlog"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(cwd, ".backlog", "settings.json"), []byte(`{"project":"APP","issue":{"create":{"issueType":"タスク","priority":"中","assignee":"self"},"list":{"status":[]}}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	got, err := config.Load("default", cwd)
	if err != nil {
		t.Fatal(err)
	}
	if got.SpaceURL != "https://example.test" || got.APIKey != "test-key" || len(got.Editor) != 1 || got.Editor[0] != "vim" || got.Local.Project != "APP" || len(got.Local.Issue.List.Status) != 0 {
		t.Fatalf("unexpected configuration: %#v", got)
	}
}

func TestLoadRejectsInvalidCommonConfiguration(t *testing.T) {
	for _, tc := range []struct{ name, data, want string }{
		{"missing profile", `{"profiles":{}}`, "profile is missing"},
		{"missing key", `{"profiles":{"default":{"spaceUrl":"https://example.test","apiKeyEnv":"UNSET_BACKLOG_TEST"}}}`, "API key environment variable is unset or empty"},
		{"unknown key", `{"profiles":{},"typo":true}`, "unknown setting key"},
		{"wrong type", `{"profiles":[]}`, "invalid setting type"},
		{"null", `{"profiles":null}`, "invalid setting type"},
		{"broken JSON", `{`, "malformed JSON"},
		{"second document", `{} {}`, "malformed JSON"},
		{"empty editor", `{"profiles":{},"editor":[]}`, "invalid editor"},
		{"empty executable", `{"profiles":{},"editor":[""]}`, "invalid editor"},
		{"NUL argument", `{"profiles":{},"editor":["code","\u0000"]}`, "invalid editor"},
		{"HTTP", `{"profiles":{"default":{"spaceUrl":"http://example.test","apiKeyEnv":"TEST_BACKLOG_KEY"}}}`, "invalid spaceUrl"},
		{"userinfo", `{"profiles":{"default":{"spaceUrl":"https://user@example.test","apiKeyEnv":"TEST_BACKLOG_KEY"}}}`, "invalid spaceUrl"},
		{"path", `{"profiles":{"default":{"spaceUrl":"https://example.test/api","apiKeyEnv":"TEST_BACKLOG_KEY"}}}`, "invalid spaceUrl"},
		{"query", `{"profiles":{"default":{"spaceUrl":"https://example.test?x=y","apiKeyEnv":"TEST_BACKLOG_KEY"}}}`, "invalid spaceUrl"},
		{"fragment", `{"profiles":{"default":{"spaceUrl":"https://example.test#x","apiKeyEnv":"TEST_BACKLOG_KEY"}}}`, "invalid spaceUrl"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			base := t.TempDir()
			t.Setenv("XDG_CONFIG_HOME", base)
			t.Setenv("TEST_BACKLOG_KEY", "key")
			t.Setenv("UNSET_BACKLOG_TEST", "")
			dir := filepath.Join(base, "backlog")
			if err := os.MkdirAll(dir, 0o700); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(filepath.Join(dir, "config.json"), []byte(tc.data), 0o600); err != nil {
				t.Fatal(err)
			}
			_, err := config.Load("default", t.TempDir())
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("error=%v; want %q", err, tc.want)
			}
		})
	}
}

func setupCommon(t *testing.T, common string) string {
	t.Helper()
	base := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", base)
	t.Setenv("TEST_BACKLOG_KEY", "test-key")
	dir := filepath.Join(base, "backlog")
	if err := os.Mkdir(dir, 0o700); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(dir, "config.json")
	if err := os.WriteFile(path, []byte(common), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestLoadLocalValidationAndNoParentSearch(t *testing.T) {
	for _, tc := range []struct{ name, data, want string }{
		{"missing", "", ""},
		{"empty object", `{}`, ""},
		{"unset status", `{"issue":{"list":{}}}`, ""},
		{"broken", `{`, "malformed JSON"},
		{"unknown", `{"profile":"work"}`, "unknown setting key"},
		{"nested unknown", `{"issue":{"create":{"status":"new"}}}`, "unknown setting key"},
		{"numeric project", `{"project":12}`, "invalid setting type"},
		{"null status", `{"issue":{"list":{"status":null}}}`, "invalid setting type"},
		{"string status", `{"issue":{"list":{"status":"new"}}}`, "invalid setting type"},
		{"empty project", `{"project":" "}`, "invalid empty setting value"},
		{"zero project ID", `{"project":"0"}`, "project ID must be a positive int64"},
		{"negative project ID", `{"project":"-1"}`, "project ID must be a positive int64"},
		{"overflow project ID", `{"project":"9223372036854775808"}`, "project ID must be a positive int64"},
		{"all project", `{"project":"all"}`, "project must be a key or positive integer ID"},
		{"maximum project ID", `{"project":"9223372036854775807"}`, ""},
		{"signed project ID", `{"project":"+42"}`, ""},
		{"project key without regex restriction", `{"project":"team/key-日本語"}`, ""},
		{"empty name", `{"issue":{"create":{"priority":""}}}`, "invalid empty setting value"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			setupCommon(t, `{"profiles":{"default":{"spaceUrl":"https://example.test","apiKeyEnv":"TEST_BACKLOG_KEY"}}}`)
			parent := t.TempDir()
			if err := os.Mkdir(filepath.Join(parent, ".backlog"), 0o700); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(filepath.Join(parent, ".backlog", "settings.json"), []byte(`broken parent`), 0o600); err != nil {
				t.Fatal(err)
			}
			cwd := filepath.Join(parent, "child")
			if err := os.MkdirAll(filepath.Join(cwd, ".backlog"), 0o700); err != nil {
				t.Fatal(err)
			}
			if tc.data != "" {
				if err := os.WriteFile(filepath.Join(cwd, ".backlog", "settings.json"), []byte(tc.data), 0o600); err != nil {
					t.Fatal(err)
				}
			}
			_, err := config.Load("default", cwd)
			if tc.want == "" {
				if err != nil {
					t.Fatal(err)
				}
			} else if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("error=%v want=%s", err, tc.want)
			}
		})
	}
}

func TestLoadDistinguishesMissingAndUnreadableFiles(t *testing.T) {
	for _, scope := range []string{"common", "local"} {
		t.Run(scope, func(t *testing.T) {
			common := setupCommon(t, `{"profiles":{"default":{"spaceUrl":"https://example.test","apiKeyEnv":"TEST_BACKLOG_KEY"}}}`)
			cwd := t.TempDir()
			path := common
			if scope == "local" {
				if err := os.Mkdir(filepath.Join(cwd, ".backlog"), 0o700); err != nil {
					t.Fatal(err)
				}
				path = filepath.Join(cwd, ".backlog", "settings.json")
			} else {
				if err := os.Remove(path); err != nil {
					t.Fatal(err)
				}
			}
			_, err := config.Load("default", cwd)
			if scope == "common" && (err == nil || !strings.Contains(err.Error(), "is missing")) {
				t.Fatalf("error=%v", err)
			}
			if scope == "local" && err != nil {
				t.Fatal(err)
			}
			if err := os.Mkdir(path, 0o700); err != nil {
				t.Fatal(err)
			}
			_, err = config.Load("default", cwd)
			if err == nil || !strings.Contains(err.Error(), "could not be read") {
				t.Fatalf("error=%v", err)
			}
		})
	}
}

func TestLoadExplicitProfileAndEditorIgnoreEnvironmentEditors(t *testing.T) {
	setupCommon(t, `{"profiles":{"work":{"spaceUrl":"https://private.example.test/","apiKeyEnv":"TEST_BACKLOG_KEY"}},"editor":["code","--wait",""]}`)
	t.Setenv("VISUAL", "should-not-run")
	t.Setenv("EDITOR", "should-not-run")
	got, err := config.Load("work", t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if got.SpaceURL != "https://private.example.test" || !reflect.DeepEqual(got.Editor, []string{"code", "--wait", ""}) {
		t.Fatalf("unexpected profile or editor: %#v", got)
	}
	if _, err := config.Load("default", t.TempDir()); err == nil {
		t.Fatal("fell back to another profile")
	}
}

func TestLoadXDGUnsetOrEmptyUsesHome(t *testing.T) {
	for _, unset := range []bool{true, false} {
		t.Run(fmt.Sprint(unset), func(t *testing.T) {
			home := t.TempDir()
			t.Setenv("HOME", home)
			t.Setenv("XDG_CONFIG_HOME", "")
			if unset {
				if err := os.Unsetenv("XDG_CONFIG_HOME"); err != nil {
					t.Fatal(err)
				}
			}
			_, err := config.Load("default", t.TempDir())
			if err == nil || !strings.Contains(err.Error(), "common configuration is missing") {
				t.Fatalf("error=%v", err)
			}
		})
	}
}
