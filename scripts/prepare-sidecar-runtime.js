const fs = require('fs');
const path = require('path');

const repoRoot = path.resolve(__dirname, '..');
const sidecarRoot = path.join(repoRoot, 'sidecar');
const venvRoot = path.join(sidecarRoot, '.venv');
const runtimeRoot = path.join(sidecarRoot, '.python-runtime');
const pyvenvConfig = path.join(venvRoot, 'pyvenv.cfg');

function ensurePathExists(target, label) {
  if (!fs.existsSync(target)) {
    throw new Error(`${label} was not found at ${target}`);
  }
}

function copyDirectory(source, destination) {
  if (fs.existsSync(destination)) {
    fs.rmSync(destination, { force: true, recursive: true });
  }

  fs.cpSync(source, destination, {
    recursive: true,
    force: true,
  });
}

function folderBytes(target) {
  let totalBytes = 0;
  const stack = [target];

  while (stack.length > 0) {
    const current = stack.pop();
    const stats = fs.lstatSync(current);

    if (stats.isDirectory()) {
      for (const entry of fs.readdirSync(current)) {
        stack.push(path.join(current, entry));
      }
      continue;
    }

    if (stats.isFile()) {
      totalBytes += stats.size;
    }
  }

  return totalBytes;
}

try {
  ensurePathExists(venvRoot, '.venv virtual environment');
  ensurePathExists(pyvenvConfig, 'Python virtualenv metadata');

  copyDirectory(venvRoot, runtimeRoot);

  const pythonCandidates = [
    path.join(runtimeRoot, 'Scripts', 'python.exe'),
    path.join(runtimeRoot, 'bin', 'python'),
  ];

  const pythonExe = pythonCandidates.find((candidate) => fs.existsSync(candidate));
  if (!pythonExe) {
    throw new Error(`Could not find Python executable in ${runtimeRoot}.`);
  }

  const sizeMb = Math.round((folderBytes(runtimeRoot) / (1024 * 1024)) * 100) / 100;
  console.log(`Staged portable Python runtime from ${venvRoot}`);
  console.log(`Destination: ${runtimeRoot}`);
  console.log(`Runtime size: ${sizeMb} MB`);
  console.log(`Python executable: ${pythonExe}`);
} catch (err) {
  console.error(err.message);
  process.exit(1);
}
