// Exact runtime pair check for governed fixture generation.
// Rejects missing, malformed, or mismatched Node or npm versions.
// Does not echo attacker-controlled values; only prints governed constants on mismatch.
'use strict';

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const nodeVersionFile = path.join(__dirname, '.node-version');
const pkgJsonFile = path.join(__dirname, 'package.json');

// Read governed Node version from .node-version
let governedNode;
try {
  governedNode = fs.readFileSync(nodeVersionFile, 'utf8').trim();
} catch {
  console.error('FAIL: cannot read .node-version');
  process.exit(1);
}
if (!/^\d+\.\d+\.\d+$/.test(governedNode)) {
  console.error('FAIL: .node-version is not a valid semver');
  process.exit(1);
}

// Read governed npm version from package.json packageManager field
let governedNpm;
try {
  const pkg = JSON.parse(fs.readFileSync(pkgJsonFile, 'utf8'));
  const pm = pkg.packageManager;
  if (typeof pm !== 'string') {
    console.error('FAIL: package.json packageManager is missing or not a string');
    process.exit(1);
  }
  const match = /^npm@(\d+\.\d+\.\d+)$/.exec(pm);
  if (!match) {
    console.error('FAIL: package.json packageManager is not in npm@x.y.z format');
    process.exit(1);
  }
  governedNpm = match[1];
} catch (e) {
  if (e.code) {
    // fs/parse error, not our explicit exit
    console.error('FAIL: cannot read or parse package.json');
    process.exit(1);
  }
  throw e; // re-throw our explicit process.exit calls
}

// Check Node version (process.versions.node, not attacker-controlled)
const actualNode = process.versions.node;
if (actualNode !== governedNode) {
  console.error('FAIL: Node ' + governedNode + ' required, got different version');
  process.exit(1);
}

// Check npm version via npm_config_user_agent (set by npm itself, not echo of argv)
const userAgent = process.env.npm_config_user_agent || '';
const npmMatch = /^npm\/(\d+\.\d+\.\d+)\s/.exec(userAgent);
if (!npmMatch) {
  console.error('FAIL: npm_config_user_agent missing or malformed');
  process.exit(1);
}
const actualNpm = npmMatch[1];
if (actualNpm !== governedNpm) {
  console.error('FAIL: npm ' + governedNpm + ' required, got different version');
  process.exit(1);
}
