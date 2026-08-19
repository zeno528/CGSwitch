import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const increment = process.argv[2];
if (!new Set(["patch", "minor", "major"]).has(increment)) {
  throw new Error("用法：node scripts/bump-version.mjs <patch|minor|major>");
}

const versionFile = new URL("../VERSION", import.meta.url);
const current = readFileSync(versionFile, "utf8").trim();
const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(current);
if (!match) {
  throw new Error(`VERSION 必须是稳定 SemVer（x.y.z），当前为：${current}`);
}

let [major, minor, patch] = match.slice(1).map(Number);
if (increment === "major") {
  major += 1;
  minor = 0;
  patch = 0;
} else if (increment === "minor") {
  minor += 1;
  patch = 0;
} else {
  patch += 1;
}

const next = `${major}.${minor}.${patch}`;
writeFileSync(versionFile, `${next}\n`);
execFileSync(process.execPath, [fileURLToPath(new URL("./sync-version.mjs", import.meta.url))], {
  stdio: "inherit",
});
console.log(`版本号已从 ${current} 更新为 ${next}`);
