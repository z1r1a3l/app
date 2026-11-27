/**
 * Fork-only: SillyTavern regex scripts for the chat view.
 *
 * The same algorithm as `crates/lettuce-context/src/fork_regex/engine.rs`
 * (a port of SillyTavern's `extensions/regex/engine.js`), on the
 * WebView's own JavaScript RegExp, so patterns behave exactly as in ST.
 */
import type { RegexScriptDto } from "@/api/generated/bindings";

export type RegexScript = RegexScriptDto;

export const placement = {
  userInput: 1,
  aiOutput: 2,
  slashCommand: 3,
  worldInfo: 5,
  reasoning: 6,
} as const;

export type RegexTarget = "display" | "prompt";

export interface RegexApplyContext {
  placement: number;
  target: RegexTarget;
  /** Messages after this one; the newest message is depth 0. */
  depth?: number;
}

export interface MacroValues {
  user?: string;
  char?: string;
  vars?: Record<string, string>;
  globalVars?: Record<string, string>;
  /** Fork: the chat's own `{{key}}` macros, keys in lower case. */
  custom?: Record<string, string>;
  /** Picks an index below `bound` for `{{random}}` / `{{roll}}`; tests make it fixed. */
  random?: (bound: number) => number;
}

export function newScript(id: string, name = ""): RegexScript {
  return {
    id,
    script_name: name,
    find_regex: "",
    replace_string: "",
    trim_strings: [],
    placement: [placement.aiOutput],
    disabled: false,
    markdown_only: true,
    prompt_only: false,
    run_on_edit: true,
    substitute_regex: 0,
    min_depth: null,
    max_depth: null,
  };
}

/** Neither flag means both (this fork keeps stored text unchanged). */
export function appliesTo(script: RegexScript, target: RegexTarget): boolean {
  return target === "display"
    ? script.markdown_only || !script.prompt_only
    : script.prompt_only || !script.markdown_only;
}

const cache = new Map<string, RegExp | null>();

/** ST `regexFromString`; `null` for a pattern that does not compile. */
export function compileFindRegex(input: string): RegExp | null {
  if (!input) return null;
  const cached = cache.get(input);
  if (cached !== undefined) return cached;
  let compiled: RegExp | null = null;
  try {
    const parts = input.match(/(\/?)(.+)\1([a-z]*)/i);
    if (parts) {
      let [, , pattern, flags] = parts as unknown as [string, string, string, string];
      if (flags && !/^(?!.*?(.).*?\1)[gmixXsuUAJ]+$/.test(flags)) {
        pattern = input;
        flags = "";
      }
      compiled = new RegExp(pattern, flags);
    }
  } catch {
    compiled = null;
  }
  if (cache.size >= 1000) cache.clear();
  cache.set(input, compiled);
  return compiled;
}

function escapeRegex(value: string): string {
  return value.replace(/[/\-\\^$*+?.()|[\]{}]/g, "\\$&");
}

let seed = 0;
function nextRandom(bound: number): number {
  if (bound <= 1) return 0;
  if (seed === 0) seed = (Date.now() | 1) >>> 0;
  seed ^= seed << 13;
  seed >>>= 0;
  seed ^= seed >>> 17;
  seed ^= seed << 5;
  seed >>>= 0;
  return seed % bound;
}

function args(body: string, name: string): string[] | null {
  const lower = body.toLowerCase();
  if (lower === name) return [];
  if (!lower.startsWith(name)) return null;
  const raw = body.slice(name.length);
  if (raw.startsWith("::")) return raw.slice(2).split("::");
  if (raw.startsWith(":")) return raw.slice(1).split(",").map((s) => s.trim());
  if (raw.startsWith(" ")) return [raw.trim()];
  return null;
}

function roll(formula: string): string | null {
  let f = formula.trim().toLowerCase().replaceAll(" ", "");
  if (/^\d+$/.test(f)) f = `1d${f}`;
  const m = f.match(/^(\d*)d(\d+)([+-]\d+)?$/);
  if (!m) return null;
  const count = m[1] ? Number(m[1]) : 1;
  const sides = Number(m[2]);
  if (count === 0 || count > 1000 || sides === 0) return null;
  let total = Number(m[3] ?? 0);
  for (let i = 0; i < count; i++) total += nextRandom(sides) + 1;
  return String(total);
}

function resolve(body: string, values: MacroValues): string | null {
  const trimmed = body.trim();
  if (trimmed.startsWith("//")) return "";
  switch (trimmed.toLowerCase()) {
    case "user":
      return values.user ?? "";
    case "char":
      return values.char ?? "";
    case "noop":
      return "";
    case "newline":
      return "\n";
  }
  const pick = values.random ?? nextRandom;
  let list = args(trimmed, "random");
  if (list) return list.length ? list[pick(list.length)]! : null;
  list = args(trimmed, "roll");
  if (list) return list[0] !== undefined ? roll(list[0]) : null;
  list = args(trimmed, "getglobalvar");
  if (list) return list[0] !== undefined ? (values.globalVars?.[list[0].trim()] ?? "") : null;
  list = args(trimmed, "getvar");
  if (list) return list[0] !== undefined ? (values.vars?.[list[0].trim()] ?? "") : null;
  const custom = values.custom?.[trimmed.toLowerCase()];
  if (custom !== undefined && Object.prototype.hasOwnProperty.call(values.custom, trimmed.toLowerCase())) return custom;
  return null;
}

/** The macro subset regex replacements use; unknown macros stay as written. */
export function substituteMacros(text: string, values: MacroValues): string {
  if (!text.includes("{{")) return text;
  let current = text;
  for (let pass = 0; pass < 64; pass++) {
    let changed = false;
    let out = "";
    let rest = current;
    for (;;) {
      const open = rest.indexOf("{{");
      if (open < 0) break;
      const after = rest.slice(open + 2);
      const close = after.indexOf("}}");
      if (close < 0) break;
      const body = after.slice(0, close);
      const inner = body.lastIndexOf("{{");
      if (inner >= 0) {
        out += rest.slice(0, open + 2 + inner);
        rest = rest.slice(open + 2 + inner);
        continue;
      }
      out += rest.slice(0, open);
      if (body.trim().toLowerCase() === "trim") {
        out = out.replace(/[\r\n]+$/, "");
        rest = after.slice(close + 2).replace(/^[\r\n]+/, "");
        changed = true;
        continue;
      }
      const value = resolve(body, values);
      if (value === null) out += rest.slice(open, open + 2 + close + 2);
      else {
        out += value;
        changed = true;
      }
      rest = after.slice(close + 2);
    }
    current = out + rest;
    if (!changed) break;
  }
  return current;
}

/** ST `runRegexScript`. */
export function runRegexScript(script: RegexScript, text: string, values: MacroValues): string {
  if (script.disabled || !script.find_regex || !text) return text;
  const macros = (value: string) => substituteMacros(value, values);
  const find =
    script.substitute_regex === 1
      ? macros(script.find_regex)
      : script.substitute_regex === 2
        ? script.find_regex.replace(/\{\{[^{}]*\}\}/g, (token) => {
            const value = macros(token);
            return value === token ? token : escapeRegex(value);
          })
        : script.find_regex;
  const regex = compileFindRegex(find);
  if (!regex) return text;
  regex.lastIndex = 0;
  const template = script.replace_string.replace(/{{match}}/gi, "$0");
  try {
    return text.replace(regex, (...found: unknown[]) => {
      const last = found[found.length - 1];
      const groups = typeof last === "object" && last !== null ? (last as Record<string, string>) : undefined;
      const withGroups = template.replaceAll(/\$(\d+)|\$<([^>]+)>/g, (_, num?: string, name?: string) => {
        const value = num ? found[Number(num)] : name ? groups?.[name] : undefined;
        if (typeof value !== "string" || !value) return "";
        let filtered = value;
        for (const trim of script.trim_strings) {
          const t = macros(trim);
          if (t) filtered = filtered.replaceAll(t, "");
        }
        return filtered;
      });
      return macros(withGroups);
    });
  } catch {
    return text;
  }
}

/** ST `getRegexedString`: every applicable script in order. */
export function applyRegexScripts(
  scripts: readonly RegexScript[],
  text: string,
  context: RegexApplyContext,
  values: MacroValues = {},
): string {
  let out = text;
  if (!out) return out;
  for (const script of scripts) {
    if (script.disabled || !appliesTo(script, context.target)) continue;
    if (context.depth !== undefined) {
      const min = script.min_depth;
      const max = script.max_depth;
      if (min != null && min >= -1 && context.depth < min) continue;
      if (max != null && max >= 0 && context.depth > max) continue;
    }
    if (script.placement.includes(context.placement)) {
      out = runRegexScript(script, out, values);
    }
  }
  return out;
}

/** Which side a script changes: what you see, what the model gets, or both. */
export type Affects = "display" | "prompt" | "both";

export function affectsOf(script: Pick<RegexScript, "markdown_only" | "prompt_only">): Affects {
  if (script.markdown_only && !script.prompt_only) return "display";
  if (script.prompt_only && !script.markdown_only) return "prompt";
  return "both";
}

