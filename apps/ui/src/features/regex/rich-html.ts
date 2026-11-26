/**
 * Fork-only: turns a regex-processed message into safe HTML for a Shadow DOM.
 *
 * Markdown is rendered with `marked`; complete HTML blocks are kept whole
 * first (as SillyTavern's showdown does), so blank lines or indentation
 * inside a styled panel never turn it into a code block. DOMPurify then
 * keeps markup and CSS (style tags, classes, ids, details, form controls,
 * popovers) and drops every script, frame, form, event handler and
 * `javascript:` URL. CSS may only load `data:` URLs and never `@import`.
 */
import DOMPurify from "dompurify";
import { marked } from "marked";

const BLOCK_TAGS = new Set([
  "address", "article", "aside", "blockquote", "center", "details", "dialog", "div", "dl",
  "fieldset", "figure", "footer", "header", "main", "nav", "ol", "section", "style", "table",
  "ul", "pre", "font", "span", "label", "button", "select", "input", "img", "hr", "br", "p",
  "h1", "h2", "h3", "h4", "h5", "h6", "summary", "small", "b", "i", "u", "s", "strong", "em",
]);
const VOID_TAGS = new Set(["input", "img", "hr", "br", "meta", "link", "source", "wbr", "col", "area"]);

/** Finds the end of the element starting at `start` (a `<tag`), or -1. */
function elementEnd(text: string, start: number, tag: string): number {
  if (VOID_TAGS.has(tag)) {
    const close = text.indexOf(">", start);
    return close < 0 ? -1 : close + 1;
  }
  if (tag === "style" || tag === "pre") {
    const close = text.toLowerCase().indexOf(`</${tag}>`, start);
    return close < 0 ? -1 : close + tag.length + 3;
  }
  const pattern = new RegExp(`<(/?)${tag}(?=[\\s>/])[^>]*?(/?)>`, "gi");
  pattern.lastIndex = start;
  let depth = 0;
  for (let found = pattern.exec(text); found; found = pattern.exec(text)) {
    if (found[1]) depth -= 1;
    else if (!found[2]) depth += 1;
    if (depth === 0) return pattern.lastIndex;
  }
  return -1;
}

/** Replaces complete top-level HTML blocks that start a line with placeholders. */
export function protectHtmlBlocks(text: string): { text: string; blocks: string[] } {
  const blocks: string[] = [];
  let out = "";
  let index = 0;
  const opener = /(^|\n)([ \t]*)<([a-zA-Z][a-zA-Z0-9-]*)(?=[\s>/])/g;
  for (let found = opener.exec(text); found; found = opener.exec(text)) {
    const tagStart = found.index + found[1]!.length + found[2]!.length;
    if (tagStart < index) continue;
    const tag = found[3]!.toLowerCase();
    if (!BLOCK_TAGS.has(tag)) continue;
    const end = elementEnd(text, tagStart, tag);
    if (end < 0) continue;
    // Only whole lines: the element must end its line.
    const rest = text.slice(end).match(/^[ \t]*(\n|$)/);
    if (!rest) continue;
    out += text.slice(index, tagStart);
    out += `\n\n<!--lettuce-block-${blocks.length}-->\n\n`;
    blocks.push(text.slice(tagStart, end));
    index = end;
    opener.lastIndex = end;
  }
  return { text: out + text.slice(index), blocks };
}

const CSS_IMPORT = /@import[^;]*;?/gi;
const CSS_URL = /url\(\s*(['"]?)(.*?)\1\s*\)/gi;
const CSS_BAD = /expression\s*\(|behavior\s*:|-moz-binding/gi;

export function sanitizeCss(css: string): string {
  return css
    .replace(CSS_IMPORT, "")
    .replace(CSS_BAD, "x:")
    .replace(CSS_URL, (whole, _quote: string, url: string) =>
      /^data:image\/(png|jpe?g|gif|webp|svg\+xml|avif);/i.test(url.trim()) || /^data:font\//i.test(url.trim())
        ? whole
        : "none",
    );
}

let hooked = false;
function hookPurifier() {
  if (hooked) return;
  hooked = true;
  DOMPurify.addHook("uponSanitizeElement", (node, data) => {
    if (data.tagName === "style" && node.textContent) {
      node.textContent = sanitizeCss(node.textContent);
    }
  });
  DOMPurify.addHook("uponSanitizeAttribute", (_node, data) => {
    if (data.attrName === "style") data.attrValue = sanitizeCss(data.attrValue);
    if (data.attrName === "href" || data.attrName === "src") {
      const value = data.attrValue.trim().toLowerCase();
      if (!/^(https?:|data:image\/|#|mailto:)/.test(value)) data.keepAttr = false;
    }
  });
  DOMPurify.addHook("afterSanitizeAttributes", (node) => {
    if (node.tagName === "A") {
      node.setAttribute("target", "_blank");
      node.setAttribute("rel", "noopener noreferrer");
    }
  });
}

const PURIFY = {
  FORCE_BODY: true,
  ADD_TAGS: ["style", "font", "center"],
  ADD_ATTR: [
    "popover", "popovertarget", "popovertargetaction", "color", "face", "size", "open",
    "name", "for", "checked", "value", "selected", "type", "tabindex", "role", "target",
  ],
  FORBID_TAGS: ["script", "iframe", "frame", "frameset", "object", "embed", "form", "link", "meta", "base", "noscript", "template", "svg", "math", "audio", "video", "textarea"],
  FORBID_ATTR: ["formaction", "action", "srcset", "ping", "background"],
  ALLOW_DATA_ATTR: true,
  ALLOW_ARIA_ATTR: true,
};

/** Markdown + HTML to sanitized HTML. */
export function renderRichHtml(text: string): string {
  hookPurifier();
  const { text: guarded, blocks } = protectHtmlBlocks(text);
  let html = marked.parse(guarded, { async: false, gfm: true, breaks: true }) as string;
  html = html.replace(/<!--lettuce-block-(\d+)-->/g, (_, n: string) => blocks[Number(n)] ?? "");
  return DOMPurify.sanitize(html, PURIFY) as unknown as string;
}

/** Whether a text needs the rich renderer (it carries markup beyond Markdown). */
export function hasRichMarkup(text: string): boolean {
  return /<(style|details|div|font|input|label|select|button|section|table|center)\b/i.test(text);
}
