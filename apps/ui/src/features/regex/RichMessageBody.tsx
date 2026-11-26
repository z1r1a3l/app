/**
 * Fork-only: a message whose regex output carries HTML/CSS, rendered in its
 * own Shadow DOM so its styles never leak into the app (or other messages)
 * and the app's styles never break it.
 */
import { useDeferredValue, useLayoutEffect, useMemo, useRef } from "react";
import { renderRichHtml } from "./rich-html";

const BASE_CSS = `
:host { display: block; contain: layout paint; overflow-wrap: anywhere; line-height: 1.6; }
.lettuce-rich { color: inherit; font: inherit; }
.lettuce-rich > :first-child { margin-top: 0; }
.lettuce-rich > :last-child { margin-bottom: 0; }
p { margin: 0 0 0.75em; }
em { color: color-mix(in oklab, currentColor 72%, transparent); }
a { color: var(--color-accent, #7aa2ff); }
img { max-width: 100%; height: auto; }
pre, code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.9em; }
pre { white-space: pre-wrap; overflow-x: auto; }
blockquote { margin: 0.5em 0; padding-left: 0.8em; border-left: 3px solid color-mix(in oklab, currentColor 25%, transparent); }
table { border-collapse: collapse; max-width: 100%; }
details > summary { cursor: pointer; }
`;

export function RichMessageBody({ text }: { text: string }) {
  const host = useRef<HTMLDivElement>(null);
  const deferred = useDeferredValue(text);
  const html = useMemo(() => renderRichHtml(deferred), [deferred]);
  useLayoutEffect(() => {
    const element = host.current;
    if (!element) return;
    const root = element.shadowRoot ?? element.attachShadow({ mode: "open" });
    let content = root.querySelector<HTMLDivElement>(".lettuce-rich");
    if (!content) {
      const base = document.createElement("style");
      base.textContent = BASE_CSS;
      content = document.createElement("div");
      content.className = "lettuce-rich";
      root.append(base, content);
    }
    // Keep open disclosures and checked controls open across re-renders
    // while a reply streams.
    const open = [...content.querySelectorAll("details")].map((d) => d.open);
    const checked = [...content.querySelectorAll<HTMLInputElement>("input")].map((i) => i.checked);
    content.innerHTML = html;
    const details = content.querySelectorAll("details");
    open.forEach((value, index) => {
      const d = details[index];
      if (d && value) d.open = true;
    });
    const inputs = content.querySelectorAll<HTMLInputElement>("input");
    checked.forEach((value, index) => {
      const input = inputs[index];
      if (input && (input.type === "checkbox" || input.type === "radio") && value) input.checked = true;
    });
  }, [html]);
  return <div ref={host} data-rich-message="" className="select-text" />;
}
