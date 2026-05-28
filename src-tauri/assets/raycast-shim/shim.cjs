#!/usr/bin/env node
// OmniLauncher entry shim for Raycast extensions.
//
// This file is auto-generated when a Raycast extension is installed.
// It implements OmniLauncher's stdin/stdout JSON plugin protocol and
// dispatches `op=execute` calls to the extension's compiled command files
// while injecting a mock `@raycast/api` (see raycast-api-shim.cjs).

"use strict";

const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const extDir = __dirname;
const pkgPath = path.join(extDir, "package.json");

let pkg = {};
try {
  pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
} catch (err) {
  process.stderr.write(`raycast-shim: failed to read package.json: ${err.message}\n`);
}

const commands = Array.isArray(pkg.commands) ? pkg.commands : [];
const extName = pkg.name || path.basename(extDir);
const extTitle = pkg.title || extName;
const extKeyword = (process.env.OMNI_RAYCAST_KEYWORD || extName).toLowerCase();
const extIcon = pkg.icon || "🟥";

function reply(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

/**
 * Parse the launcher query. Returns:
 *   { matched: bool, rest: string }
 * where `matched` is true if the extension's keyword prefix was present
 * (or there is no keyword), and `rest` is the remaining search text.
 */
function parseQuery(query) {
  const q = (query || "").trim();
  if (!extKeyword) return { matched: true, rest: q };
  const lower = q.toLowerCase();
  const kw = extKeyword.toLowerCase();
  if (lower === kw) return { matched: true, rest: "" };
  if (lower.startsWith(kw + " ")) {
    return { matched: true, rest: q.slice(extKeyword.length + 1).trim() };
  }
  // Not a keyword match — caller decides whether to filter globally.
  return { matched: false, rest: q };
}

/**
 * Build the list of OmniLauncher result rows for a Raycast extension.
 * Behavior:
 *   - Keyword matched  → list every command. Encode the remaining text
 *                        as the command's search input so execute can use it.
 *   - Keyword absent   → filter commands by substring against the whole
 *                        query (global "search any extension" mode).
 */
function listCommands(parsed) {
  let candidates = commands;
  if (!parsed.matched) {
    const f = parsed.rest.toLowerCase();
    if (!f) return [];
    candidates = commands.filter((c) => {
      const hay = [c.title, c.name, c.subtitle, c.description]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return hay.includes(f);
    });
  }

  const userQuery = parsed.matched ? parsed.rest : "";

  return candidates.map((c, i) => {
    const subtitle = userQuery
      ? `${c.title || c.name} — search "${userQuery}"`
      : c.subtitle || c.description || `${c.mode || "command"}`;
    return {
      id: c.name,
      title: userQuery
        ? `${extTitle}: ${c.title || c.name} "${userQuery}"`
        : `${extTitle}: ${c.title || c.name}`,
      subtitle,
      icon: c.icon || extIcon,
      score: 95 - i,
      action_type: "plugin_execute",
      // Encode command + user query so the execute side can pass the query
      // into the Raycast command (via arguments / env / fallback URL).
      action_data: JSON.stringify({ cmd: c.name, q: userQuery }),
    };
  });
}

function findCommandFile(commandName) {
  const builtCandidates = [
    path.join(extDir, "dist", `${commandName}.js`),
    path.join(extDir, ".dist", `${commandName}.js`),
    path.join(extDir, "build", `${commandName}.js`),
  ];
  for (const f of builtCandidates) {
    if (fs.existsSync(f)) return { kind: "built", file: f };
  }
  const sourceCandidates = [
    path.join(extDir, "src", `${commandName}.tsx`),
    path.join(extDir, "src", `${commandName}.ts`),
    path.join(extDir, "src", `${commandName}.jsx`),
    path.join(extDir, "src", `${commandName}.js`),
  ];
  for (const f of sourceCandidates) {
    if (fs.existsSync(f)) return { kind: "source", file: f };
  }
  return null;
}

function patchRequire(shimPath) {
  const Module = require("module");
  const orig = Module._resolveFilename;
  Module._resolveFilename = function (request, parent, ...rest) {
    if (request === "@raycast/api") return shimPath;
    return orig.call(this, request, parent, ...rest);
  };
}

async function runBuilt(found, commandName, userQuery) {
  const shimPath = path.join(__dirname, "raycast-api-shim.cjs");
  patchRequire(shimPath);

  process.env.OMNI_RAYCAST_EXT_DIR = extDir;
  process.env.OMNI_RAYCAST_COMMAND = commandName;
  process.env.OMNI_RAYCAST_QUERY = userQuery || "";

  const api = require(shimPath);
  api.__resetCapture();

  // If OmniLauncher built a headless search bundle (dist/<name>.search.js),
  // use it to answer the query without invoking any React hooks.
  // The search bundle exports the extension's utility functions directly.
  const searchBundlePath = path.join(extDir, "dist", `${commandName}.search.js`);
  if (fs.existsSync(searchBundlePath)) {
    try {
      const searchMod = require(searchBundlePath);
      const output = runHeadlessSearch(searchMod, userQuery || "", commandName);
      if (output !== null) {
        reply({ output });
        return;
      }
    } catch (e) {
      // Search bundle failed — fall through to normal execution path.
      log(`raycast-shim: search bundle error for '${commandName}': ${e.message}`);
    }
  }

  let mod;
  try {
    mod = require(found.file);
  } catch (e) {
    reply({
      output: `Raycast command '${commandName}' failed to load: ${e.message}`,
    });
    return;
  }

  const fn = mod && (mod.default || mod);
  const launchProps = {
    launchContext: {},
    arguments: userQuery ? { query: userQuery, text: userQuery } : {},
    fallbackText: userQuery || "",
  };

  try {
    let returnValue;
    if (typeof fn === "function") {
      returnValue = await fn(launchProps);
    }
    const captured = api.__getCapture();
    let outputText;
    if (captured.length > 0) {
      outputText = captured.join("\n");
    } else if (typeof returnValue === "string") {
      outputText = returnValue;
    } else {
      outputText = `Ran ${commandName}`;
    }
    reply({ output: outputText });
  } catch (e) {
    reply({ output: `Raycast command '${commandName}' threw: ${e.message}` });
  }
}

/**
 * Try to run a headless search against a search bundle module.
 * Looks for common patterns: getStaticResult, search, filter functions,
 * or an exported list/array to search over.
 * Returns output text, or null if no usable search function was found.
 */
function runHeadlessSearch(mod, userQuery, commandName) {
  // Strip common exchange prefixes like "NASDAQ:AAPL" → "AAPL"
  const normalizedQuery = userQuery.replace(/^[A-Z0-9]+:/i, "").trim();
  const q = normalizedQuery.toLowerCase();

  // 1. Look for a dedicated search/filter function by common names.
  const searchFnNames = ["getStaticResult", "search", "filter", "query", "find", "lookup"];
  for (const name of searchFnNames) {
    if (typeof mod[name] === "function") {
      try {
        const results = mod[name](normalizedQuery);
        // If the dedicated function returned results, use them.
        // If it returned nothing, fall through to generic search so the shim
        // can broaden the match (e.g. search descriptions, not just tickers).
        if (Array.isArray(results) && results.length > 0) {
          return formatResults(results, userQuery, commandName);
        }
      } catch {
        // try next
      }
    }
  }

  // 2. Look for an exported array/list to search over generically.
  for (const key of Object.keys(mod)) {
    const val = mod[key];
    if (Array.isArray(val) && val.length > 0 && typeof val[0] === "object") {
      const hits = val.filter((item) => {
        return Object.values(item).some(
          (v) => typeof v === "string" && v.toLowerCase().includes(q)
        );
      });
      return formatResults(hits, userQuery, commandName);
    }
  }

  return null;
}

/**
 * Format an array of result objects (or strings) into plain text output.
 */
function formatResults(results, userQuery, commandName) {
  if (!Array.isArray(results) || results.length === 0) {
    return `No results found for "${userQuery}".`;
  }
  return results
    .slice(0, 20)
    .map((r) => {
      if (typeof r === "string") return r;
      // Prefer common field names: query/title/name + description + url
      const label = r.query || r.title || r.name || r.id || JSON.stringify(r);
      const desc = r.description || r.subtitle || "";
      const url = r.url || r.link || "";
      return [label, desc, url].filter(Boolean).join(" — ");
    })
    .join("\n");
}

function log(msg) {
  process.stderr.write(msg + "\n");
}

function runSource(found, commandName, userQuery) {
  const runner = process.platform === "win32" ? "npx.cmd" : "npx";
  const check = spawnSync(runner, ["--no-install", "tsx", "--version"], {
    cwd: extDir,
  });
  if (check.status !== 0) {
    reply({
      output:
        `Raycast command '${commandName}' is not built. ` +
        `Run 'npm install && npm run build' in ${extDir} ` +
        `(or install 'tsx' globally to run from source).`,
    });
    return;
  }
  // tsx alone cannot inject our '@raycast/api' shim. Use a tiny loader.
  const loader = path.join(__dirname, "raycast-source-loader.cjs");
  const out = spawnSync(
    runner,
    ["--no-install", "tsx", loader, found.file, commandName],
    {
      cwd: extDir,
      env: {
        ...process.env,
        OMNI_RAYCAST_EXT_DIR: extDir,
        OMNI_RAYCAST_COMMAND: commandName,
        OMNI_RAYCAST_QUERY: userQuery || "",
      },
    }
  );
  const stdout = (out.stdout || "").toString().trim();
  const stderr = (out.stderr || "").toString().trim();
  reply({
    output: stdout || stderr || `Ran ${commandName}`,
  });
}

function decodeActionData(actionData) {
  if (!actionData) return { cmd: "", q: "" };
  // Backwards-compat: older shim versions encoded just the command name
  // as a bare string. Newer shim encodes JSON {cmd, q}.
  if (actionData[0] === "{") {
    try {
      const obj = JSON.parse(actionData);
      return { cmd: obj.cmd || "", q: obj.q || "" };
    } catch {
      return { cmd: actionData, q: "" };
    }
  }
  return { cmd: actionData, q: "" };
}

async function execute(actionData) {
  const { cmd: commandName, q: userQuery } = decodeActionData(actionData);
  if (!commandName) {
    reply({ output: "No Raycast command specified." });
    return;
  }
  const found = findCommandFile(commandName);
  if (!found) {
    reply({
      output:
        `Raycast command '${commandName}' has no built dist/ output and no source file in src/. ` +
        `Run 'npm install && npm run build' in ${extDir}.`,
    });
    return;
  }
  if (found.kind === "built") {
    await runBuilt(found, commandName, userQuery);
  } else {
    runSource(found, commandName, userQuery);
  }
}

// Invoked by the AI agent via `op: tool_call`. Picks the requested command
// (or the first one when unspecified), runs it with `args.query`, and
// returns the captured output as plain text.
async function toolCall(args) {
  const a = args && typeof args === "object" ? args : {};
  const query = typeof a.query === "string" ? a.query : "";
  let commandName = typeof a.command === "string" ? a.command : "";
  if (!commandName) {
    commandName = commands.length > 0 ? commands[0].name : "";
  }
  if (!commandName) {
    reply({ output: `Raycast extension '${extName}' has no commands.` });
    return;
  }
  // Verify the requested command exists in the extension.
  if (!commands.some((c) => c.name === commandName)) {
    reply({
      output:
        `Raycast extension '${extName}' has no command '${commandName}'. ` +
        `Available: ${commands.map((c) => c.name).join(", ") || "(none)"}.`,
    });
    return;
  }
  await execute(JSON.stringify({ cmd: commandName, q: query }));
}

(async () => {
  let raw = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (d) => (raw += d));
  process.stdin.on("end", async () => {
    let req = {};
    try {
      req = JSON.parse(raw);
    } catch {
      /* ignore */
    }
    if (req.op === "query") {
      reply({ results: listCommands(parseQuery(req.query)) });
    } else if (req.op === "execute") {
      await execute(req.action_data || req.id || "");
    } else if (req.op === "tool_call") {
      await toolCall(req.args || {});
    } else {
      reply({ results: [] });
    }
  });
})();
