import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();

describe("Make/ops single-binary handling", () => {
  it("treats lowercase role=backend as a backend-only restart", () => {
    const output = execFileSync(
      "make",
      ["-n", "restart", "role=backend", "DEBUG=1", "VERBOSE=1"],
      { cwd: repoRoot, encoding: "utf8" },
    );

    expect(output).toContain("make stop ROLE=backend");
    expect(output).toContain("make build ROLE=backend");
    expect(output).not.toContain("build-frontend-command");
  });

  it("build no longer copies role binaries (single self-dispatching binary)", () => {
    const output = execFileSync("make", ["-n", "build", "ROLE=backend"], {
      cwd: repoRoot,
      encoding: "utf8",
    });

    // The historical role-copy step is gone: build is just `cargo build`.
    expect(output).toContain("cargo build --release");
    expect(output).not.toContain("prepare-binaries");
    expect(output).not.toContain("omnilauncher-frontend");
    expect(output).not.toContain("omnilauncher-backend");
  });

  it("install-cli symlinks the single binary as `ol`", () => {
    const output = execFileSync("make", ["-n", "install-cli"], {
      cwd: repoRoot,
      encoding: "utf8",
    });

    expect(output).toContain(".local/bin/ol");
    expect(output).toContain("src-tauri/target/release/omnilauncher");
  });
});
