// Offline check: no Homebrew install, downloads, shell evaluation, or file writes.
// node scripts/docs/test-homebrew.mjs [--tap-formula /path/to/Formula/deciduous.rb]
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const args = process.argv.slice(2);
assert(args.length === 0 || (args.length === 2 && args[0] === '--tap-formula'),
  'Usage: node scripts/docs/test-homebrew.mjs [--tap-formula /path/to/deciduous.rb]');

const workflow = fs.readFileSync(path.join(root, '.github/workflows/release.yml'), 'utf8');
const match = workflow.match(/^( +)cat > deciduous\.rb << FORMULA\r?\n([\s\S]*?)^\1FORMULA\s*$/m);
assert(match, 'The release workflow must contain its formula heredoc');
const indentation = match[1];
const template = match[2].split('\n').map(line => {
  assert(!line.trim() || line.startsWith(indentation), 'Unexpected formula heredoc indentation');
  return line.startsWith(indentation) ? line.slice(indentation.length) : '';
}).join('\n');

// The unquoted shell heredoc expands these release inputs. Reject shell command
// substitutions instead of executing the workflow just to test its output.
assert(!template.includes('$(') && !template.includes('`'),
  'Formula heredoc must not execute shell command substitutions');
const platforms = ['darwin-arm64', 'darwin-amd64', 'linux-arm64', 'linux-amd64'];
const variables = platforms.map(platform => `SHA_${platform.toUpperCase().replaceAll('-', '_')}`);
const substitutions = [...template.matchAll(/\$\{([^}]+)\}/g)].map(item => item[1]);
assert.deepEqual(substitutions, ['VERSION', ...variables], 'Release inputs changed or are missing');

function render(values) {
  return template.replace(/\$\{([^}]+)\}/g, (_, name) => {
    assert(Object.hasOwn(values, name), `Missing release input: ${name}`);
    return values[name];
  });
}

function rubySyntax(formula) {
  const result = spawnSync('ruby', ['-c'], {input: formula, encoding: 'utf8'});
  assert.ifError(result.error);
  assert.equal(result.status, 0, `Invalid Ruby formula:\n${result.stderr}`);
}

function caveats(formula) {
  const body = formula.match(/^  def caveats\r?\n    <<~EOS\r?\n([\s\S]*?)^    EOS\r?\n  end$/m)?.[1];
  assert(body, 'Formula must define caveats with its setup message');
  return body.split('\n').map(line => line.startsWith('      ') ? line.slice(6) : line).join('\n');
}

const fixtures = {VERSION: '9.8.7'};
variables.forEach((name, index) => { fixtures[name] = String(index + 1).repeat(64); });
const generated = render(fixtures);
rubySyntax(generated);
assert.match(generated, /^  homepage "https:\/\/deciduous\.dev\/"$/m);
assert.match(generated, /^  version "9\.8\.7"$/m);
assert.deepEqual([...generated.matchAll(/^\s+sha256 "([a-f0-9]{64})"$/gm)].map(item => item[1]),
  variables.map(name => fixtures[name]), 'Checksums must come from their matching release inputs');
assert.deepEqual([...generated.matchAll(/^\s+url "([^"]+)"$/gm)].map(item => item[1]),
  platforms.map(platform => `https://github.com/notactuallytreyanastasio/deciduous/releases/download/v#{version}/deciduous-${platform}`),
  'Keep the four supported release artifacts and Ruby version interpolation');

const message = caveats(generated);
for (const required of [
  'https://deciduous.dev/tutorial/local-postgres.html',
  'https://deciduous.dev/tutorial/upgrading.html',
  'did NOT create a shared graph server',
  'CLI writes and stdio MCP still use local SQLite',
  'does not migrate your existing data',
  'brew info deciduous',
]) assert(message.includes(required), `Missing onboarding caveat: ${required}`);
assert(!/\bdeciduous setup\b/.test(message),
  'Released 1.0.0 caveats must not require the newer source-only setup command');

if (args.length) {
  const tap = fs.readFileSync(path.resolve(args[1]), 'utf8');
  rubySyntax(tap);
  const version = tap.match(/^  version "([^"]+)"$/m)?.[1];
  const checksums = [...tap.matchAll(/^\s+sha256 "([a-f0-9]{64})"$/gm)].map(item => item[1]);
  assert(version && checksums.length === variables.length, 'Tap must specify a version and four SHA-256 values');
  const values = {VERSION: version};
  variables.forEach((name, index) => { values[name] = checksums[index]; });
  assert.equal(caveats(tap), message, 'Tap and release generator onboarding messages differ');
  assert.equal(tap.trimEnd(), render(values).trimEnd(),
    'Tap formula must match the release generator using its version and checksums');
}

console.log(`PASS: generated Ruby, four release artifacts/checksum inputs, onboarding caveats${args.length ? ', and exact tap parity' : ''}.`);
