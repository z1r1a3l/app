// Golden outputs for the Rust regex engine, produced by SillyTavern's own
// algorithm (regex/engine.js runRegexScript + getRegexedString, utils.js
// regexFromString) on V8's native RegExp. Regenerate with:
//   node gen-golden.mjs > golden.json
import { readFileSync } from "node:fs";

function regexFromString(input) {
  try {
    var m = input.match(/(\/?)(.+)\1([a-z]*)/i);
    if (m[3] && !/^(?!.*?(.).*?\1)[gmixXsuUAJ]+$/.test(m[3])) return RegExp(input);
    return new RegExp(m[2], m[3]);
  } catch { return; }
}
// Deterministic stand-in for substituteParams, mirrored by the Rust test.
function substituteParams(s) {
  return s
    .replace(/\{\{user\}\}/gi, "Алекс")
    .replace(/\{\{char\}\}/gi, "Шарлотта")
    .replace(/\{\{random::([^}]*?)\}\}/gi, (_, list) => list.split("::")[0])
    .replace(/\{\{getglobalvar::([^}]*?)\}\}/gi, () => "");
}
function filterString(raw, trimStrings) {
  let out = raw;
  for (const t of trimStrings ?? []) out = out.replaceAll(substituteParams(t), "");
  return out;
}
function runRegexScript(script, raw) {
  if (!script || script.disabled || !script.findRegex || !raw) return raw;
  const findRegex = regexFromString(script.findRegex);
  if (!findRegex) return raw;
  return raw.replace(findRegex, function (match) {
    const args = [...arguments];
    const replaceString = script.replaceString.replace(/{{match}}/gi, "$0");
    const withGroups = replaceString.replaceAll(/\$(\d+)|\$<([^>]+)>/g, (_, num, groupName) => {
      if (num) match = args[Number(num)];
      else if (groupName) { const groups = args[args.length - 1]; match = groups && typeof groups === "object" && groups[groupName]; }
      if (!match || typeof match !== "string") return "";
      return filterString(match, script.trimStrings);
    });
    return substituteParams(withGroups);
  });
}
function getRegexedString(scripts, raw, placement, { isMarkdown, isPrompt, depth }) {
  let out = raw;
  for (const script of scripts) {
    // Fork rule: no stored-text mode, so "neither only" applies to both.
    const neither = !script.markdownOnly && !script.promptOnly;
    if ((script.markdownOnly && isMarkdown) || (script.promptOnly && isPrompt) || (neither && (isMarkdown || isPrompt))) {
      if (typeof depth === "number") {
        if (!isNaN(script.minDepth) && script.minDepth !== null && script.minDepth >= -1 && depth < script.minDepth) continue;
        if (!isNaN(script.maxDepth) && script.maxDepth !== null && script.maxDepth >= 0 && depth > script.maxDepth) continue;
      }
      if ((script.placement ?? []).includes(placement)) out = runRegexScript(script, out);
    }
  }
  return out;
}

const scripts = JSON.parse(readFileSync(new URL("./vexa-v1.0-regex.json", import.meta.url)));
const inputs = ["reply-full.txt", "reply-short.txt", "reply-streaming-partial.txt"];
const cases = [];
for (const file of inputs) {
  const text = readFileSync(new URL("./" + file, import.meta.url), "utf8");
  cases.push({ file, target: "display", depth: 0, output: getRegexedString(scripts, text, 2, { isMarkdown: true, depth: 0 }) });
  for (const depth of [0, 1, 3, 5]) {
    cases.push({ file, target: "prompt", depth, output: getRegexedString(scripts, text, 2, { isPrompt: true, depth }) });
  }
  // A user message: no Vexa script has the user-input placement.
  cases.push({ file, target: "prompt-user", depth: 1, output: getRegexedString(scripts, text, 1, { isPrompt: true, depth: 1 }) });
}
process.stdout.write(JSON.stringify(cases, null, 1));
