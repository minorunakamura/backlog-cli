package markup_test

import (
	"backlog-cli/internal/markup"
	"backlog-cli/internal/output"
	"os"
	"regexp"
	"strings"
	"testing"
)

func TestMarkdownChecklistDoesNotConsumeLaterImageOrExtensions(t *testing.T) {
	source := "- [x] 済\n\n[[APP-2]] / APP-3 / [[Home]]\n#rev(11)\n\n![画像alt](https://example.test/image.png)\n"
	converted := markup.Markdown(source, "https://space.test")
	for _, want := range []string{"- [x] 済", "[APP-2](https://space.test/view/APP-2)", "[APP-3](https://space.test/view/APP-3)", "Wiki: Home", "Subversionリビジョン: r11", "画像: 画像alt"} {
		if !strings.Contains(converted, want) {
			t.Errorf("missing %q: %s", want, converted)
		}
	}
}

func TestMarkdownNestedHTMLCodeReferencesAndEscapes(t *testing.T) {
	source := "<details>\n<summary>詳細</summary>\n`<tag> #rev(11) [[APP-1]]`\n\n```text\n<span> #rev(11) [[APP-1]]\n```\n</details>\n\n\\[[APP-1]] \\#rev(APP-1) \\<tag>\n\n<https://example.test/APP-1> [既存](https://example.test/APP-1)\n\nAPP-2. (APP-3) 日本語APP-4\n\n![参照画像][pic]\n\n[pic]: https://example.test/pic.png\n"
	converted := markup.Markdown(source, "https://space.test")
	for _, want := range []string{"`<tag> #rev(11) [[APP-1]]`", "<span> #rev(11) [[APP-1]]", "\\[[APP-1]]", "\\#rev(APP-1)", "\\<tag>", "<https://example.test/APP-1>", "[既存](https://example.test/APP-1)", "[APP-2](https://space.test/view/APP-2).", "([APP-3](https://space.test/view/APP-3))", "画像: 参照画像", "https://example.test/pic.png"} {
		if !strings.Contains(converted, want) {
			t.Errorf("missing %q in %s", want, converted)
		}
	}
}

func TestMarkdownTablesPreserveEscapedAndCodePipesAndOverflowCells(t *testing.T) {
	source := "|A|B|\n|---|---|\n|escaped \\| pipe|`code|pipe`|extra|\n[caption]\n\n```text\n|code|only|\n```\n"
	converted := markup.Markdown(source, "https://space.test")
	for _, want := range []string{"escaped \\| pipe", "`code\\|pipe`", "extra", "[caption]", "```text\n|code|only|\n```"} {
		if !strings.Contains(converted, want) {
			t.Errorf("lost %q: %s", want, converted)
		}
	}
	rendered, err := markup.Render(source, "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"escaped | pipe", "code|pipe", "extra", "caption", "|code|only|"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("lost %q: %s", want, rendered)
		}
	}
}

func TestMarkdownPreservesIndentedCodeTableLines(t *testing.T) {
	for _, source := range []string{
		"    |a|b|\n    |---|---|\n    |c|d|\n",
		"  ```\n  |code|only|\n  ```\n",
	} {
		t.Run(source, func(t *testing.T) {
			if converted := markup.Markdown(source, "https://space.test"); converted != source {
				t.Errorf("code changed: %q, want %q", converted, source)
			}
			rendered, err := markup.Render(source, "https://space.test", false)
			if err != nil {
				t.Fatal(err)
			}
			for _, line := range strings.Split(source, "\n") {
				line = strings.TrimSpace(line)
				if strings.HasPrefix(line, "|") && !strings.Contains(rendered, line) {
					t.Errorf("lost code line %q: %q", line, rendered)
				}
			}
			if strings.Count(rendered, "|---|---|") != strings.Count(source, "|---|---|") {
				t.Errorf("code delimiter changed: %q", rendered)
			}
		})
	}
}

func TestMarkdownMalformedContentIsRetained(t *testing.T) {
	for _, source := range []string{"#foo(outer #bar(inner) tail)", "#foo(unterminated", "[[broken", "<unknown>日本語👩‍💻<nested>本文</unknown>", "<unknown title=\">\">日本語👩‍💻本文</unknown>", strings.Repeat("#foo(", 20000) + "END", strings.Repeat("<", 100000) + "END"} {
		converted := markup.Markdown(source, "https://space.test")
		for _, want := range []string{"outer", "inner", "tail", "unterminated", "broken", "日本語👩‍💻", "本文", "END"} {
			if strings.Contains(source, want) && !strings.Contains(converted, want) {
				t.Fatalf("dropped %q", want)
			}
		}
	}
}

func TestMarkdownEntityAndTerminalSafety(t *testing.T) {
	for _, source := range []string{
		"日本語👩‍💻\r\n\t&#27;[31m赤&#x1b;]52;c;stolen&#7;終", "&amp;#27;[2J &#38;#x1b;[31m", "# &#27;[31m\n\n[toc]", "# &#x1b;]52;c;stolen&#7;\n\n#contents", "[URL](https://example.test/&#27;[31m)",
		"`&#27;[31m &amp; &#7;`", "```text\n&#27;[31m &amp; &#7;\n```", "\x1b[31m日本語👩‍💻\x1b[0m\x1b]52;c;stolen\a終", "\x9b2J\u009dtitle\u009c本文", "前\x1bPunterminated", "\\&#27;[31m", "&#0; &#127; &#x85; &NewLine;",
	} {
		t.Run(source, func(t *testing.T) {
			for _, tty := range []bool{false, true} {
				rendered, err := markup.Render(source, "https://example.test", tty)
				if err != nil {
					t.Fatal(err)
				}
				// Removing only SGR leaves no other control bytes or C1 runes.
				plain := regexp.MustCompile(`\x1b\[[0-9;:]*m`).ReplaceAllString(rendered, "")
				for _, r := range plain {
					if r != '\n' && r != '\t' && (r < 32 || r >= 127 && r <= 159) {
						t.Fatalf("unsafe rune %U: %q", r, rendered)
					}
				}
				if !tty && strings.ContainsRune(rendered, '\x1b') {
					t.Fatalf("ANSI on pipe: %q", rendered)
				}
			}
			converted := markup.Markdown(source, "https://example.test")
			if !strings.HasPrefix(source, "`") && !strings.HasPrefix(source, "\\&") && strings.Contains(converted, "&#27;") {
				t.Fatalf("dangerous entity before renderer: %q", converted)
			}
		})
	}
	renderedEscape, err := markup.Render("\\&#27;[31m", "https://example.test", false)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(renderedEscape, "&#27;[31m") {
		t.Fatalf("escaped entity changed: %q", renderedEscape)
	}
	for _, source := range []string{"`&#27;[31m &amp; &#7;`", "```text\n&#27;[31m &amp; &#7;\n```"} {
		rendered, err := markup.Render(source, "https://example.test", false)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(rendered, "&#27;[31m &amp; &#7;") {
			t.Fatalf("code entities changed: %q", rendered)
		}
	}
}

func TestMarkdownPreservesStandardAutolinks(t *testing.T) {
	for _, address := range []string{"person@example.com", "HTTPS://example.test", "https://example.test", "mailto:person@example.com"} {
		t.Run(address, func(t *testing.T) {
			source := "<" + address + ">"
			if converted := markup.Markdown(source, "https://space.test"); converted != source {
				t.Errorf("autolink changed: %q, want %q", converted, source)
			}
			for _, tty := range []bool{false, true} {
				rendered, err := markup.Render(source, "https://space.test", tty)
				if err != nil {
					t.Fatal(err)
				}
				if !strings.Contains(output.SafeText(rendered), address) {
					t.Errorf("TTY=%v: autolink lost: %q", tty, rendered)
				}
			}
		})
	}
}

func TestMarkdownPreservesAutolinksAcrossHTMLAndCode(t *testing.T) {
	source := "前 <broken <https://example.test/APP-1> <unknown>本文</unknown>\n\n`<person@example.com>`\n\n```text\n<https://example.test/code>\n```\n\n\\<tag> #rev(11) <person@example.com> <HTTPS://example.test/end>\n"
	want := "前 <broken <https://example.test/APP-1> 本文\n\n`<person@example.com>`\n\n```text\n<https://example.test/code>\n```\n\n\\<tag> Subversionリビジョン: r11 <person@example.com> <HTTPS://example.test/end>\n"
	if converted := markup.Markdown(source, "https://space.test"); converted != want {
		t.Errorf("autolinks or code changed: %q, want %q", converted, want)
	}
	for _, tty := range []bool{false, true} {
		rendered, err := markup.Render(source, "https://space.test", tty)
		if err != nil {
			t.Fatal(err)
		}
		plain := output.SafeText(rendered)
		for _, text := range []string{"https://example.test/APP-1", "本文", "<person@example.com>", "<https://example.test/code>", "<tag>", "Subversionリビジョン: r11", "person@example.com", "HTTPS://example.test/end"} {
			if !strings.Contains(plain, text) {
				t.Errorf("TTY=%v: lost %q: %q", tty, text, rendered)
			}
		}
	}
}

func TestMarkdownLinkLabelPreservesInlineCodeEntities(t *testing.T) {
	for _, tc := range []struct{ source, want string }{
		{"[`&amp; &#27;[31m`](https://example.test)", "[`&amp;amp; &amp;#27;[31m`](https://example.test)"},
		{"[label `&amp; &#27;[31m`](https://example.test)", "[label `&amp;amp; &amp;#27;[31m`](https://example.test)"},
	} {
		t.Run(tc.source, func(t *testing.T) {
			if converted := markup.Markdown(tc.source, "https://space.test"); converted != tc.want {
				t.Errorf("inline code entities changed: %q, want %q", converted, tc.want)
			}
			for _, tty := range []bool{false, true} {
				rendered, err := markup.Render(tc.source, "https://space.test", tty)
				if err != nil {
					t.Fatal(err)
				}
				plain := output.SafeText(rendered)
				if !strings.Contains(plain, "&amp; &#27;[31m") || !strings.Contains(plain, "https://example.test") {
					t.Errorf("TTY=%v: code or URL lost: %q", tty, rendered)
				}
				if strings.Contains(rendered, "\x1b[31m") {
					t.Errorf("TTY=%v: external SGR revived: %q", tty, rendered)
				}
			}
		})
	}
}

func TestMarkdownOfficialExtensions(t *testing.T) {
	source, err := os.ReadFile("testdata/markdown.md")
	if err != nil {
		t.Fatal(err)
	}
	converted := markup.Markdown(string(source), "https://space.test")
	for _, want := range []string{"目次", "- 見出し", "- 詳細", "クリックして詳細を表示", "本文", "色の本文", "未知の本文", "[BLG-104](https://space.test/view/BLG-104)", "[BLG-87](https://space.test/view/BLG-87)", "Wiki: Home", "Subversionリビジョン: r11", "Gitリビジョン: app@abcdefg", "添付ファイル: sample.zip（ID: 11）", "画像: ID: 11", "画像: 代替テキスト", "https://example.test/image.png", "未対応の記法（名前: foo、内容: 残す内容）"} {
		if !strings.Contains(converted, want) {
			t.Errorf("missing %q in %s", want, converted)
		}
	}
	for _, bad := range []string{"<details>", "<summary>", "<span", "<unknown>", "#rev(", "#attach(", "#image(", "#foo(", "!["} {
		if strings.Contains(converted, bad) {
			t.Errorf("unconverted %q", bad)
		}
	}
	rendered, err := markup.Render(string(source), "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"結合", "ヘッダー１", "ヘッダー２", "ヘッダー３", "セル1", "長いセル", "セル2", "中央寄せ", "右寄せ", "サンプル"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("dropped %q: %s", want, rendered)
		}
	}
}

func TestMarkdownPreservesCodeEscapesAndOrdinaryNewlines(t *testing.T) {
	source := "# 見出し\n一行目\n二行目\n\n**強調** ~~削除~~\n- [x] 済\n\n`#rev(11) &amp; &#7;`\n\n```text\n#rev(11) &amp; &#7; [[BLG-104]]\n```\n\n    #rev(11)\n\n\\#rev(11)\n"
	converted := markup.Markdown(source, "https://example.test")
	for _, want := range []string{"# 見出し", "一行目\n二行目", "**強調** ~~削除~~", "- [x] 済", "`#rev(11) &amp;amp; &amp;#7;`", "#rev(11) &amp; &#7; [[BLG-104]]", "    #rev(11)", "\\#rev(11)"} {
		if !strings.Contains(converted, want) {
			t.Errorf("missing %q in %q", want, converted)
		}
	}
	rendered, err := markup.Render(source, "https://example.test", false)
	if err != nil {
		t.Fatal(err)
	}
	lines := strings.Split(rendered, "\n")
	for i := range lines {
		lines[i] = strings.TrimSpace(lines[i])
	}
	rendered = strings.Join(lines, "\n")
	for _, want := range []string{"見出し", "一行目\n二行目", "#rev(11) &amp; &#7;", "[[BLG-104]]"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("missing %q in %q", want, rendered)
		}
	}
	if strings.ContainsAny(rendered, "\x1b\a") {
		t.Fatalf("unsafe: %q", rendered)
	}
}
