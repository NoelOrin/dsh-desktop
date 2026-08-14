import { execSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";

const packagePath = new URL("../../package.json", import.meta.url);
const changelogPath = new URL("../../CHANGELOG.md", import.meta.url);

const readVersion = () => JSON.parse(readFileSync(packagePath, "utf8")).version;

function setOutput(key, value) {
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, `${key}=${value}\n`);
  }
  console.log(`${key}=${value}`);
}

function run(command) {
  return execSync(command, { encoding: "utf8" }).trim();
}

function previousTag() {
  try {
    return run("git describe --tags --abbrev=0 HEAD^ 2>/dev/null || true");
  } catch {
    return "";
  }
}

function commitsSince(tag) {
  const range = tag ? `${tag}..HEAD` : "HEAD";
  return run(`git log ${range} --pretty=%s`)
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function resolveLevel(messages) {
  let level = "patch";
  for (const message of messages) {
    if (/BREAKING CHANGE|^[a-z]+(\([^)]*\))?!:/.test(message)) return "major";
    if (/^feat(\([^)]*\))?:/.test(message)) level = "minor";
  }
  return level;
}

function increment(version, level) {
  const match = version.match(/^(\d+)\.(\d+)\.(\d+)/);
  const base = match ? [Number(match[1]), Number(match[2]), Number(match[3])] : [0, 1, 0];
  if (level === "major") return `${base[0] + 1}.0.0`;
  if (level === "minor") return `${base[0]}.${base[1] + 1}.0`;
  return `${base[0]}.${base[1]}.${base[2] + 1}`;
}

function updateChangelog(version, messages) {
  const date = new Date().toISOString().slice(0, 10);
  const breaking = messages.filter((message) =>
    /BREAKING CHANGE|^[a-z]+(\([^)]*\))?!:/.test(message),
  );
  const features = messages.filter(
    (message) => /^feat(\([^)]*\))?:/.test(message) && !breaking.includes(message),
  );
  const fixes = messages.filter(
    (message) => /^fix(\([^)]*\))?:/.test(message) && !breaking.includes(message),
  );
  const other = messages.filter(
    (message) =>
      /^[a-z]+(\([^)]*\))?:/.test(message) &&
      !breaking.includes(message) &&
      !features.includes(message) &&
      !fixes.includes(message),
  );

  const sections = [];
  if (breaking.length > 0) sections.push(["Breaking Changes", breaking]);
  if (features.length > 0) sections.push(["Features", features]);
  if (fixes.length > 0) sections.push(["Bug Fixes", fixes]);
  if (other.length > 0) sections.push(["Other", other]);
  if (sections.length === 0) return;

  const lines = [`## [${version}] - ${date}`, ""];
  for (const [title, items] of sections) {
    lines.push(`### ${title}`, "", ...items.map((message) => `- ${message}`), "");
  }
  const block = lines.join("\n");
  const existing = existsSync(changelogPath)
    ? readFileSync(changelogPath, "utf8")
    : "# Changelog\n";
  const next = existing.startsWith("# Changelog")
    ? existing.replace("# Changelog\n", `# Changelog\n\n${block}`)
    : `# Changelog\n\n${block}${existing}`;
  writeFileSync(changelogPath, next);
}

const isReleasePush =
  process.env.GITHUB_EVENT_NAME === "push" && process.env.GITHUB_REF === "refs/heads/release";
const isAutoBumpCommit = (process.env.HEAD_MESSAGE ?? "").startsWith("chore: bump version");

if (!isReleasePush || isAutoBumpCommit) {
  setOutput("version", readVersion());
  setOutput("bumped", "false");
  process.exit(0);
}

const current = readVersion();
const previous = previousTag();
const currentTag = run("git describe --tags --exact-match HEAD 2>/dev/null || true");
if (!previous) {
  if (currentTag) {
    setOutput("version", current);
    setOutput("bumped", "false");
    process.exit(0);
  }

  updateChangelog(current, commitsSince(""));
  execSync('git config user.name "github-actions[bot]"', { stdio: "inherit" });
  execSync('git config user.email "41898282+github-actions[bot]@users.noreply.github.com"', {
    stdio: "inherit",
  });
  execSync("git add CHANGELOG.md", { stdio: "inherit" });
  execSync(`git commit -m "chore: bump version to ${current}"`, {
    stdio: "inherit",
  });
  execSync("git push origin HEAD:release", { stdio: "inherit" });
  execSync(`git tag v${current}`, { stdio: "inherit" });
  execSync(`git push origin v${current}`, { stdio: "inherit" });
  setOutput("version", current);
  setOutput("bumped", "true");
  process.exit(0);
}

const messages = commitsSince(previous);
if (messages.length === 0) {
  setOutput("version", current);
  setOutput("bumped", "false");
  process.exit(0);
}

const next = increment(current, resolveLevel(messages));
const pkg = JSON.parse(readFileSync(packagePath, "utf8"));
pkg.version = next;
writeFileSync(packagePath, `${JSON.stringify(pkg, null, 2)}\n`);
updateChangelog(next, messages);

execSync('git config user.name "github-actions[bot]"', { stdio: "inherit" });
execSync('git config user.email "41898282+github-actions[bot]@users.noreply.github.com"', {
  stdio: "inherit",
});
execSync("git add package.json CHANGELOG.md", { stdio: "inherit" });
execSync(`git commit -m "chore: bump version to ${next}"`, {
  stdio: "inherit",
});
execSync("git push origin HEAD:release", { stdio: "inherit" });
execSync(`git tag v${next}`, { stdio: "inherit" });
execSync(`git push origin v${next}`, { stdio: "inherit" });

setOutput("version", next);
setOutput("bumped", "true");
