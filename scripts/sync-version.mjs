#!/usr/bin/env node

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(__dirname, '..');

const version = process.argv[2];
const semverPattern =
  /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:0|[1-9]\d*|[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|[A-Za-z-][0-9A-Za-z-]*))*)?$/;

if (!version || !semverPattern.test(version)) {
  console.error('Usage: node sync-version.mjs <version> (semver, e.g. 1.2.3 or 1.2.3-beta.1)');
  process.exit(1);
}

const packageJsonPath = path.join(rootDir, 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
packageJson.version = version;
const packageJsonOutput = JSON.stringify(packageJson, null, 2) + '\n';

const tauriConfPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
const tauriConf = JSON.parse(fs.readFileSync(tauriConfPath, 'utf-8'));
tauriConf.version = version;
const tauriConfOutput = JSON.stringify(tauriConf, null, 2) + '\n';

const cargoTomlPath = path.join(rootDir, 'src-tauri', 'Cargo.toml');
let cargoToml = fs.readFileSync(cargoTomlPath, 'utf-8');
const packageVersionPattern =
  /(^\[package\]\n(?:[^\[]|\[(?![^\]]+\]))*?^version\s*=\s*)"[^"]*"/m;
if (!packageVersionPattern.test(cargoToml)) {
  throw new Error('Could not find [package] version in src-tauri/Cargo.toml');
}
cargoToml = cargoToml.replace(packageVersionPattern, `$1"${version}"`);

const cargoLockPath = path.join(rootDir, 'Cargo.lock');
let cargoLockOutput = null;
if (fs.existsSync(cargoLockPath)) {
  const cargoLock = fs.readFileSync(cargoLockPath, 'utf-8');
  const cargoLockVersionPattern = /(\[\[package\]\]\nname = "oorouter"\nversion = )"([^"]*)"/;
  const cargoLockMatch = cargoLock.match(cargoLockVersionPattern);
  if (!cargoLockMatch) {
    throw new Error('Could not find oorouter package version in Cargo.lock');
  }
  cargoLockOutput =
    cargoLockMatch[2] === version
      ? cargoLock
      : cargoLock.replace(cargoLockVersionPattern, `$1"${version}"`);
}

fs.writeFileSync(packageJsonPath, packageJsonOutput);
console.log(`Updated package.json to version ${version}`);

fs.writeFileSync(tauriConfPath, tauriConfOutput);
console.log(`Updated src-tauri/tauri.conf.json to version ${version}`);

fs.writeFileSync(cargoTomlPath, cargoToml);
console.log(`Updated src-tauri/Cargo.toml to version ${version}`);

if (cargoLockOutput !== null) {
  fs.writeFileSync(cargoLockPath, cargoLockOutput);
  console.log(`Updated Cargo.lock to version ${version}`);
}
