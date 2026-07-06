import { afterEach, describe, expect, it } from "vitest";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const repoRoot = process.cwd();
const releaseDir = join(repoRoot, "src-tauri", "target", "release");
const binaries = ["omnilauncher", "omnilauncher-frontend", "omnilauncher-backend"];
const backups: Array<{ from: string; to: string }> = [];

function backupRoleBinaries() {
  mkdirSync(releaseDir, { recursive: true });

  for (const name of binaries) {
    const from = join(releaseDir, name);
    const to = join(releaseDir, `${name}.test-backup`);
    rmSync(to, { force: true });
    if (existsSync(from)) {
      renameSync(from, to);
      backups.push({ from, to });
    }
  }
}

afterEach(() => {
  for (const name of binaries) {
    rmSync(join(releaseDir, name), { force: true });
  }
  while (backups.length > 0) {
    const backup = backups.pop();
    if (backup && existsSync(backup.to)) {
      renameSync(backup.to, backup.from);
    }
  }
});

describe("Make/ops role handling", () => {
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

  it("prepare-binaries frontend succeeds without a backend binary", () => {
    backupRoleBinaries();
    writeFileSync(join(releaseDir, "omnilauncher"), "fake binary");

    const result = spawnSync("bash", ["scripts/ops.sh", "prepare-binaries", "frontend"], {
      cwd: repoRoot,
      encoding: "utf8",
    });

    expect(result.status).toBe(0);
    expect(result.stdout).toContain("Prepared role binaries (role=frontend):");
    expect(existsSync(join(releaseDir, "omnilauncher-frontend"))).toBe(true);
    expect(existsSync(join(releaseDir, "omnilauncher-backend"))).toBe(false);
  });
});
