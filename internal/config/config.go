package config

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

type Profile struct {
	SpaceURL  string `json:"spaceUrl"`
	APIKeyEnv string `json:"apiKeyEnv"`
}

type Local struct {
	Project string `json:"project"`
	Issue   struct {
		Create struct {
			IssueType string `json:"issueType"`
			Priority  string `json:"priority"`
			Assignee  string `json:"assignee"`
		} `json:"create"`
		List struct {
			Status []string `json:"status"`
		} `json:"list"`
	} `json:"issue"`
}

type Settings struct {
	SpaceURL string
	APIKey   string
	Editor   []string
	Local    Local
}

// Origin validates an HTTPS origin without limiting it to public Backlog hosts.
func Origin(value string) (string, error) {
	u, err := url.Parse(value)
	if err != nil || u.Scheme != "https" || u.Hostname() == "" || u.User != nil || u.RawQuery != "" || u.ForceQuery || u.Fragment != "" || strings.Contains(value, "#") || (u.Path != "" && u.Path != "/") || u.RawPath != "" || u.Opaque != "" {
		return "", errors.New("invalid spaceUrl: HTTPS origin required")
	}
	u.Path = ""
	return u.String(), nil
}

// ProjectID validates a project value, returning its normalized ID or an empty string for a key.
func ProjectID(value string) (string, error) {
	if strings.TrimSpace(value) == "" || value == "all" {
		return "", errors.New("project must be a key or positive integer ID (not all)")
	}
	// Signed integer spellings are IDs too, never keys on overflow or nonpositive input.
	digits := strings.TrimPrefix(strings.TrimPrefix(value, "+"), "-")
	if digits != "" && strings.Trim(digits, "0123456789") == "" {
		id, err := strconv.ParseInt(value, 10, 64)
		if err != nil || id <= 0 {
			return "", errors.New("project ID must be a positive int64")
		}
		return strconv.FormatInt(id, 10), nil
	}
	return "", nil
}

// Load reads only the selected profile and the working directory's local defaults.
func Load(profile, cwd string) (Settings, error) {
	var result Settings
	base := os.Getenv("XDG_CONFIG_HOME")
	if !filepath.IsAbs(base) {
		home, err := os.UserHomeDir()
		if err != nil {
			return result, errors.New("common configuration home is unavailable")
		}
		base = filepath.Join(home, ".config")
	}
	var common struct {
		Profiles map[string]Profile `json:"profiles"`
		Editor   []string           `json:"editor"`
	}
	if _, err := read(filepath.Join(base, "backlog", "config.json"), "common", &common, false); err != nil {
		return result, err
	}
	result.Editor = common.Editor
	if result.Editor == nil {
		result.Editor = []string{"vim"}
	}
	if len(result.Editor) == 0 || strings.TrimSpace(result.Editor[0]) == "" {
		return result, errors.New("invalid editor: executable required")
	}
	for _, arg := range result.Editor {
		if strings.ContainsRune(arg, 0) {
			return result, errors.New("invalid editor: NUL is not executable")
		}
	}
	for _, p := range common.Profiles {
		if _, err := Origin(p.SpaceURL); err != nil {
			return result, err
		}
		if p.APIKeyEnv == "" || strings.ContainsAny(p.APIKeyEnv, "=\x00") {
			return result, errors.New("invalid apiKeyEnv")
		}
	}
	selected, exists := common.Profiles[profile]
	if !exists {
		return result, errors.New("selected profile is missing")
	}
	result.SpaceURL, _ = Origin(selected.SpaceURL)
	result.APIKey = os.Getenv(selected.APIKeyEnv)
	if result.APIKey == "" {
		return result, errors.New("API key environment variable is unset or empty")
	}
	// HTTP header values cannot contain controls; never include the value in an error.
	for _, r := range result.APIKey {
		if r < 32 && r != '\t' || r == 127 {
			return result, errors.New("invalid API key header value")
		}
	}
	if _, err := read(filepath.Join(cwd, ".backlog", "settings.json"), "local", &result.Local, true); err != nil {
		return result, err
	}
	if result.Local.Project != "" {
		if _, err := ProjectID(result.Local.Project); err != nil {
			return result, fmt.Errorf("local configuration: %w", err)
		}
	}
	return result, nil
}

func read(path, scope string, dest any, optional bool) (bool, error) {
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		if optional {
			return false, nil
		}
		return false, fmt.Errorf("%s configuration is missing", scope)
	}
	if err != nil {
		return false, fmt.Errorf("%s configuration could not be read", scope)
	}
	var tree any
	if err := json.Unmarshal(data, &tree); err != nil {
		return false, fmt.Errorf("%s configuration: malformed JSON", scope)
	}
	if _, ok := tree.(map[string]any); !ok || hasNull(tree) {
		return false, fmt.Errorf("%s configuration: invalid setting type", scope)
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(dest); err != nil {
		var typeError *json.UnmarshalTypeError
		if errors.As(err, &typeError) {
			return false, fmt.Errorf("%s configuration: invalid setting type", scope)
		}
		return false, fmt.Errorf("%s configuration: unknown setting key", scope)
	}
	if scope == "local" && hasEmptyString(tree) {
		return false, errors.New("local configuration: invalid empty setting value")
	}
	return true, nil
}

func hasNull(value any) bool {
	switch v := value.(type) {
	case nil:
		return true
	case map[string]any:
		for _, child := range v {
			if hasNull(child) {
				return true
			}
		}
	case []any:
		for _, child := range v {
			if hasNull(child) {
				return true
			}
		}
	}
	return false
}

func hasEmptyString(value any) bool {
	switch v := value.(type) {
	case string:
		return strings.TrimSpace(v) == ""
	case map[string]any:
		for _, child := range v {
			if hasEmptyString(child) {
				return true
			}
		}
	case []any:
		for _, child := range v {
			if hasEmptyString(child) {
				return true
			}
		}
	}
	return false
}
