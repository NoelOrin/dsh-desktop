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
  return run(`git log ${range} --pretty=format:%H%x00%s`)
    .split("\n")
    .map((line) => {
      const [sha, message] = line.split("\0");
      return { sha: sha.trim(), message: (message ?? "").trim() };
    })
    .filter((entry) => entry.message);
}

function pushReleaseWithTag(version) {
  execSync(`git tag v${version}`, { stdio: "inherit" });
  execSync(`git push --atomic origin HEAD:release v${version}`, { stdio: "inherit" });
}

function messageOf(entry) {
  return typeof entry === "string" ? entry : entry.message;
}

function resolveLevel(messages) {
  let level = "patch";
  for (const entry of messages) {
    const message = messageOf(entry);
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

function prSuffix(entry) {
  if (!entry.sha) return "";
  if (/\(#\d+\)/.test(entry.message)) return "";
  if (!process.env.GH_TOKEN || !process.env.GITHUB_REPOSITORY) return "";

  try {
    const number = run(
      `gh api "repos/${process.env.GITHUB_REPOSITORY}/commits/${entry.sha}/pulls" --jq '.[0].number'`,
    );
    return /^\d+$/.test(number) ? ` (#${number})` : "";
  } catch {
    return "";
  }
}

function extractUnreleased(existing) {
  const lines = existing.split("\n");
  const start = lines.findIndex((line) => line.trim() === "## [Unreleased]");
  if (start === -1) return { notes: [], remaining: existing };

  let end = lines.length;
  for (let i = start + 1; i < lines.length; i += 1) {
    if (lines[i].startsWith("## [")) {
      end = i;
      break;
    }
  }

  const notes = lines
    .slice(start + 1, end)
    .map((line) => line.trim())
    .filter((line) => line.startsWith("- ") || line.startsWith("* "));
  const remaining = [...lines.slice(0, start), ...lines.slice(end)]
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  return { notes, remaining: remaining ? `${remaining}\n` : "# Changelog\n" };
}

function updateChangelog(version, messages) {
  const date = new Date().toISOString().slice(0, 10);
  const entries = messages.map((entry) =>
    typeof entry === "string" ? { message: entry, sha: "" } : entry,
  );
  const breaking = entries.filter((entry) =>
    /BREAKING CHANGE|^[a-z]+(\([^)]*\))?!:/.test(entry.message),
  );
  const features = entries.filter(
    (entry) => /^feat(\([^)]*\))?:/.test(entry.message) && !breaking.includes(entry),
  );
  const fixes = entries.filter(
    (entry) => /^fix(\([^)]*\))?:/.test(entry.message) && !breaking.includes(entry),
  );
  const other = entries.filter(
    (entry) =>
      /^[a-z]+(\([^)]*\))?:/.test(entry.message) &&
      !breaking.includes(entry) &&
      !features.includes(entry) &&
      !fixes.includes(entry),
  );

  const sections = [];
  if (breaking.length > 0) sections.push(["Breaking Changes", breaking]);
  if (features.length > 0) sections.push(["Features", features]);
  if (fixes.length > 0) sections.push(["Bug Fixes", fixes]);
  if (other.length > 0) sections.push(["Other", other]);

  const existingRaw = existsSync(changelogPath)
    ? readFileSync(changelogPath, "utf8")
    : "# Changelog\n";
  const { notes: unreleasedNotes, remaining: existing } = extractUnreleased(existingRaw);
  if (sections.length === 0 && unreleasedNotes.length === 0) return;

  const lines = [`## [${version}] - ${date}`, ""];
  for (const [title, items] of sections) {
    lines.push(
      `### ${title}`,
      "",
      ...items.map((entry) => `- ${entry.message}${prSuffix(entry)}`),
      "",
    );
  }
  if (unreleasedNotes.length > 0) {
    lines.push("### Pending Updates", "", ...unreleasedNotes, "");
  }

  const block = lines.join("\n");
  if (existing.includes(`## [${version}]`)) {
    console.log(`changelog already contains ${version}, skip update`);
    return;
  }
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
  pushReleaseWithTag(current);
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
pushReleaseWithTag(next);

setOutput("version", next);
setOutput("bumped", "true");
