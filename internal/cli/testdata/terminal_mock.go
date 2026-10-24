// Run from the repository root: go run ./internal/cli/testdata/terminal_mock.go issue-type list
// This harness uses an isolated TLS mock and temporary configuration, never a real Backlog space.
package main

import (
	"backlog-cli/internal/cli"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"os/signal"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
)

func main() { os.Exit(run()) }

func run() int {
	rule := os.Getenv("BL_TERMINAL_MOCK_RULE")
	if rule == "" {
		rule = "markdown"
	}
	total := 3
	if value := os.Getenv("BL_TERMINAL_MOCK_ISSUES"); value != "" {
		parsed, err := strconv.Atoi(value)
		if err != nil || parsed < 0 {
			fmt.Fprintln(os.Stderr, "invalid mock issue count")
			return 2
		}
		total = parsed
	}
	server := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if os.Getenv("BL_TERMINAL_MOCK_TRACE") == "1" {
			fmt.Fprintf(os.Stderr, "MOCK %s offset=%s\n", r.URL.Path, r.URL.Query().Get("offset"))
		}
		issues := r.URL.Path == "/api/v2/issues" || r.URL.Path == "/api/v2/issues/count"
		if r.Header.Get("Backlog-API-Key") != "mock-only-secret" || (!issues && r.URL.RawQuery != "") {
			http.Error(w, "unexpected authentication or query", 400)
			return
		}
		switch {
		case strings.HasPrefix(r.URL.Path, "/api/v2/issues/") && r.URL.Path != "/api/v2/issues/count":
			issueKey := strings.TrimPrefix(r.URL.Path, "/api/v2/issues/")
			if os.Getenv("BL_TERMINAL_MOCK_FAIL_DETAIL") == issueKey {
				http.Error(w, "detail failed mock-only-secret\x1b]52;c;stolen\a", 500)
				return
			}
			description := "# 見出し\n一行目\n二行目\n\n[toc]\n\n## 詳細\n<details><summary>クリックして詳細を表示</summary>展開された本文</details>\n\n**強調** ~~削除~~\n- [x] 済\n\n[[APP-2]] / APP-3 / [[Home]]\n#rev(11) / #rev(app:abcdefg) / #attach(sample.zip:11) / #image(11)\n\n![画像alt](https://example.invalid/image.png)\n\n| |結合||\n|見出し１|見出し２|見出し３|\n|---|---|---|\n|セル1|長いセル||\n|セル2|中央|右|\n[サンプル]\n\n`#rev(11) &amp; &#27;`\n\n```text\n#rev(11) &amp; &#27;\n```\n\n日本語👩‍💻\x1b[31m赤\x1b[0m &#27;[2J\x1b]52;c;stolen\a mock-only-secret"
			if rule == "backlog" {
				description = backlogDescription
			}
			switch os.Getenv("BL_TERMINAL_MOCK_BODY") {
			case "short":
				description = "短い本文 日本語👩‍💻"
			case "styled-secret":
				description = "xxxxxxxx mock-only-**secret**"
				if rule == "backlog" {
					description = "xxxxxxxx mock-only-''secret''"
				}
			case "long":
				for i := range 100 {
					description += fmt.Sprintf("\n\n行%03d %s", i, strings.Repeat("長い日本語👩‍💻の本文 ", 12))
				}
			}
			_ = json.NewEncoder(w).Encode(map[string]any{"issueKey": issueKey, "projectId": 42, "summary": rule + "詳細表示 日本語👩‍💻\x1b]52;c;stolen\a mock-only-secret", "description": description})
		case r.URL.Path == "/api/v2/issues/count":
			_, _ = fmt.Fprintf(w, `{"count":%d}`, total)
		case r.URL.Path == "/api/v2/issues" && r.Method == http.MethodPost:
			if r.Header.Get("Content-Type") != "application/x-www-form-urlencoded" || r.URL.RawQuery != "" || r.ParseForm() != nil || r.PostForm.Get("projectId") == "" || r.PostForm.Get("issueTypeId") == "" || r.PostForm.Get("priorityId") == "" || strings.TrimSpace(r.PostForm.Get("summary")) == "" {
				http.Error(w, "unexpected create form", 400)
				return
			}
			w.Header().Set("Location", "/view/APP-4")
			w.WriteHeader(http.StatusCreated)
			_ = json.NewEncoder(w).Encode(map[string]any{"issueKey": "APP-4", "summary": r.PostForm.Get("summary"), "description": r.PostForm.Get("description"), "future": json.Number("12345678901234567890")})
		case r.URL.Path == "/api/v2/priorities":
			_, _ = fmt.Fprint(w, `[{"id":3,"name":"中"}]`)
		case r.URL.Path == "/api/v2/issues":
			limit, limitErr := strconv.ParseInt(r.URL.Query().Get("count"), 10, 64)
			offset, offsetErr := strconv.ParseInt(r.URL.Query().Get("offset"), 10, 64)
			if limitErr != nil || offsetErr != nil || limit < 1 || limit > 100 || offset < 0 {
				http.Error(w, "unexpected pagination", 400)
				return
			}
			if offset > 0 && os.Getenv("BL_TERMINAL_MOCK_FAIL_NEXT") == "1" {
				http.Error(w, "next failed mock-only-secret\x1b]52;c;stolen\a", 500)
				return
			}
			items := []map[string]any{}
			for i := offset; i < int64(total) && int64(len(items)) < limit; i++ {
				summary := "日本語👩‍💻\x1b[31m赤\x1b[0m\x07mock-only-secret"
				if i%5 == 0 {
					summary = strings.Repeat("長い日本語👩‍💻", 12) + summary
				}
				var assignee any
				if i%3 != 0 {
					assignee = map[string]string{"name": "担当者 日本語👩‍💻"}
				}
				items = append(items, map[string]any{"issueKey": fmt.Sprintf("APP-%d", i+1), "summary": summary, "status": map[string]string{"name": "処理中"}, "assignee": assignee, "future": json.Number("12345678901234567890")})
			}
			_ = json.NewEncoder(w).Encode(items)
		case r.URL.Path == "/api/v2/projects":
			projects := make([]map[string]any, 25)
			for i := range projects {
				projects[i] = map[string]any{"id": i + 1, "projectKey": fmt.Sprintf("APP_%02d", i+1), "name": strings.Repeat("長い日本語👩‍💻", 12) + "\x1b[31m赤\x1b[0m\x07mock-only-secret"}
			}
			_ = json.NewEncoder(w).Encode(projects)
		case r.URL.Path == "/api/v2/projects/APP" || r.URL.Path == "/api/v2/projects/42":
			_ = json.NewEncoder(w).Encode(map[string]any{"id": 42, "projectKey": "APP", "textFormattingRule": rule})
		case r.URL.Path == "/api/v2/users/myself":
			_, _ = fmt.Fprint(w, `{"id":10,"name":"本人"}`)
		case strings.HasPrefix(r.URL.Path, "/api/v2/projects/") && strings.HasSuffix(r.URL.Path, "/statuses"):
			_, _ = fmt.Fprint(w, `[{"id":1,"name":"未対応"},{"id":2,"name":"処理中"},{"id":3,"name":"完了"},{"id":4,"name":"123"},{"id":5,"name":"a,b"},{"id":6,"name":"all"},{"id":7,"name":"重複"},{"id":8,"name":"重複"}]`)
		case strings.HasPrefix(r.URL.Path, "/api/v2/projects/") && strings.HasSuffix(r.URL.Path, "/users"):
			_, _ = fmt.Fprint(w, `[{"id":10,"name":"本人"},{"id":11,"name":"self"},{"id":12,"name":"山田"},{"id":13,"name":"123"},{"id":14,"name":"a,b"},{"id":15,"name":"重複"},{"id":16,"name":"重複"}]`)
		case strings.HasPrefix(r.URL.Path, "/api/v2/projects/") && strings.HasSuffix(r.URL.Path, "/versions"):
			_, _ = fmt.Fprint(w, `[{"id":7,"name":"v1.0"},{"id":8,"name":"123"},{"id":9,"name":"a,b"},{"id":10,"name":"重複"},{"id":11,"name":"重複"}]`)
		case strings.HasPrefix(r.URL.Path, "/api/v2/projects/") && strings.HasSuffix(r.URL.Path, "/issueTypes"):
			project := strings.TrimSuffix(strings.TrimPrefix(r.URL.Path, "/api/v2/projects/"), "/issueTypes")
			_ = json.NewEncoder(w).Encode([]map[string]any{{"id": 7, "name": "タスク👩‍💻 (project " + project + ")", "future": json.Number("12345678901234567890")}})
		default:
			http.Error(w, "mock endpoint not found", 404)
		}
	}))
	defer server.Close()
	base, err := os.MkdirTemp("", "bl-terminal-mock-*")
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	defer os.RemoveAll(base)
	if err := os.Mkdir(filepath.Join(base, "backlog"), 0o700); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	config, err := json.Marshal(map[string]any{"profiles": map[string]any{"default": map[string]string{"spaceUrl": server.URL, "apiKeyEnv": "BL_TERMINAL_MOCK_KEY"}}})
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.WriteFile(filepath.Join(base, "backlog", "config.json"), config, 0o600); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.Setenv("XDG_CONFIG_HOME", base); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.Setenv("BL_TERMINAL_MOCK_KEY", "mock-only-secret"); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	if err := os.Chdir(base); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	return cli.ExecuteWithInput(ctx, os.Args[1:], os.Stdin, os.Stdout, os.Stderr, server.Client())
}

const backlogDescription = `* 見出し
一行目
二行目

#contents
[toc]

** 詳細
<details><summary>クリックして詳細を表示</summary>本文</details>
&color(red) { 色の本文 }
<unknown>未知の本文</unknown>

- リスト
-- 入れ子
+ 番号
- [x] 済
''太字'' '''斜体''' %%打消し%%
> 行引用
{quote}
ブロック引用
{/quote}

[[BLG-104]] / BLG-87 / [[Home]]
[[Backlog>https://backlog.com/ja/]]
#rev(11) / #rev(app:abcdefg) / #attach(sample.zip:11) / #image(11) / #thumbnail(11)
<img alt="代替テキスト" src="https://example.test/image.png">
#foo(残す内容)

| |結合||
|ヘッダー１|ヘッダー２|ヘッダー３|h
|セル1|長いセル||
|~セル2|中央寄せ|右寄せ|
|escaped \| pipe|{code}code|pipe{/code}|追加セル|
[サンプル]

{code:java}
#rev(11) &amp; &#27; [[BLG-104]]
|code|table|h
* コード内見出し
{/code}
\#rev(11)
\[[BLG-104]]

日本語👩‍💻` + "\x1b[31m赤\x1b[0m &#27;[2J\x1b]52;c;stolen\a mock-only-secret"
