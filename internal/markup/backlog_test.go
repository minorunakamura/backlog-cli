package markup_test

import (
	"backlog-cli/internal/markup"
	"os"
	"regexp"
	"strings"
	"testing"
	"time"
)

func TestBacklogCodeEscapesAndEntities(t *testing.T) {
	source := "{code:java}\n#rev(11) [[APP-1]] <tag> &#27; &amp;\n~~~\n{/code}\n\\#rev(APP-1) \\[[APP-2]] \\<tag> \\&#27;\n&#27;[31m &amp;#x1b;[2J\n{code}\n未終端 ''原文''\n"
	converted := markup.Backlog(source, "https://space.test")
	if !strings.Contains(converted, "#rev(11) [[APP-1]] <tag> &#27; &amp;") || !strings.Contains(converted, `U\+001B`) {
		t.Fatalf("code/entities: %s", converted)
	}
	rendered, err := markup.RenderBacklog(source, "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"#rev(11) [[APP-1]] <tag> &#27; &amp;", "#rev(APP-1)", "[[APP-2]]", "<tag>", "&#27;", "U+001B", "未終端 ''原文''"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("lost %q: %s", want, rendered)
		}
	}
	if strings.Contains(rendered, "/view/APP-") {
		t.Errorf("escaped key converted: %s", rendered)
	}
}

func TestBacklogReferencesHTMLAndColor(t *testing.T) {
	source := "APP-1 [[APP-2]] [[Home]]\n[[https://backlog.com/ja/]] [[Backlog>https://backlog.com/ja/]] [[Backlog:https://backlog.com/ja/]]\n#rev(11) #rev(app:abcdefg) #attach(sample.zip:11) #image(11) #thumbnail(11)\n#image(https://example.test/a.png,画像alt)\n&color(red) { 赤 } &color(#ffffff, #8abe00) { 背景色 }\n<details><summary>詳細</summary>展開<unknown>本文</unknown></details><img alt=\"代替\" src=\"https://example.test/b.png\">\n#foo(outer #bar(inner) <unknown>内容</unknown> tail) &br; END\n"
	converted := markup.Backlog(source, "https://space.test")
	for _, want := range []string{"[APP-1](https://space.test/view/APP-1)", "[APP-2](https://space.test/view/APP-2)", "Wiki: Home", "[Backlog](https://backlog.com/ja/)", "Subversionリビジョン: r11", "Gitリビジョン: app@abcdefg", "添付ファイル: sample.zip（ID: 11）", "画像: ID: 11", "画像alt", "https://example.test/a.png", "詳細", "展開", "本文", "代替", "https://example.test/b.png", "赤", "背景色", "未対応の記法（名前: foo、内容: outer", "inner", "tail", "END"} {
		if !strings.Contains(converted, want) {
			t.Errorf("missing %q: %s", want, converted)
		}
	}
	for _, bad := range []string{"<details>", "<summary>", "<unknown>", "&color", "&br;", "#foo(", "#bar("} {
		if strings.Contains(converted, bad) {
			t.Errorf("unconverted %q: %s", bad, converted)
		}
	}
	rendered, err := markup.RenderBacklog(source, "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(rendered, "<unknown>") || strings.Contains(rendered, "#bar(") {
		t.Errorf("nested notation leaked: %s", rendered)
	}
	rendered = strings.Join(strings.Fields(rendered), " ")
	for _, want := range []string{"赤", "背景色", "詳細", "本文", "Wiki: Home", "https://backlog.com/ja/", "outer", "inner", "tail", "内容"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("lost %q: %s", want, rendered)
		}
	}
}

func TestBacklogOfficialFixtureTablesAndContents(t *testing.T) {
	source, err := os.ReadFile("testdata/backlog.txt")
	if err != nil {
		t.Fatal(err)
	}
	converted := markup.Backlog(string(source), "https://space.test")
	for _, want := range []string{"目次", "- 見出し", "- 詳細", "|**セル2**|", "|**ヘッダー１**|**ヘッダー２**|**ヘッダー３**|", "|---|---|---|"} {
		if !strings.Contains(converted, want) {
			t.Errorf("missing %q: %s", want, converted)
		}
	}
	if strings.Count(converted, "目次") != 2 || strings.Contains(converted, "- コード内見出し") {
		t.Errorf("incorrect contents: %s", converted)
	}
	rendered, err := markup.RenderBacklog(string(source), "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"結合", "ヘッダー１", "ヘッダー２", "ヘッダー３", "セル1", "長いセル", "セル2", "中央寄せ", "右寄せ", "追加セル", "サンプル", "escaped | pipe", "code|pipe", "#rev(11) &amp; &#27; [[BLG-104]]", "|code|table|h", "* コード内見出し"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("lost %q: %s", want, rendered)
		}
	}
}

func TestBacklogNestedFormattingAndMarkdownOnlySyntax(t *testing.T) {
	source := "''太字<unknown>本文</unknown> &color(red) { 赤 }''\n~~literal~~\n1. not a list\n\n    [label](url)\n\n{code}&#27; &amp;{/code}\n"
	rendered, err := markup.RenderBacklog(source, "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"太字本文", "赤", "~~literal~~", "1. not a list", "[label](url)", "&#27; &amp;"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("lost %q: %s", want, rendered)
		}
	}
	if strings.Contains(rendered, "<unknown>") || strings.Contains(rendered, "&color") || strings.Contains(rendered, `\[label\]`) {
		t.Errorf("notation leaked: %s", rendered)
	}
}

func TestBacklogRetainsTableSuffixesInlineLanguageCodeAndMarkdownSetext(t *testing.T) {
	source := "|A|B|h\n|left|escaped \\|h\n|表caption|c\n\n{code:java}&#27; | <tag>{/code}\n後続\n\nplain\n===\n\n[[括弧>https://example.test/a(b)?q=x&n=1]]\n"
	rendered, err := markup.RenderBacklog(source, "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"escaped |h", "表caption", "&#27; | <tag>", "後続", "plain", "===", "https://example.test/a%28b%29?q=x&n=1"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("lost %q: %s", want, rendered)
		}
	}
}

func TestBacklogSafetyAndMalformedInputs(t *testing.T) {
	for _, source := range []string{
		"日本語👩‍💻\r\n\t&#27;[31m赤&#x1b;]52;c;stolen&#7;終", "&amp;#27;[2J &#38;#x1b;[31m", "* &#27;[31m\n\n[toc]", "[[URL>https://example.test/&#27;[31m]]", "{code}&#27; &amp;{/code}", "{code}\n&#27;[31m &amp; &#7;\n{/code}", "\x1b[31m日本語👩‍💻\x1b[0m\x1b]52;c;stolen\a終", "\x9b2J\u009dtitle\u009c本文", "前\x1bPunterminated", "\\&#27;[31m", "&#0; &#127; &#x85; &NewLine;", "''<unknown>本文</unknown>''", "#foo(outer #bar(inner) tail)", "[[broken", "{code:broken", "<unknown title=\">\">日本語👩‍💻<nested>本文</unknown>",
	} {
		t.Run(source, func(t *testing.T) {
			for _, tty := range []bool{false, true} {
				rendered, err := markup.RenderBacklog(source, "https://space.test", tty)
				if err != nil {
					t.Fatal(err)
				}
				plain := regexp.MustCompile(`\x1b\[[0-9;:]*m`).ReplaceAllString(rendered, "")
				for _, r := range plain {
					if r != '\n' && r != '\t' && (r < 32 || r >= 127 && r <= 159) {
						t.Fatalf("unsafe rune %U: %q", r, rendered)
					}
				}
				if !tty && strings.ContainsRune(rendered, '\x1b') {
					t.Fatalf("ANSI on pipe: %q", rendered)
				}
				for _, want := range []string{"日本語👩‍💻", "本文", "outer", "inner", "tail", "broken"} {
					if strings.Contains(source, want) && !strings.Contains(plain, want) {
						t.Errorf("lost %q: %s", want, rendered)
					}
				}
			}
		})
	}
	for _, source := range []string{strings.Repeat("#foo(", 20000), strings.Repeat("\\[[", 20000), strings.Repeat("<", 100000), strings.Repeat("{code", 20000), strings.Repeat("{code}", 20000), strings.Repeat("&x", 30000), strings.Repeat("<!--", 20000), strings.Repeat("\\&x", 20000), "[[" + strings.Repeat("x:", 20000) + "]]"} {
		started := time.Now()
		converted := markup.Backlog(source+"END", "https://space.test")
		if !strings.Contains(converted, "END") {
			t.Fatal("lost long input suffix")
		}
		if time.Since(started) > 5*time.Second {
			t.Errorf("slow malformed input prefix %q", source[:5])
		}
	}
}

func TestBacklogBasicFormattingAndMarkdownLiterals(t *testing.T) {
	source := "* 見出し\n** 小見出し\n*** 詳細\n- 項目\n-- 子\n--- 孫\n+ 番号\n- [ ] 未完\n- [x] 済\n''太字'' '''斜体''' %%取消%%\n> 引用\n{quote}\n引用本文\n{/quote}\nMarkdown: **literal** _under_ `tick` [label](url) ![alt](url)\n"
	converted := markup.Backlog(source, "https://space.test")
	for _, want := range []string{"# 見出し", "## 小見出し", "### 詳細", "- 項目", "  - 子", "    - 孫", "1. 番号", "- [ ] 未完", "- [x] 済", "**太字**", "*斜体*", "~~取消~~", "> 引用", "> 引用本文"} {
		if !strings.Contains(converted, want) {
			t.Errorf("missing %q: %s", want, converted)
		}
	}
	rendered, err := markup.RenderBacklog(source, "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"太字", "斜体", "取消", "**literal** _under_ `tick`", "[label](url)", "![alt](url)"} {
		if !strings.Contains(rendered, want) {
			t.Errorf("lost %q: %s", want, rendered)
		}
	}
}

func TestBacklogEmphasisSkipsCodeAndEscapedDelimiters(t *testing.T) {
	for _, tc := range []struct{ source, converted, text string }{
		{"''before {code}'' &amp;{/code} after''", "**before ` '' &amp;amp; ` after**", "before '' &amp; after"},
		{"''before \\''escaped\\'' after''", "**before ''escaped'' after**", "before ''escaped'' after"},
	} {
		t.Run(tc.source, func(t *testing.T) {
			if got := markup.Backlog(tc.source, "https://space.test"); got != tc.converted {
				t.Errorf("converted = %q, want %q", got, tc.converted)
			}
			got, err := markup.RenderBacklog(tc.source, "https://space.test", false)
			if err != nil {
				t.Fatal(err)
			}
			if strings.Join(strings.Fields(got), " ") != tc.text {
				t.Errorf("rendered = %q, want %q", got, tc.text)
			}
		})
	}
}

func TestBacklogHTMLAcrossLinesAndInReferences(t *testing.T) {
	for _, tc := range []struct{ source, text string }{
		{"<unknown\n title=\"x\">本文</unknown>", "本文"},
		{"#attach(<unknown>sample.zip</unknown>:11)", "添付ファイル: sample.zip（ID: 11）"},
		{"[[<unknown>label</unknown>>https://example.test/]]", "label https://example.test/"},
		{"#attach(<b>sample.zip</b>:11)", "添付ファイル: sample.zip（ID: 11）"},
		{"\\<unknown\n title=\"x\">本文\\</unknown>", "<unknown title=\"x\">本文</unknown>"},
		{"#attach({code}<unknown>raw</unknown>{/code}:11)", "添付ファイル: {code}<unknown>raw</unknown>{/code}（ID: 11）"},
		{"#attach(\\<unknown>raw\\</unknown>:11)", "添付ファイル: \\<unknown>raw\\</unknown>（ID: 11）"},
	} {
		t.Run(tc.source, func(t *testing.T) {
			got, err := markup.RenderBacklog(tc.source, "https://space.test", false)
			if err != nil {
				t.Fatal(err)
			}
			if strings.Join(strings.Fields(got), " ") != tc.text {
				t.Errorf("rendered = %q, want %q", got, tc.text)
			}
		})
	}
}

func TestBacklogAutolinksSurviveHTMLStripping(t *testing.T) {
	for _, tc := range []struct{ form, text string }{
		{"https://example.test/path", "https://example.test/path"},
		{"HTTPS://example.test/path", "HTTPS://example.test/path"},
		{"user@example.test", "user@example.test"},
		{"mailto:user@example.test", "user@example.test"},
	} {
		t.Run(tc.form, func(t *testing.T) {
			angle := "<" + tc.form + ">"
			source := "before " + angle + " <unknown>kept</unknown> after\n" +
				"#color(red,<unknown>" + angle + "</unknown>)\n" +
				"[[<unknown>" + angle + "</unknown>]]\n" +
				"{code}" + angle + " <unknown>&amp;</unknown>{/code}\n" +
				"\\" + angle + " \\<unknown>&amp;\\</unknown>"
			converted := markup.Backlog(source, "https://space.test")
			if !strings.HasPrefix(converted, "before "+angle+" kept after\n") || strings.Count(converted, tc.text) != 5 {
				t.Errorf("autolink lost or unknown tag retained: %q", converted)
			}
			got, err := markup.RenderBacklog(source, "https://space.test", false)
			if err != nil {
				t.Fatal(err)
			}
			if strings.Count(got, tc.text) < 5 || strings.Count(got, angle) != 4 || !strings.Contains(got, "kept after") {
				t.Errorf("autolink or surrounding text lost: %q", got)
			}
			for _, want := range []string{"Wiki: " + angle, angle + " <unknown>&amp;</unknown>"} {
				if !strings.Contains(got, want) {
					t.Errorf("literal reference/code text lost %q: %q", want, got)
				}
			}
			if strings.Count(got, "<unknown>") != 2 || strings.Count(got, "</unknown>") != 2 {
				t.Errorf("unknown tags not stripped outside code/escapes: %q", got)
			}
		})
	}
}

func TestBacklogTableCellBreaksRetainRowsAndColumns(t *testing.T) {
	for _, br := range []string{"&br;", "<br>"} {
		t.Run(br, func(t *testing.T) {
			source := "|H1|H2|h\n|left" + br + "retained|right|\n|END1|END2|\n[caption]"
			const want = "|**H1**|**H2**|\n|---|---|\n|left retained|right|\n|END1|END2|"
			gotTable, _, _ := strings.Cut(markup.Backlog(source, "https://space.test"), "\n\n")
			if gotTable != want {
				t.Errorf("converted table = %q, want %q", gotTable, want)
			}
			got, err := markup.RenderBacklog(source, "https://space.test", false)
			if err != nil {
				t.Fatal(err)
			}
			var rows [][]string
			for _, line := range strings.Split(got, "\n") {
				if strings.Contains(line, "│") {
					cells := strings.Split(strings.TrimSpace(line), "│")
					for i := range cells {
						cells[i] = strings.TrimSpace(cells[i])
					}
					rows = append(rows, cells)
				}
			}
			if len(rows) != 3 || strings.Join(rows[0], "/") != "H1/H2" || strings.Join(rows[1], "/") != "left retained/right" || strings.Join(rows[2], "/") != "END1/END2" || !strings.Contains(got, "[caption]") {
				t.Errorf("table structure lost: rows=%q rendered=%q", rows, got)
			}
		})
	}
}

func TestBacklogMultilineCodeAfterTextProtectsBodyAndResumes(t *testing.T) {
	for _, source := range []string{
		"prefix {code}\n#rev(11) <unknown>raw</unknown> &amp;\n{/code}\nafter #rev(22)",
		"prefix {code}\n#rev(11) <unknown>raw</unknown> &amp;",
		"{code}<unknown\n title=\"x\">raw</unknown>{/code}\nafter #rev(22)",
	} {
		t.Run(source, func(t *testing.T) {
			converted := markup.Backlog(source, "https://space.test")
			got, err := markup.RenderBacklog(source, "https://space.test", false)
			if err != nil {
				t.Fatal(err)
			}
			for _, want := range []string{"#rev(11) <unknown>raw</unknown> &amp;", "<unknown\n title=\"x\">raw</unknown>"} {
				if strings.Contains(source, want) && (!strings.Contains(converted, want) || !strings.Contains(strings.Join(strings.Fields(got), " "), strings.Join(strings.Fields(want), " "))) {
					t.Errorf("code changed: converted=%q rendered=%q", converted, got)
				}
			}
			if strings.Contains(source, "prefix") && !strings.Contains(got, "prefix") {
				t.Errorf("prefix lost: %q", got)
			}
			if strings.Contains(source, "after") && !strings.Contains(got, "after Subversionリビジョン: r22") {
				t.Errorf("did not resume conversion: %q", got)
			}
		})
	}
}

func TestBacklogContentsSkipsMultilineCodeAfterText(t *testing.T) {
	got := markup.Backlog("#contents\nprefix {code}\n* hidden\n{/code}\n* visible", "https://space.test")
	if strings.Count(got, "hidden") != 1 || !strings.Contains(got, "- visible") || !strings.Contains(got, "# visible") {
		t.Fatalf("contents includes code heading or lost heading: %q", got)
	}
}

func TestBacklogCodeLanguageCannotExtendFence(t *testing.T) {
	for _, lang := range []string{"~~~", "java"} {
		t.Run(lang, func(t *testing.T) {
			source := "{code:" + lang + "}\n#rev(11)\n{/code}\n#rev(22)"
			converted := markup.Backlog(source, "https://space.test")
			lines := strings.Split(strings.TrimSpace(converted), "\n")
			if lines[0] != "~~~ "+lang || !strings.Contains(converted, "\n~~~\n\nSubversionリビジョン: r22") {
				t.Errorf("fence/info or following body incorrect: %q", converted)
			}
			got, err := markup.RenderBacklog(source, "https://space.test", false)
			if err != nil {
				t.Fatal(err)
			}
			if !strings.Contains(got, "#rev(11)") || !strings.Contains(got, "Subversionリビジョン: r22") || strings.Contains(got, "#rev(22)") {
				t.Errorf("code swallowed following body: %q", got)
			}
		})
	}
}

func TestBacklogRepeatedOrderedMarkersRetainNesting(t *testing.T) {
	const source = "+ parent\n++ child\n+++ grandchild\n+ sibling"
	const want = "1. parent\n   1. child\n      1. grandchild\n1. sibling"
	if got := markup.Backlog(source, "https://space.test"); got != want {
		t.Errorf("converted = %q, want %q", got, want)
	}
	got, err := markup.RenderBacklog(source, "https://space.test", false)
	if err != nil {
		t.Fatal(err)
	}
	var rows []string
	for _, line := range strings.Split(got, "\n") {
		if strings.TrimSpace(line) != "" {
			rows = append(rows, strings.TrimRight(line, " "))
		}
	}
	if len(rows) != 4 || !strings.HasSuffix(rows[0], "1. parent") || !strings.HasSuffix(rows[1], "1. child") || !strings.HasSuffix(rows[2], "1. grandchild") || !strings.HasSuffix(rows[3], "2. sibling") {
		t.Fatalf("list nesting lost: %q", got)
	}
	if strings.Index(rows[1], "1.") <= strings.Index(rows[0], "1.") || strings.Index(rows[2], "1.") <= strings.Index(rows[1], "1.") {
		t.Fatalf("list flattened: %q", got)
	}
}
