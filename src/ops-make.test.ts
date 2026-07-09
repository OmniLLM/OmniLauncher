import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();

// Keep a little headroom for Make startup on Windows runners.
const MAKE_TIMEOUT_MS = 30_000;

describe("Make/build-install boundary", () => {
  it("build only builds frontend assets and the single release binary", () => {
    const output = execFileSync("make", ["-n", "build"], {
      cwd: repoRoot,
      encoding: "utf8",
    });

    expect(output).toContain("npm run build");
    expect(output).toContain("cargo build --release");
    expect(output).not.toContain(" ol start");
    expect(output).not.toContain(" ol stop");
    expect(output).not.toContain("prepare-binaries");
    expect(output).not.toContain("omnilauncher-frontend");
    expect(output).not.toContain("omnilauncher-backend");
  }, MAKE_TIMEOUT_MS);

  it("install-cli symlinks the single binary as `ol`", () => {
    const output = execFileSync("make", ["-n", "install-cli"], {
      cwd: repoRoot,
      encoding: "utf8",
    });

    expect(output).toContain(".local/bin/ol");
    expect(output).toContain(".local/bin/omnilauncher");
    expect(output).toContain("src-tauri/target/release/omnilauncher");
  }, MAKE_TIMEOUT_MS);
});
