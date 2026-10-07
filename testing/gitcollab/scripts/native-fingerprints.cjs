const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const output = process.argv[2];
if (!output) throw new Error('Provide a new evidence JSON output path.');
const target = process.env.CARGO_TARGET_DIR || path.resolve('target');
const root = process.platform === 'win32'
  ? path.join(target, 'release', 'git-collab-desktop.exe')
  : path.join(target, 'release', 'bundle', 'macos');
const files = [];
function walk(file) {
  const stat = fs.lstatSync(file);
  if (stat.isSymbolicLink()) {
    files.push({ path: path.relative(target, file).split(path.sep).join('/'), link: fs.readlinkSync(file) });
  } else if (stat.isDirectory()) {
    for (const name of fs.readdirSync(file).sort()) walk(path.join(file, name));
  } else if (stat.isFile()) {
    files.push({ path: path.relative(target, file).split(path.sep).join('/'), bytes: stat.size,
      sha256: crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') });
  }
}
walk(root);
const evidence = { sourceHead: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
  os: process.platform, arch: process.arch, files,
  note: 'Build and startup acceptance only; macOS app is not distributed or notarized.' };
fs.writeFileSync(output, JSON.stringify(evidence, null, 2), { flag: 'wx' });
console.log(JSON.stringify({ sourceHead: evidence.sourceHead, os: evidence.os, arch: evidence.arch, files: files.length }));
