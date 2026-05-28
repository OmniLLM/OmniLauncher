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

  // Raycast view-mode commands are React components. Calling them outside a
  // real renderer makes React 19's hook dispatcher resolve to null, so the
  // first `useState()` call throws `Cannot read properties of null (reading
  // 'useState')`. Install a stub dispatcher so the synchronous body can run
  // and the shim can capture any side-effects (toast/HUD/clipboard) before
  // returning a meaningful message instead of a React internals error.
  const restoreDispatcher = installStubHookDispatcher();
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
      // The component rendered (or returned a React element) but produced no
      // captured output. View-mode commands need an interactive launcher; we
      // can't fully render their UI here. Surface this clearly to the AI.
      outputText = viewModeFallbackMessage(commandName, userQuery);
    }
    reply({ output: outputText });
  } catch (e) {
    reply({
      output:
        `Raycast command '${commandName}' is a view-mode UI command and can't be ` +
        `fully executed headlessly (${e.message}). ` +
        viewModeFallbackMessage(commandName, userQuery),
    });
  } finally {
    restoreDispatcher();
  }
}

/**
 * Replace React 19's hook dispatcher with a stub so hooks called outside a
 * real renderer return sensible defaults instead of throwing on `null`.
 * Returns a function that restores the original dispatcher.
 *
 * Stubs only the hooks needed for a typical synchronous first render:
 * useState/useReducer return `[initial, noop]`, useEffect/useLayoutEffect/
 * useInsertionEffect are noops, useMemo/useCallback invoke the factory
 * synchronously, useRef returns `{current: initial ?? null}`, useContext
 * returns the context's default value, and useId returns a deterministic id.
 *
 * This is best-effort: components that depend on async effects to populate
 * data will still see only their initial loading state.
 */
function installStubHookDispatcher() {
  let React;
  try {
    React = require("react");
  } catch {
    return () => {};
  }
  const internals =
    React.__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE ||
    React.__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED;
  if (!internals) return () => {};

  // React 19 uses `H`; React 18 used `ReactCurrentDispatcher.current`.
  const hasH = "H" in internals;
  const original = hasH
    ? internals.H
    : internals.ReactCurrentDispatcher && internals.ReactCurrentDispatcher.current;

  let idCounter = 0;
  const noop = () => {};
  const stub = {
    useState: (init) => [typeof init === "function" ? init() : init, noop],
    useReducer: (_r, init, initFn) => [
      typeof initFn === "function" ? initFn(init) : init,
      noop,
    ],
    useEffect: noop,
    useLayoutEffect: noop,
    useInsertionEffect: noop,
    useMemo: (factory) => {
      try {
        return factory();
      } catch {
        return undefined;
      }
    },
    useCallback: (cb) => cb,
    useRef: (init) => ({ current: init === undefined ? null : init }),
    useContext: (ctx) => (ctx && "_currentValue" in ctx ? ctx._currentValue : undefined),
    useImperativeHandle: noop,
    useDebugValue: noop,
    useId: () => `omni-shim-id-${++idCounter}`,
    useTransition: () => [false, noop],
    useDeferredValue: (v) => v,
    useSyncExternalStore: (_sub, get) => {
      try {
        return get();
      } catch {
        return undefined;
      }
    },
    useOptimistic: (v) => [v, noop],
    useActionState: (_action, init) => [init, noop, false],
  };

  if (hasH) {
    internals.H = stub;
    return () => {
      internals.H = original;
    };
  }
  if (internals.ReactCurrentDispatcher) {
    internals.ReactCurrentDispatcher.current = stub;
    return () => {
      internals.ReactCurrentDispatcher.current = original;
    };
  }
  return () => {};
}

function viewModeFallbackMessage(commandName, userQuery) {
  const q = userQuery ? ` for "${userQuery}"` : "";
  return (
    `Raycast command '${commandName}' is an interactive view-mode UI command. ` +
    `OmniLauncher's AI shim can't fully render its React-based interface headlessly` +
    `${q}. Open the launcher and type the extension keyword to use it interactively, ` +
    `or call a different tool (e.g. web_search / web_fetch) for this query.`
  );
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
