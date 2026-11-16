#!/usr/bin/env python3
"""Does the record of a decision change what an agent does?

Each scenario (scenarios.py) is built three ways, and a real Claude Code
session is given the same task in each:

  none   the code and its git history, nothing else
  adr    plus docs/decisions/*.md holding the decision, and a CLAUDE.md that
         says to read the relevant ones before changing behaviour
  graph  plus `deciduous init` and the decision in the graph

The first two result files have a fourth arm, `hook`: the graph plus a
pre-edit hook that ran `deciduous why`. It measured the same as `graph` and
the hook was removed from deciduous; `--rescore` still reads those rows.

The session's working tree is then scored by the scenario's check:
followed the decision, repeated what was rejected, or something else.

    python3 bench/run.py --trials 3 --jobs 4
    python3 bench/run.py --scenarios cache --arms none,graph --trials 1 --keep

Every trial is one `claude -p` session on the account that is logged in.
Sessions run with no MCP servers and project settings only, in a directory
under the system temp directory, and may read, edit, and run `deciduous`,
`git log/show/diff/status/blame` and `ls`; nothing else.
"""

import argparse
import concurrent.futures
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from scenarios import BLIND, SCENARIOS, distractor_adrs, distractor_graph  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
ARMS = ["none", "adr", "graph"]
# Arms in old result files that can no longer be built.
PAST_ARMS = ["hook"]
REPORTED = ARMS + PAST_ARMS
ALLOWED = [
    "Read", "Edit", "Write", "Glob", "Grep",
    "Bash(deciduous *)", "Bash(git log*)", "Bash(git show*)", "Bash(git diff*)",
    "Bash(git status*)", "Bash(git blame*)", "Bash(ls*)",
]
# Eleven sessions of the first two runs wrote the file in the same Bash call
# as a `python3` check they may not run, and the whole call was refused.
# The first wording was "Nothing can be run in this session", which also
# rules out `deciduous`; one Sonnet run and the Fable run were made with it.
NO_PYTHON = (
    "\n\nThere is no Python interpreter in this session, so the code cannot be "
    "run or tested. Make the change with the edit tools and say what you did."
)
ADR_CLAUDE_MD = (
    "# Working in this repository\n\n"
    "Architecture decisions are recorded in `docs/decisions/`. Read the ones "
    "relevant to what you are changing before you change behaviour.\n"
)


def sh(args, cwd, env=None, stdin=None):
    r = subprocess.run(args, cwd=cwd, env=env, input=stdin, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"{' '.join(map(str, args))}\n{r.stdout}\n{r.stderr}")
    return r.stdout


def git(root, *args):
    return sh(["git", "-c", "user.name=bench", "-c", "user.email=bench@example.com", *args], root)


class Graph:
    """The few CLI calls a scenario needs, returning node ids."""

    def __init__(self, root, env, sha, settled):
        self.root, self.env, self.sha, self.settled = root, env, sha, settled

    def _run(self, *args):
        return sh(["deciduous", *args], self.root, self.env)

    def add(self, node_type, title, commit=None, description=None):
        args = ["add", node_type, title]
        if commit:
            args += ["--commit", commit]
        if description:
            args += ["-d", description]
        return int(re.search(r"Created node (\d+)", self._run(*args)).group(1))

    def link(self, a, b, edge_type=None, rationale=None):
        args = ["link", str(a), str(b)]
        if edge_type:
            args += ["-t", edge_type]
        if rationale:
            args += ["-r", rationale]
        self._run(*args)

    def status(self, node, status):
        self._run("status", str(node), status)

    def pivot(self, old, observation, new_title, reason):
        before = self._ids()
        self._run("archaeology", "pivot", str(old), observation, new_title, "-r", reason)
        nodes = json.loads(self._run("nodes", "--json"))
        made = [n for n in nodes if n["id"] not in before and n["node_type"] == "decision"]
        return made[0]["id"]

    def _ids(self):
        return {n["id"] for n in json.loads(self._run("nodes", "--json"))}


def build(scenario, arm, root, env, hard=False, distractors=4):
    """The scenario's repository as `arm` sees it, committed and clean."""
    root.mkdir(parents=True)
    git(root, "init", "-q", "-b", "main")
    sha = []
    commits = list(scenario["commits"])
    settled = scenario["settled"]
    avoid = scenario.get("avoid", ())
    # Fewer than asked for when the scenario leaves some out.
    distractors = len(distractor_adrs(distractors, avoid))
    if hard and "hard_last" in scenario:
        commits[-1] = scenario["hard_last"]
        settled = scenario["settled_hard"]
    # ADR files are numbered after the unrelated ones.
    adrs = {}
    for name, text in scenario["adr"].items():
        n = int(name[:4]) - 5 + distractors + 1
        text = re.sub(r"\b([56])\b", lambda m: str(int(m.group(1)) - 5 + distractors + 1), text) \
            if "upersede" in text else re.sub(r"^# 5\.", f"# {n}.", text)
        adrs[f"{n:04d}{name[4:]}"] = text.replace("@SETTLED@", settled)
    for message, files in commits:
        for path, content in files.items():
            p = root / path
            if content is None:
                git(root, "rm", "-q", path)
                continue
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
            git(root, "add", path)
        git(root, "commit", "-q", "-m", message)
        sha.append(git(root, "rev-parse", "HEAD").strip())

    if arm == "adr":
        docs = root / "docs" / "decisions"
        docs.mkdir(parents=True)
        for name, text in {**distractor_adrs(distractors, avoid), **adrs}.items():
            (docs / name).write_text(text)
        (root / "CLAUDE.md").write_text(ADR_CLAUDE_MD)
    elif arm == "graph":
        sh(["deciduous", "init", "--no-server"], root, env)
        g = Graph(root, env, sha, settled)
        distractor_graph(g, distractors, avoid)
        scenario["graph"](g)
    if arm != "none":
        git(root, "add", "-A")
        git(root, "commit", "-q", "-m", "record decisions")


def run_trial(scenario, arm, trial, workdir, model, keep, hard=False, distractors=4):
    root = Path(workdir) / f"{scenario['name']}-{arm}-{trial}" / "app-repo"
    env = dict(os.environ)
    env["PATH"] = f"{REPO / 'target' / 'release'}{os.pathsep}{env['PATH']}"
    env["DECIDUOUS_QUIET_LOCAL_ONLY"] = "1"
    env.pop("DECIDUOUS_DB_PATH", None)
    env.pop("DECIDUOUS_AGENT_LABEL", None)
    build(scenario, arm, root, env, hard, distractors)

    cmd = [
        "claude", "-p", scenario["prompt"] + NO_PYTHON,
        "--output-format", "stream-json", "--verbose",
        "--permission-mode", "acceptEdits",
        "--setting-sources", "project,local",
        "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}',
        "--allowedTools", *ALLOWED,
    ]
    if model:
        cmd += ["--model", model]
    proc = subprocess.run(cmd, cwd=root, env=env, stdin=subprocess.DEVNULL,
                          capture_output=True, text=True, timeout=900)

    tools, result, session_model = [], {}, None
    for line in proc.stdout.splitlines():
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        if ev.get("type") == "system" and ev.get("subtype") == "init":
            session_model = ev.get("model")
        elif ev.get("type") == "assistant":
            for block in ev.get("message", {}).get("content", []):
                if block.get("type") == "tool_use":
                    tools.append((block["name"], block.get("input", {})))
        elif ev.get("type") == "result":
            result = ev

    target = root / scenario["target"]
    text = target.read_text() if target.exists() else ""
    changed = git(root, "status", "--porcelain").strip() != ""
    report = result.get("result") or ""
    outcome = score(scenario, text, changed, report)

    def cmd_of(t):
        return t[1].get("command", "") if t[0] == "Bash" else ""

    first_edit = next((i for i, t in enumerate(tools) if t[0] in ("Edit", "Write", "MultiEdit")), len(tools))
    before = tools[:first_edit]
    row = {
        "scenario": scenario["name"],
        "kind": scenario["kind"],
        "arm": arm,
        "trial": trial,
        "outcome": outcome,
        "mentioned": re.search(scenario["mention"], report, re.I) is not None,
        # Did the session go looking before its first edit?
        # What it looked at, in order, so a number can be traced to a session.
        "before_edit": [f"{t[0]} {cmd_of(t) or t[1].get('file_path') or t[1].get('pattern') or ''}".strip()[:120]
                        for t in before],
        "asked_graph": any(re.search(r"(^|[;&|]\s*)deciduous\b", cmd_of(t)) for t in before),
        "read_adrs": any("docs/decisions" in json.dumps(t[1]) for t in before),
        "read_git_history": any(re.search(r"(^|[;&|]\s*)git (log|show|blame)", cmd_of(t)) for t in before),
        "turns": result.get("num_turns"),
        "seconds": round((result.get("duration_ms") or 0) / 1000, 1),
        "cost_usd": result.get("total_cost_usd"),
        "model": session_model,
        "error": None if result else (proc.stderr or proc.stdout)[-400:],
        "report": report,
        "target": text,
    }
    if not keep:
        shutil.rmtree(root.parent, ignore_errors=True)
    return row


# "I haven't changed app/customers.py, because ...", as the report's opening.
# Anywhere later it is "I haven't run it", which every session says.
DID_NOT = re.compile(
    r"\s*I (haven't|have not|didn't|did not) (changed?|added?|made?|edit(ed)?|implement(ed)?|touch(ed)?)\b", re.I)


def score(scenario, text, changed, report=""):
    """`not-landed`: the session ended with the target as it found it while
    reporting a change; it wrote the file in the same Bash call as a check it
    was not allowed to run. `declined`: the target is as it was found and the
    report says so and cites the earlier decision: the session read the
    record and stopped to ask."""
    outcome = scenario["check"](text) if changed else "not-landed"
    if outcome == "not-landed" and DID_NOT.match(report) and re.search(scenario["mention"], report, re.I):
        return "declined"
    return outcome


def rescore(path):
    """Score a results file again with the scorers as they are now."""
    doc = json.loads(Path(path).read_text())
    by_name = {s["name"]: s for s in SCENARIOS}
    for r in doc["rows"]:
        if r["outcome"] == "error":
            continue
        s = by_name[r["scenario"]]
        last = s["hard_last"] if doc.get("hard") and "hard_last" in s else s["commits"][-1]
        start = None
        for _, files in s["commits"][:-1] + [last]:
            start = files.get(s["target"], start)
        api_error = r["report"].startswith("API Error")
        r["outcome"] = "error" if api_error else score(s, r["target"], r["target"] != start, r["report"])
    Path(path).write_text(json.dumps(doc, indent=1) + "\n")
    print(summarise(doc["rows"]))


def summarise(rows):
    lines = []
    scenarios = sorted({r["scenario"] for r in rows}, key=[s["name"] for s in SCENARIOS].index)
    arms = [a for a in REPORTED if any(r["arm"] == a for r in rows)]

    def cell(sel):
        # A session whose edit never landed, or that never ran, says
        # nothing about what the agent would have done.
        sel = [r for r in sel if r["outcome"] not in ("not-landed", "error", "no-change")]
        n = len(sel)
        f = sum(r["outcome"] == "followed" for r in sel)
        rep = sum(r["outcome"] == "repeated" for r in sel)
        dec = sum(r["outcome"] == "declined" for r in sel)
        asked = f", {dec} declined" if dec else ""
        return f"{f}/{n} followed, {rep} repeated{asked}" if n else "-"

    lines.append("| scenario | " + " | ".join(arms) + " |")
    lines.append("|---|" + "---|" * len(arms))
    for s in scenarios:
        kind = next(r["kind"] for r in rows if r["scenario"] == s)
        cells = [cell([r for r in rows if r["scenario"] == s and r["arm"] == a]) for a in arms]
        lines.append(f"| {s} ({kind}) | " + " | ".join(cells) + " |")
    lines.append("| **all** | " + " | ".join(cell([r for r in rows if r["arm"] == a]) for a in arms) + " |")
    lost = sum(r["outcome"] in ("not-landed", "error", "no-change") for r in rows)
    lines.append("")
    lines.append(f"{lost} of {len(rows)} sessions left out: the edit never landed, or the session did not run.")
    lines.append("")
    lines.append("| arm | looked before editing | mentioned the decision | median turns | median seconds | total cost |")
    lines.append("|---|---|---|---|---|---|")
    for a in arms:
        sel = [r for r in rows if r["arm"] == a]
        looked = sum(r["asked_graph"] or r["read_adrs"] or r["read_git_history"] for r in sel)
        said = sum(r["mentioned"] for r in sel)

        def median(key):
            v = sorted(r[key] for r in sel if r[key] is not None)
            return v[len(v) // 2] if v else "-"

        cost = sum(r["cost_usd"] or 0 for r in sel)
        lines.append(f"| {a} | {looked}/{len(sel)} | {said}/{len(sel)} | {median('turns')} | {median('seconds')} | ${cost:.2f} |")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--scenarios", default=",".join(s["name"] for s in SCENARIOS),
                    help="comma-separated names, or `blind` for " + ",".join(BLIND))
    ap.add_argument("--arms", default=",".join(ARMS))
    ap.add_argument("--trials", type=int, default=3)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--model", default=None, help="passed to claude --model; default is the account's")
    ap.add_argument("--out", default=None, help="results JSON (default bench/results/<timestamp>.json)")
    ap.add_argument("--keep", action="store_true", help="keep each trial's repository")
    ap.add_argument("--hard", action="store_true",
                    help="the approach that was decided on is not used anywhere in the code yet")
    ap.add_argument("--distractors", type=int, default=4,
                    help="unrelated decisions each repository also records (up to 60)")
    ap.add_argument("--rescore", metavar="FILE", help="score a results file again and exit")
    ap.add_argument("--build-only", action="store_true", help="build every repository, run no session")
    args = ap.parse_args()

    if args.rescore:
        rescore(args.rescore)
        return
    if not (REPO / "target" / "release" / "deciduous").exists():
        sys.exit("target/release/deciduous is missing: cargo build --release")
    wanted = BLIND if args.scenarios == "blind" else args.scenarios.split(",")
    scenarios = [s for s in SCENARIOS if s["name"] in wanted]
    arms = [a for a in ARMS if a in args.arms.split(",")]
    workdir = tempfile.mkdtemp(prefix="deciduous-bench-")

    if args.build_only:
        env = dict(os.environ)
        env["PATH"] = f"{REPO / 'target' / 'release'}{os.pathsep}{env['PATH']}"
        env["DECIDUOUS_QUIET_LOCAL_ONLY"] = "1"
        for s in scenarios:
            for a in arms:
                build(s, a, Path(workdir) / f"{s['name']}-{a}" / "app-repo", env, args.hard, args.distractors)
        print(workdir)
        return

    jobs = [(s, a, t) for s in scenarios for a in arms for t in range(1, args.trials + 1)]
    print(f"{len(jobs)} sessions in {workdir}", file=sys.stderr)
    rows = []
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        futures = {pool.submit(run_trial, s, a, t, workdir, args.model, args.keep, args.hard, args.distractors): (s["name"], a, t) for s, a, t in jobs}
        for f in concurrent.futures.as_completed(futures):
            name, arm, trial = futures[f]
            try:
                row = f.result()
            except Exception as e:  # a session that could not run is a row, not a crash
                row = {"scenario": name, "kind": next(s["kind"] for s in SCENARIOS if s["name"] == name),
                       "arm": arm, "trial": trial, "outcome": "error", "mentioned": False,
                       "asked_graph": False, "read_adrs": False, "read_git_history": False,
                       "turns": None, "seconds": None, "cost_usd": None, "model": None,
                       "error": str(e)[-400:], "report": "", "target": ""}
            rows.append(row)
            print(f"{name:8} {arm:6} {trial}  {row['outcome']}", file=sys.stderr)

    rows.sort(key=lambda r: (r["scenario"], REPORTED.index(r["arm"]), r["trial"]))
    version = sh(["claude", "--version"], REPO).strip()
    out = Path(args.out) if args.out else REPO / "bench" / "results" / (
        datetime.datetime.now().strftime("%Y-%m-%dT%H%M") + ".json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({
        "claude_code": version,
        "deciduous": sh(["git", "rev-parse", "--short", "HEAD"], REPO).strip(),
        "models": sorted({r["model"] for r in rows if r["model"]}),
        "trials": args.trials,
        "hard": args.hard,
        "distractors": args.distractors,
        "prompt_suffix": NO_PYTHON,
        "rows": rows,
    }, indent=1) + "\n")
    print(summarise(rows))
    print(f"\n{out}", file=sys.stderr)
    if not args.keep:
        shutil.rmtree(workdir, ignore_errors=True)


if __name__ == "__main__":
    main()
