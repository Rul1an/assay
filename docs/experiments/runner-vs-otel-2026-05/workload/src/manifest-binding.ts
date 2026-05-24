/**
 * Compute the tamper-evident manifest-digest binding between an OTel trace
 * and an Assay-Runner archive.
 *
 * Reads `manifest.json` bytes from either:
 *  - a `.tar.gz` Runner archive (Arm A/C, produced by `assay runner-spike run`)
 *  - an extracted archive directory (handy when iterating locally)
 *
 * Returns the SHA-256 digest as `sha256:<hex>`, the same format used by
 * `compare/compare.py` and by the Runner archive's own per-file digests
 * (see `crates/assay-evidence` Merkle infrastructure).
 *
 * The digest is intended to be attached as a span event attribute named
 * `assay.archive.manifest_digest` on the root experiment span, mirroring the
 * SLSA provenance pattern of referring to an artifact by digest rather than
 * embedding it in the trace.
 */

import { createHash } from "node:crypto";
import { readFileSync, existsSync, statSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const MANIFEST_PATH = "manifest.json";

export interface ManifestBinding {
  archivePath: string;
  manifestDigest: string;
  manifestBytes: number;
  source: "tarball" | "directory";
}

export function computeManifestBinding(archivePath: string): ManifestBinding {
  if (!existsSync(archivePath)) {
    throw new Error(`archive path does not exist: ${archivePath}`);
  }

  const stats = statSync(archivePath);
  if (stats.isDirectory()) {
    const manifestFile = join(archivePath, MANIFEST_PATH);
    if (!existsSync(manifestFile)) {
      throw new Error(
        `extracted archive missing ${MANIFEST_PATH}: ${manifestFile}`,
      );
    }
    const bytes = readFileSync(manifestFile);
    return {
      archivePath,
      manifestDigest: digestOf(bytes),
      manifestBytes: bytes.length,
      source: "directory",
    };
  }

  // Treat as tarball. Use the system `tar` to avoid pulling a JS tar dep into
  // the experiment; this also matches what CI / the delegated runner has.
  const workDir = mkdtempSync(join(tmpdir(), "assay-otel-binding-"));
  execFileSync("tar", ["-xzf", archivePath, "-C", workDir, MANIFEST_PATH], {
    stdio: ["ignore", "ignore", "pipe"],
  });
  const bytes = readFileSync(join(workDir, MANIFEST_PATH));
  return {
    archivePath,
    manifestDigest: digestOf(bytes),
    manifestBytes: bytes.length,
    source: "tarball",
  };
}

function digestOf(bytes: Buffer): string {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}
