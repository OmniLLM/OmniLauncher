// Helper invoked via `npx tsx` to run a Raycast extension command from
// TypeScript source files. Patches `require` so `@raycast/api` resolves
// to our local shim, then loads and invokes the default export.

"use strict";

const path = require("path");
const Module = require("module");

const target = process.argv[2];
const commandName = process.argv[3] || "";

if (!target) {
  process.stderr.write("raycast-source-loader: missing target file argument\n");
  process.exit(2);
}

const shimPath = path.join(__dirname, "raycast-api-shim.cjs");
const origResolve = Module._resolveFilename;
Module._resolveFilename = function (request, parent, ...rest) {
  if (request === "@raycast/api") return shimPath;
  return origResolve.call(this, request, parent, ...rest);
};

const api = require(shimPath);
api.__resetCapture();

(async () => {
  try {
    const mod = require(target);
    const fn = mod && (mod.default || mod);
    let result;
    if (typeof fn === "function") {
      result = await fn({ launchContext: {}, arguments: {} });
    }
    const captured = api.__getCapture();
    if (captured.length > 0) {
      process.stdout.write(captured.join("\n"));
    } else if (typeof result === "string") {
      process.stdout.write(result);
    } else {
      process.stdout.write(`Ran ${commandName || path.basename(target)}`);
    }
  } catch (e) {
    process.stderr.write(`raycast-source-loader: ${e.message}\n`);
    process.exit(1);
  }
})();
