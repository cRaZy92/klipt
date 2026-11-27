#!/usr/bin/env node
// Cuts a desktop release: bumps the app's version everywhere, commits and tags `vX.Y.Z`.
// Pushing the tag makes CI publish it (.github/workflows/desktop-windows.yml, docs/RELEASING.md).
//
//   pnpm release patch|minor|major|X.Y.Z [--push]
//
// patch = only fixes, minor = new features (backwards compatible), major = breaking changes
// (e.g. the app needs a newer server, or settings/library can't be read by older versions).
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
const fail = (msg) => {
  console.error(`release: ${msg}`);
  process.exit(1);
};

const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/;
const parse = (v) => {
  const m = SEMVER.exec(v);
  return m && m.slice(1).map(Number);
};
const compare = (a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2];

export function next(current, bump) {
  const [major, minor, patch] = parse(current) ?? fail(`current version ${current} isn't X.Y.Z`);
  if (bump === 'major') return `${major + 1}.0.0`;
  if (bump === 'minor') return `${major}.${minor + 1}.0`;
  if (bump === 'patch') return `${major}.${minor}.${patch + 1}`;
  if (parse(bump)) return bump;
  return fail(`unknown bump "${bump}" (patch, minor, major or X.Y.Z)`);
}

// Each file keeps its formatting: only the version string changes.
const FILES = [
  ['desktop/src-tauri/tauri.conf.json', /^(  "version": ")([^"]+)(",)$/m],
  ['desktop/package.json', /^(  "version": ")([^"]+)(",)$/m],
  ['desktop/src-tauri/Cargo.toml', /^(\[package\][^[]*?\nversion = ")([^"]+)(")$/m],
  ['desktop/Cargo.lock', /^(name = "klipt-desktop"\nversion = ")([^"]+)(")$/m],
];

function setVersion(version) {
  for (const [file, pattern] of FILES) {
    const path = join(root, file);
    const text = readFileSync(path, 'utf8');
    if (!pattern.test(text)) fail(`no version found in ${file}`);
    writeFileSync(path, text.replace(pattern, `$1${version}$3`));
  }
}

function main() {
  const args = process.argv.slice(2);
  const push = args.includes('--push');
  const bump = args.find((a) => !a.startsWith('--'));
  if (!bump) fail('usage: pnpm release patch|minor|major|X.Y.Z [--push]');

  if (git('rev-parse', '--abbrev-ref', 'HEAD') !== 'main') fail('releases are cut from main');
  if (git('status', '--porcelain', '--untracked-files=no')) fail('commit or stash your changes first');

  const conf = JSON.parse(readFileSync(join(root, FILES[0][0]), 'utf8'));
  const version = next(conf.version, bump);
  const tag = `v${version}`;

  // Installed copies only update to a higher version than any release so far.
  const released = git('tag', '--list', 'v*')
    .split('\n')
    .map((t) => parse(t.slice(1)))
    .filter(Boolean)
    .sort(compare)
    .at(-1);
  if (released && compare(parse(version), released) <= 0) {
    fail(`${version} isn't higher than the newest release v${released.join('.')}`);
  }

  let previous = '';
  try {
    previous = git('describe', '--tags', '--abbrev=0', '--match', 'v[0-9]*');
  } catch {} // no release yet
  const changes = git('log', '--no-merges', '--pretty=- %s', previous ? `${previous}..HEAD` : 'HEAD');

  setVersion(version);
  git('commit', '--quiet', '-m', `release: ${tag}`, ...FILES.map(([f]) => f));
  git('tag', '--annotate', tag, '-m', `Klipt ${version}\n\n${changes}`);
  console.log(`release: ${conf.version} → ${version}, tagged ${tag}`);

  if (push) {
    git('push', '--atomic', 'origin', 'main', tag);
    console.log(`release: pushed; CI publishes ${tag}`);
  } else {
    console.log(`release: publish it with  git push --atomic origin main ${tag}`);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
