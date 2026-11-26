import { describe, expect, it } from "vitest";
import { applyRegexScripts, compileFindRegex, substituteMacros, type RegexScript } from "./engine";
import { protectHtmlBlocks, renderRichHtml, sanitizeCss } from "./rich-html";
import vexaJson from "../../../../../crates/lettuce-context/tests/fixtures/vexa/vexa-v1.0-regex.json?raw";
import goldenJson from "../../../../../crates/lettuce-context/tests/fixtures/vexa/golden.json?raw";
import replyFull from "../../../../../crates/lettuce-context/tests/fixtures/vexa/reply-full.txt?raw";
import replyShort from "../../../../../crates/lettuce-context/tests/fixtures/vexa/reply-short.txt?raw";
import replyPartial from "../../../../../crates/lettuce-context/tests/fixtures/vexa/reply-streaming-partial.txt?raw";

const files: Record<string, string> = {
  "reply-full.txt": replyFull,
  "reply-short.txt": replyShort,
  "reply-streaming-partial.txt": replyPartial,
};
const read = (name: string) => {
  const text = files[name];
  if (text === undefined) throw new Error(`unknown fixture ${name}`);
  return text;
};

function fromSillyTavern(raw: Record<string, unknown>, index: number): RegexScript {
  const depth = (value: unknown) => (typeof value === "number" && Number.isFinite(value) ? value : null);
  return {
    id: String(raw.id ?? index),
    script_name: String(raw.scriptName ?? ""),
    find_regex: String(raw.findRegex ?? ""),
    replace_string: String(raw.replaceString ?? ""),
    trim_strings: (raw.trimStrings as string[] | undefined) ?? [],
    placement: (raw.placement as number[] | undefined) ?? [],
    disabled: Boolean(raw.disabled),
    markdown_only: Boolean(raw.markdownOnly),
    prompt_only: Boolean(raw.promptOnly),
    run_on_edit: Boolean(raw.runOnEdit),
    substitute_regex: Number(raw.substituteRegex ?? 0),
    min_depth: depth(raw.minDepth),
    max_depth: depth(raw.maxDepth),
  };
}

const vexa = (JSON.parse(vexaJson) as Record<string, unknown>[]).map(fromSillyTavern);
const golden = JSON.parse(goldenJson) as { file: string; target: "display" | "prompt" | "prompt-user"; depth: number; output: string }[];
const macros = { user: "Алекс", char: "Шарлотта", random: () => 0 };

describe("SillyTavern regex engine (TypeScript port)", () => {
  it("matches SillyTavern's output for every Vexa golden case", () => {
    expect(golden.length).toBe(18);
    for (const item of golden) {
      const user = item.target === "prompt-user";
      const out = applyRegexScripts(vexa, read(item.file), { placement: user ? 1 : 2, target: item.target === "prompt-user" ? "prompt" : item.target, depth: item.depth }, macros);
      if (out !== item.output) {
        let i = 0;
        while (out[i] === item.output[i]) i++;
        throw new Error(`${item.file} ${item.target} depth ${item.depth} differs at ${i}: got ${JSON.stringify(out.slice(i - 80, i + 120))} want ${JSON.stringify(item.output.slice(i - 80, i + 120))}`);
      }
    }
  });

  it("compiles every Vexa pattern", () => {
    expect(vexa.length).toBe(143);
    expect(vexa.filter((script) => compileFindRegex(script.find_regex) === null)).toEqual([]);
  });

  it("resolves nested macros innermost first and keeps unknown ones", () => {
    expect(substituteMacros("{{random::a::{{char}}}} {{unknown}}", { char: "X", random: () => 1 })).toBe("X {{unknown}}");
    expect(substituteMacros("a\n{{trim}}\nb", {})).toBe("ab");
  });
});

describe("rich message HTML", () => {
  it("keeps styled HTML, form controls and popovers but strips scripts and remote CSS", () => {
    const html = renderRichHtml(
      '<style>@import url(https://x.test/a.css); .a{background:url(https://x.test/t.png);color:red}</style>\n<div class="a" onclick="alert(1)"><details open><summary>S</summary>\n\n    indented stays html\n</details><input type="radio" name="r" checked><button popovertarget="p">P</button><div id="p" popover>Pop</div></div>\n<script>alert(1)</script><a href="javascript:alert(1)">x</a>',
    );
    expect(html).toContain("<style>");
    expect(html).not.toContain("@import");
    expect(html).not.toContain("https://x.test/t.png");
    expect(html).toContain('class="a"');
    expect(html).toContain("popovertarget");
    expect(html).toContain('type="radio"');
    expect(html).not.toContain("onclick");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("javascript:");
    expect(html).not.toContain("<pre");
  });

  it("protects whole HTML blocks from Markdown", () => {
    const { blocks } = protectHtmlBlocks("Text\n<div>\n\n    code?\n</div>\nMore");
    expect(blocks).toEqual(["<div>\n\n    code?\n</div>"]);
    expect(sanitizeCss("a{b:url(data:image/png;base64,AA)}")).toContain("data:image/png");
  });

  it("renders the Vexa HUD for a full reply", () => {
    const shown = applyRegexScripts(vexa, read("reply-full.txt"), { placement: 2, target: "display", depth: 0 }, macros);
    const html = renderRichHtml(shown);
    expect(html.length).toBeGreaterThan(10_000);
    expect(html).toContain("<style>");
    const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
    expect(doc.querySelector("script, iframe, form")).toBeNull();
    const handlers = [...doc.querySelectorAll("*")].flatMap((el) => [...el.attributes].filter((a) => a.name.startsWith("on")));
    expect(handlers).toEqual([]);
  });
});
