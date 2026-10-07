// Start an empty, isolated production application. This is a startup check,
// not proof of physical tray, notifications, Dock interactions or installation.
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const binary = path.resolve(process.argv[2] || '');
const data = process.env.GITCOLLAB_DATA_DIR;
const report = process.env.GITCOLLAB_TEST_REPORT;
if (!process.argv[2] || !fs.existsSync(binary) || !data || !report) {
  throw new Error('Provide the production executable, GITCOLLAB_DATA_DIR and GITCOLLAB_TEST_REPORT.');
}
if (fs.existsSync(data) || fs.existsSync(report)) throw new Error('Smoke paths must be fresh; preserve prior state and evidence.');
fs.mkdirSync(data, { recursive: true });
fs.mkdirSync(path.dirname(report), { recursive: true });
const child = spawn(binary, [], { windowsHide: true, stdio: 'ignore', env: process.env });
let exited = false;
let failure;
child.on('error', err => { failure = err; });
child.on('exit', () => { exited = true; });
(async () => {
  const deadline = Date.now() + 60000;
  try {
    while (Date.now() < deadline) {
      if (failure) throw failure;
      if (exited) throw new Error('Production app exited before ready.');
      const events = fs.existsSync(report) ? fs.readFileSync(report, 'utf8').split(/\r?\n/) : [];
      if (events.includes('startup-error')) throw new Error('Production app reported a startup error.');
      if (events.includes('ready')) {
        const state = JSON.parse(fs.readFileSync(path.join(data, 'state.json'), 'utf8'));
        if (state.snapshot.projects.length || state.snapshot.messages.length || state.snapshot.systemNotifications) {
          throw new Error('Fresh app must start empty with notifications disabled.');
        }
        const evidence = { status: 'passed', os: process.platform, arch: process.arch,
          checks: ['production-startup', 'fresh-empty-store', 'notifications-off'],
          limitations: ['no-physical-ui-check', 'no-installation', 'fixture-process-terminated-after-ready'] };
        fs.writeFileSync(`${report}.json`, JSON.stringify(evidence, null, 2), { flag: 'wx' });
        console.log(JSON.stringify(evidence));
        return;
      }
      await new Promise(resolve => setTimeout(resolve, 250));
    }
    throw new Error('Production application did not become ready within 60 seconds.');
  } finally {
    // The fixture has no projects or accepted Git work. Native drain behavior
    // is covered separately by the native unit tests, not by this termination.
    if (!exited) child.kill();
  }
})().catch(err => { console.error(err.message); process.exitCode = 1; });
