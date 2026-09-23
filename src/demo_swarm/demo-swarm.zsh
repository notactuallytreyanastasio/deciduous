#!/bin/zsh
# deciduous demo-swarm: one Opus boss and up to nine Sonnet workers build one
# Tetris together, in iTerm2 or Ghostty panes, over one decision graph.
#
#   deciduous demo-swarm [--workers N] [--dry-run] [--ask] [--dir PATH]
#
# The binary embeds this file and runs `launch`. `launch` builds the arena
# repository, copies this file into it, and opens the window; each pane then
# runs `run <name>` under `script -r`, so every pane is recorded with
# timestamps for a later replay. `run boss` plays the tour first.
emulate -L zsh
setopt no_unset pipe_fail extended_glob
# No err_exit: arithmetic tests that evaluate to 0 would end the tour.
# Steps that must not fail say so with `|| die`.
zmodload zsh/zselect zsh/datetime

SELF=${0:A}

# ---------------------------------------------------------------- palette
e=$'\e' CR=$'\r' Z=''   # Z: the empty string padding flags fill from
RST="${e}[0m" B="${e}[1m" DIM="${e}[2m" IT="${e}[3m"
rgb() { print -rn -- "${e}[38;2;$1;$2;$3m" }
C_I=$(rgb 0 220 235)  C_O=$(rgb 245 210 40)  C_T=$(rgb 175 95 235)
C_S=$(rgb 90 215 95)  C_Z=$(rgb 240 80 80)   C_J=$(rgb 80 130 245)
C_L=$(rgb 245 150 40) C_P=$(rgb 245 120 190) C_M=$(rgb 180 230 60)
C_LEAF=$(rgb 232 140 60) C_GREY=$(rgb 120 120 130) C_TXT=$(rgb 225 225 230)
C_BOSS=$(rgb 255 190 80)
WCOL=("$C_I" "$C_O" "$C_T" "$C_S" "$C_Z" "$C_J" "$C_L" "$C_P" "$C_M")

# ------------------------------------------------------------- the crew
ROLE=(engine rotation input render scoring audio shell effects qa)
SHORT=(engine rotate input render score audio shell juice qa)
OWNS=("src/engine.js" "src/rotation.js" "src/input.js" "src/render.js"
      "src/scoring.js" "src/audio.js" "index.html (after the skeleton), style.css, src/shell.js"
      "src/effects.js" "tests/")
DOES=("board, pieces, 7-bag, gravity, lock delay, the game state"
      "SRS rotation, wall kicks for I and JLSTZ, T-spin detection"
      "keyboard, DAS and ARR, soft and hard drop, pause, remapping"
      "canvas: board, ghost piece, hold, next queue, HUD"
      "guideline scoring, combos, back-to-back, levels, gravity curve"
      "WebAudio synth: every sound and the music, no audio files"
      "the page: menus, pause screen, game over, restart, high scores"
      "line-clear animation, particles, screen shake, level-up flash"
      "node test runner, tests for engine, rotation and scoring, a browser smoke check")

die() { print -u2 -- "demo-swarm: $*"; exit 1 }

# ------------------------------------------------------------ timing / keys
FAST=0
nap() { (( FAST )) && return 0; zselect -t $1 || true; poll_key }   # centiseconds
poll_key() {
  local k
  if [[ -t 0 ]] && read -s -t 0 -k 1 k 2>/dev/null; then FAST=1; fi
  return 0
}
type_out() {           # type_out <color> <text>  (text has no escapes)
  local c=$1 s=$2 i
  print -rn -- "$c"
  if (( FAST )); then print -r -- "$s$RST"; return; fi
  for (( i = 1; i <= ${#s}; i++ )); do
    print -rn -- "${s[i]}"
    [[ ${s[i]} == [.,:\;] ]] && nap 6 || nap 1
  done
  print -r -- "$RST"
}
say()  { print -rn -- "$MARGIN"; type_out "$C_TXT" "$1" }
dim()  { print -rn -- "$MARGIN"; type_out "$DIM$C_GREY" "$1" }
head_() {
  print; print -rn -- "$MARGIN"; type_out "$B$C_LEAF" "$1"
  print -r -- "$MARGIN$C_GREY${(l:${#1}::─:)Z}$RST"
}
beat() { nap ${1:-60} }

# ================================================================= launch
launch() {
  local workers=9 dry=0 ask=0 nowin=0 base="$HOME/deciduous-swarm" dir="" term=""
  while (( $# )); do
    case $1 in
      --workers) workers=${2:-}; shift 2 ;;
      --dry-run) dry=1; shift ;;
      --ask)     ask=1; shift ;;
      --dir)     dir=${2:-}; shift 2 ;;
      --terminal) term=${2:-}; shift 2 ;;
      --no-window) nowin=1; dry=1; shift ;;
      -h|--help) usage; return 0 ;;
      *) usage >&2; return 2 ;;
    esac
  done
  [[ $workers == <1-9> ]] || die "--workers takes 1 to 9, not '$workers'"

  [[ $(uname) == Darwin ]] || die "this needs macOS: it drives iTerm2 or Ghostty through AppleScript"
  if [[ -z $term ]]; then
    case ${TERM_PROGRAM:-} in
      iTerm.app) term=iterm ;;
      ghostty)   term=ghostty ;;
      *) print -u2 -- "demo-swarm is an easter egg for iTerm2 and Ghostty, and this terminal is '${TERM_PROGRAM:-unknown}'."
         print -u2 -- "Run it from an iTerm2 or Ghostty window."
         return 1 ;;
    esac
  fi
  [[ $term == (iterm|ghostty) ]] || die "--terminal is iterm or ghostty, not '$term'"

  local bin
  for bin in git claude osascript script uuidgen; do
    command -v $bin >/dev/null || die "$bin is not on PATH"
  done
  if (( ! dry )); then
    claude mcp get deciduous >/dev/null 2>&1 ||
      die "no 'deciduous' MCP server in Claude Code; the swarm shares its plan through it. Set one up with 'deciduous remote login' and 'claude mcp add'."
  fi

  local tag=$(strftime %m%d-%H%M $EPOCHSECONDS)
  [[ -n $dir ]] || dir="$base/swarm-$tag"
  dir=${dir:a}
  [[ ! -e $dir ]] || die "$dir already exists; pass --dir for another place"
  local ws=${dir:t}
  mkdir -p $dir/.swarm/rec && cd $dir || die "cannot create $dir"

  # ---- the repository every session works in
  git init -q -b main || die "git init failed in $dir"
  write_arena_files $tag $ws $workers
  print -r -- $'.swarm/\ncrew/' > .gitignore
  git add CLAUDE.md BOSS.md ROSTER.md .gitignore
  git -c user.name=demo-swarm -c user.email=demo-swarm@deciduous.dev \
    commit -q -m "arena: the rules, the roster, and the boss's playbook" || die "first commit failed"
  local i
  for (( i = 1; i <= workers; i++ )); do
    git worktree add -q crew/w$i -b w$i main || die "worktree crew/w$i failed"
  done

  # ---- per-run state the panes read
  cp $SELF .swarm/demo-swarm.zsh || die "cannot copy the script into the arena"
  print -r -- '{"crossSessionInbound":"accept"}' > .swarm/settings.json
  mkdir -p .swarm/online
  {
    print -r -- "TAG=${(q)tag}"
    print -r -- "WS=${(q)ws}"
    print -r -- "N=$workers"
    print -r -- "DRY=$dry"
    print -r -- "ASK=$ask"
    print -r -- "TERMKIND=$term"
    print -r -- "SID_boss=$(uuidgen | tr A-Z a-z)"
    for (( i = 1; i <= workers; i++ )); do
      print -r -- "SID_w$i=$(uuidgen | tr A-Z a-z)"
    done
  } > .swarm/env
  write_manifest $dir

  # ---- the window
  local -a names cmds
  names=(boss)
  for (( i = 1; i <= workers; i++ )); do names+=(w$i); done
  local n
  for n in $names; do
    local cwd=$dir; [[ $n == boss ]] || cwd=$dir/crew/$n
    cmds+=("cd ${(q)cwd} && clear && script -q -F -r ${(q)dir}/.swarm/rec/$n.rec zsh ${(q)dir}/.swarm/demo-swarm.zsh run $n")
  done
  if (( nowin )); then
    print -r -- "demo-swarm: arena built at $dir, no window opened. Each pane would run:"
    print -rl -- "  "${^cmds}
    return 0
  fi
  open_window $term $workers $dir "${cmds[@]}"

  local tname=iTerm2; [[ $term == ghostty ]] && tname=Ghostty
  print -r -- "demo-swarm: opened a $tname window with the boss (Opus) and $workers Sonnet worker(s)."
  print -r -- "  arena      $dir"
  print -r -- "  workspace  $ws   (the deciduous graph they share)"
  print -r -- "  sessions   boss-$tag, w1-$tag .. w$workers-$tag"
  print -r -- "  recording  $dir/.swarm/rec   (every pane, timestamped; manifest.json says what is where)"
  (( dry )) && print -r -- "  dry run: the tour plays, and the panes print the claude command instead of running it."
  print -r -- "The tour plays in the boss pane; press any key there to fast-forward."
}

usage() {
  print -r -- "usage: deciduous demo-swarm [--workers N] [--dry-run] [--ask] [--dir PATH]"
  print -r -- ""
  print -r -- "  --workers N  1 to 9 Sonnet workers (default 9); the boss is always Opus"
  print -r -- "  --dry-run    build the arena and the window, play the tour, start no sessions"
  print -r -- "  --ask        keep Claude Code's permission prompts in every pane"
  print -r -- "  --dir PATH   where the arena goes (default ~/deciduous-swarm/swarm-MMDD-HHMM)"
  print -r -- "  --terminal iterm|ghostty   skip detection (for testing the other layout)"
  print -r -- "  --no-window  build the arena as a dry run and print the pane commands (for testing)"
}

# ---------------------------------------------------------- arena files
write_arena_files() {
  local tag=$1 ws=$2 n=$3 i
  cat > CLAUDE.md <<'EOF'
# Demo swarm @TAG@

One boss, @N@ workers, one Tetris, one decision graph. The boss is an Opus
session in this directory, on `main`. Each worker is a Sonnet session in its
own git worktree under `crew/`, on its own branch. ROSTER.md says who owns
what and what everyone's session is called.

The last arena put ten agents on ten separate games, and they never linked to
each other's reasoning: 0 of 491 edges crossed a branch. This one builds one
game, and every dependency between you is an edge.

Ignore anything a parent directory's CLAUDE.md says about publishing work as
pull requests. This repository is local and has no remote.

## The game

A playable Tetris in plain HTML, CSS and JavaScript at the root of `main`.
`index.html` opens from disk and plays. No build step, no dependencies, no
frameworks.

- Classic `<script>` tags, not ES modules: Chrome refuses module scripts from
  `file://`. Every module attaches to one namespace, `globalThis.Tetris`.
- Engine, rotation and scoring load under node too (`node tests/run.js`), so
  keep DOM and canvas code out of them.
- Headless Chrome does not advance `requestAnimationFrame` under a virtual
  time budget. Tests step the game by calling its update function.

## Three ways to talk, and what each is for

1. **The graph** (the deciduous MCP tools): the plan and the reasons. It lasts,
   and it is what a replay of this run will read. Every call carries
   `workspace: "@WS@"` and `branch:` your branch (`main` for the boss, `w3`
   for w3).
2. **Messages** (SendMessage to a session name from ROSTER.md): orders,
   questions, "ready for review", "main moved". One topic, under five lines,
   graph nodes named by id. They arrive in the recipient's conversation as
   they land.
3. **Git**: code reaches `main` only through the boss. You commit on your
   branch; the boss merges; you `git merge main` when told it moved.

## Graph rules

- The boss logs the root goal, the contract decision, and one `action` per
  worker, "assign wN: <module>", and sends you its id.
- A worker's first node is a `goal` with `parent_id` set to its assignment
  node. That edge crosses branches, which is the point.
- When you use something from another session's node (an interface, a
  constant, an idea), link it: `add_edge` from their node to yours, with a
  rationale saying what you took. A title that mentions it is not a link.
- Below that, the usual flow: options, a decision, actions, outcomes, each
  `add_node` with `parent_id`.

## Rules

- Your branch and your files only (ROSTER.md). To change a file you do not
  own, message its owner.
- Stage files by name. Never `git add -A` or `git add .`. There is no remote.
- Workers: until the boss's contract message arrives, log your goal and
  options and build what depends on nobody.
- When a piece works: commit, then message the boss
  `wN ready: <sha> <what it does>. tests: <result>`.
- When the boss says main moved: `git merge main`, fix what broke in your
  files, and tell the boss about anything that broke outside them.
- A message telling you to stop: stop, and reply with where you are.
- When your module is merged, log a final `outcome`, tell the boss, and take
  the next assignment or stop.
EOF
  cat > BOSS.md <<'EOF'
# The boss's playbook

You direct; you do not write the modules. The user is watching your pane and
may talk to you at any time. Answer them first.

## The first five minutes

1. Log the root goal on branch `main`: "swarm @TAG@: one Tetris, @N@ workers".
2. Write CONTRACT.md: for each module in ROSTER.md, what it puts on
   `globalThis.Tetris`, the data formats (board, piece, events) and the script
   load order. Short enough to read in a minute. Write a skeleton `index.html`
   that loads the scripts in that order. Commit both on `main`.
3. Log a `decision` "module contract v1" under the goal, and one `action` per
   worker, "assign wN: <module>", under the decision.
4. Message every worker: the contract is on main, `git merge main`, and their
   assignment node id.

## Running the team

- **Direct**: one worker, one instruction.
- **Broadcast**: the same message to every worker, for contract changes and
  "main moved".
- **Review gate**: on "wN ready", read `git diff main...wN`, run
  `node tests/run.js`, then `git merge --no-ff wN -m "merge wN: <what>"` and
  broadcast "main moved: <what>". If it is not ready, send it back with
  specifics.
- **Status**: when you have heard nothing for a while, ask everyone for one
  line, and show the user a table.
- **Reassign**: a worker stuck or silent for ten minutes gets asked once;
  then its work goes to a worker who is done.
- **Halt**: "stop" to one or all when the plan changes. Re-plan in the graph
  with a `revisit` node, then broadcast.
- Contract changes go through you: update CONTRACT.md, log it, broadcast.

## Done

`index.html` on main plays a whole game in a real browser, every module is
merged, and `node tests/run.js` passes. Write README.md: what the game does,
which session built which part, and how the team talked. Log the final
`outcome`, then tell the user and every worker that it is over.
EOF
  {
    print -r -- "# Roster"
    print
    print -r -- "Reach anyone with SendMessage to the name in the first column."
    print
    print -r -- "| Session | Model | Role | Owns | Branch | Worktree |"
    print -r -- "|---|---|---|---|---|---|"
    print -r -- "| boss-$tag | Opus | boss: contract, reviews, merges | CONTRACT.md, README.md, every merge to main; index.html until the skeleton is in | main | . |"
    for (( i = 1; i <= n; i++ )); do
      print -r -- "| w$i-$tag | Sonnet | ${ROLE[i]}: ${DOES[i]} | ${OWNS[i]} | w$i | crew/w$i |"
    done
    if (( n < 9 )); then
      print
      print -r -- "Unassigned modules, the boss's to hand out or build:"
      for (( i = n + 1; i <= 9; i++ )); do print -r -- "- ${ROLE[i]}: ${DOES[i]} (${OWNS[i]})"; done
    fi
  } > ROSTER.md
  local f
  for f in CLAUDE.md BOSS.md; do
    sed -i '' -e "s/@TAG@/$tag/g" -e "s/@WS@/$ws/g" -e "s/@N@/$n/g" $f
  done
}

write_manifest() {
  local dir=$1 i
  source $dir/.swarm/env
  {
    print -r -- '{'
    print -r -- "  \"format\": \"one macOS script -r file per pane: repeated {u64 len, u64 sec, u32 usec, u32 dir ('s' start, 'o' output, 'i' input, 'e' end)} + len bytes, little-endian; <name>.size holds cols rows and the epoch the pane started; each claude transcript is ~/.claude/projects/*/<session_id>.jsonl\","
    print -r -- "  \"tag\": \"$TAG\", \"workspace\": \"$WS\", \"arena\": \"$dir\","
    print -r -- "  \"started\": $EPOCHSECONDS, \"terminal\": \"$TERMKIND\", \"dry_run\": $(( DRY ? 1 : 0 )),"
    print -r -- "  \"sessions\": ["
    print -r -- "    {\"name\": \"boss-$TAG\", \"pane\": \"boss\", \"model\": \"opus\", \"branch\": \"main\", \"cwd\": \"$dir\", \"session_id\": \"$SID_boss\", \"recording\": \"boss.rec\"}$([[ $N -gt 0 ]] && print ,)"
    for (( i = 1; i <= N; i++ )); do
      local sid_var=SID_w$i
      print -r -- "    {\"name\": \"w$i-$TAG\", \"pane\": \"w$i\", \"model\": \"sonnet\", \"role\": \"${ROLE[i]}\", \"branch\": \"w$i\", \"cwd\": \"$dir/crew/w$i\", \"session_id\": \"${(P)sid_var}\", \"recording\": \"w$i.rec\"}$( (( i < N )) && print ,)"
    done
    print -r -- "  ]"
    print -r -- '}'
  } > $dir/.swarm/rec/manifest.json
}

# ------------------------------------------------------------ the window
open_window() {
  local term=$1 n=$2 dir=$3; shift 3
  local -a cmds=("$@")
  local ncols=$(( (n + 2) / 3 ))
  local scr=$(osascript -l JavaScript -e 'ObjC.import("AppKit"); var f = $.NSScreen.mainScreen.visibleFrame; var h = $.NSScreen.mainScreen.frame.size.height; [Math.round(f.origin.x), Math.round(h - f.origin.y - f.size.height), Math.round(f.size.width), Math.round(f.size.height)].join(" ")')
  local -a s=(${=scr})
  local as=$dir/.swarm/layout.applescript c r k idx
  if [[ $term == iterm ]]; then
    {
      print -r -- 'tell application "iTerm"'
      print -r -- '  activate'
      print -r -- '  set w to (create window with default profile)'
      print -r -- "  set bounds of w to {${s[1]}, ${s[2]}, $(( s[1] + s[3] )), $(( s[2] + s[4] ))}"
      print -r -- '  set p0 to current session of current tab of w'
      print -r -- '  tell p0 to set c1 to (split vertically with default profile)'
      for (( c = 2; c <= ncols; c++ )); do
        print -r -- "  tell c$(( c - 1 )) to set c$c to (split vertically with default profile)"
      done
      idx=0
      for (( c = 1; c <= ncols; c++ )); do
        local rows=$(( n - (c - 1) * 3 )); (( rows > 3 )) && rows=3
        print -r -- "  set p$(( idx + 1 )) to c$c"
        for (( r = 2; r <= rows; r++ )); do
          print -r -- "  tell p$(( idx + r - 1 )) to set p$(( idx + r )) to (split horizontally with default profile)"
        done
        idx=$(( idx + rows ))
      done
      # Splits halve; set the sizes outright: the boss gets 40%, the columns share the rest.
      # iTerm evens out sibling splits by itself; widen the boss to 40% and
      # share the rest, then even out the rows in each column.
      print -r -- '  delay 0.5'
      local sum="(columns of p0)"
      for (( c = 1; c <= ncols; c++ )); do sum+=" + (columns of c$c)"; done
      print -r -- "  set tot to $sum"
      print -r -- '  set columns of p0 to (tot * 2 div 5)'
      print -r -- "  set cw to (tot - (tot * 2 div 5)) div $ncols"
      for (( c = 1; c < ncols; c++ )); do print -r -- "  set columns of c$c to cw"; done
      idx=0
      for (( c = 1; c <= ncols; c++ )); do
        local rows=$(( n - (c - 1) * 3 )); (( rows > 3 )) && rows=3
        if (( rows > 1 )); then
          local rsum="(rows of p$(( idx + 1 )))"
          for (( r = 2; r <= rows; r++ )); do rsum+=" + (rows of p$(( idx + r )))"; done
          print -r -- "  set rt to $rsum"
          for (( r = 1; r < rows; r++ )); do print -r -- "  set rows of p$(( idx + r )) to rt div $rows"; done
        fi
        idx=$(( idx + rows ))
      done
      print -r -- '  delay 0.7'
      for (( k = 0; k <= n; k++ )); do
        local esc=${cmds[k+1]//\\/\\\\}; esc=${esc//\"/\\\"}
        print -r -- "  tell p$k to write text \"$esc\""
      done
      print -r -- 'end tell'
    } > $as
  else
    {
      print -r -- 'tell application "Ghostty"'
      print -r -- '  activate'
      for (( k = 0; k <= n; k++ )); do
        local esc=${cmds[k+1]//\\/\\\\}; esc=${esc//\"/\\\"}
        local cwd=$dir; (( k )) && cwd=$dir/crew/w$k
        print -r -- "  set cf$k to new surface configuration"
        print -r -- "  set initial working directory of cf$k to \"$cwd\""
        print -r -- "  set initial input of cf$k to \"$esc\" & linefeed"
      done
      print -r -- '  set w to new window with configuration cf0'
      print -r -- '  set p0 to focused terminal of selected tab of w'
      print -r -- '  set c1 to split p0 direction right with configuration cf1'
      for (( c = 2; c <= ncols; c++ )); do
        print -r -- "  set c$c to split c$(( c - 1 )) direction right with configuration cf$(( (c - 1) * 3 + 1 ))"
      done
      idx=0
      for (( c = 1; c <= ncols; c++ )); do
        local rows=$(( n - (c - 1) * 3 )); (( rows > 3 )) && rows=3
        print -r -- "  set p$(( idx + 1 )) to c$c"
        for (( r = 2; r <= rows; r++ )); do
          print -r -- "  set p$(( idx + r )) to split p$(( idx + r - 1 )) direction down with configuration cf$(( idx + r ))"
        done
        idx=$(( idx + rows ))
      done
      print -r -- '  perform action "toggle_maximize" on p0'
      print -r -- '  delay 0.3'
      print -r -- '  perform action "equalize_splits" on p0'
      print -r -- 'end tell'
    } > $as
  fi
  osascript $as >/dev/null || die "the $term layout script failed: $as"
}

# ================================================================== panes
run() {
  local me=$1
  local dir=${SELF:h:h}
  source $dir/.swarm/env
  print -r -- "$COLUMNS $LINES $EPOCHREALTIME" > $dir/.swarm/rec/$me.size
  MARGIN="  "; (( COLUMNS > 84 )) && MARGIN=${(l:$(( (COLUMNS - 80) / 2 )):: :)Z}
  print -rn -- "${e}[?25l"
  trap 'print -rn -- "${e}[?25h$RST"' EXIT INT TERM
  if [[ $me == boss ]]; then tour $dir; else standby $dir $me; fi
  print -rn -- "${e}[?25h"
  start_claude $dir $me
}

start_claude() {
  local dir=$1 me=$2 model name prompt sid_var=SID_$2
  local -a perm
  (( ASK )) || perm=(--dangerously-skip-permissions)
  if [[ $me == boss ]]; then
    model=opus name=boss-$TAG
    prompt="You are the boss of demo swarm $TAG: one Opus session directing $N Sonnet worker(s) who build one Tetris together. Read CLAUDE.md, then BOSS.md and ROSTER.md in this directory, and follow them. The user is watching this pane. Begin."
  else
    local i=${me#w}
    model=sonnet name=$me-$TAG
    prompt="You are $me, the ${ROLE[i]} worker in demo swarm $TAG. This directory is your worktree, on branch $me. Read CLAUDE.md and ROSTER.md here and follow them. Begin."
  fi
  local -a cmd=(claude --model $model --name $name --session-id ${(P)sid_var}
                 --settings $dir/.swarm/settings.json $perm $prompt)
  if (( DRY )); then
    print
    print -r -- "$MARGIN$DIM${C_GREY}dry run; this pane would now run:$RST"
    print -r -- "${(q-)cmd[@]}" | fold -w $(( COLUMNS - ${#MARGIN} - 2 )) | sed "s/^/$MARGIN/"
    return 0
  fi
  exec $cmd
}

# ------------------------------------------------------- worker standby
standby() {
  local dir=$1 me=$2 i=${2#w}
  local col=${WCOL[i]} spin=(⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏) f=0
  clear
  print
  print -r -- "  $col$B▐██▌ $me$RST  $C_TXT${ROLE[i]}$RST  $DIM${C_GREY}sonnet · branch $me$RST"
  print -r -- "  $DIM$C_GREY${DOES[i]}$RST"
  print
  while [[ ! -e $dir/.swarm/go ]]; do
    print -rn -- "$CR  $col${spin[f % 10 + 1]}$RST $C_GREY standing by for the boss…$RST  "
    f=$(( f + 1 )); zselect -t 8 || true
  done
  zselect -t $(( i * 35 )) || true
  print -r -- "$CR  $col●$RST ${C_TXT}online$RST $DIM${C_GREY}· starting claude --model sonnet$RST          "
  : > $dir/.swarm/online/$me
}

# ================================================================== tour
banner() {
  # 5-row glyphs; '#' is a block. Each letter falls into place.
  local -A G
  G[D]="##. #.# #.# #.# ##."   G[E]="### #.. ##. #.. ###"
  G[M]="#...# ##.## #.#.# #...# #...#" G[O]="### #.# #.# #.# ###"
  G[S]="### #.. ### ..# ###"   G[W]="#...# #...# #.#.# ##.## #...#"
  G[A]=".#. #.# ### #.# #.#"   G[R]="##. #.# ##. #.# #.#"
  G[_]=".. .. .. .. .."
  local word="DEMO_SWARM" cols=("$C_I" "$C_O" "$C_T" "$C_S" "" "$C_Z" "$C_J" "$C_L" "$C_P" "$C_M")
  local nl=${#word} frame last=$(( ${#word} * 2 + 6 )) L row k s src w cell
  local -a g
  for (( frame = 0; frame <= last; frame++ )); do
    (( frame )) && print -rn -- "${e}[5F"
    for (( row = 1; row <= 5; row++ )); do
      L=$MARGIN
      for (( k = 1; k <= nl; k++ )); do
        g=(${=G[${word[k]}]})
        s=$(( frame - (k - 1) * 2 ))                 # rows of this letter visible
        (( s > 5 )) && s=5
        src=$(( row - (5 - (s < 0 ? 0 : s)) ))  # glyph row shown here
        w=${#g[1]}
        if (( s > 0 && src >= 1 )); then
          cell=${g[src]//\#/█}; cell=${cell//./ }
          L+="${cols[k]}$cell$RST "
        else
          L+="${(l:$w:: :)Z} "
        fi
      done
      print -r -- "$L${e}[K"
    done
    (( FAST )) || zselect -t 4 || true
    poll_key
  done
}

draw_map() {         # draw_map <lit-count> <redraw?>
  local lit=$1 redraw=$2 n=$N i r c
  (( redraw )) && print -rn -- "${e}[7F"
  local bosscol=$C_GREY; (( lit >= 0 )) && bosscol=$C_BOSS
  cell() {
    local j=$1
    if (( j > n )); then print -rn -- "$DIM$C_GREY ·         $RST"; return; fi
    if (( j <= lit )); then print -rn -- "${WCOL[j]}● w$j ${(r:6:)SHORT[j]}$RST"
    else print -rn -- "$DIM$C_GREY○ w$j ${(r:6:)SHORT[j]}$RST"; fi
  }
  local G=$C_GREY
  print -r -- "$MARGIN$G┌────────────────┬─────────────┬─────────────┬─────────────┐$RST"
  print -r -- "$MARGIN$G│$RST                $G│$RST $(cell 1) $G│$RST $(cell 4) $G│$RST $(cell 7) $G│$RST"
  print -r -- "$MARGIN$G│$RST    $bosscol$B  BOSS  $RST    $G├─────────────┼─────────────┼─────────────┤$RST"
  print -r -- "$MARGIN$G│$RST    $bosscol  opus  $RST    $G│$RST $(cell 2) $G│$RST $(cell 5) $G│$RST $(cell 8) $G│$RST"
  print -r -- "$MARGIN$G│$RST                $G├─────────────┼─────────────┼─────────────┤$RST"
  print -r -- "$MARGIN$G│$RST                $G│$RST $(cell 3) $G│$RST $(cell 6) $G│$RST $(cell 9) $G│$RST"
  print -r -- "$MARGIN$G└────────────────┴─────────────┴─────────────┴─────────────┘$RST"
}

packet() {          # packet <from> <to> <color> : a dot travels the wire
  local from=$1 to=$2 col=$3 len=28 p
  for (( p = 0; p <= len; p++ )); do
    local wire="${(l:$p::─:)Z}●${(l:$(( len - p ))::─:)Z}"
    print -rn -- "$CR$MARGIN   $B$C_BOSS${(r:4:)from}$RST $C_GREY$wire$RST▶ $col$B$to$RST${e}[K"
    (( FAST )) || zselect -t 2 || true; poll_key
  done
  print
}

broadcast() {
  local n=$N len=24 t j
  for (( j = 1; j <= n; j++ )); do print; done
  for (( t = 0; t <= len + n * 2; t++ )); do
    print -rn -- "${e}[${n}F"
    for (( j = 1; j <= n; j++ )); do
      local p=$(( t - (j - 1) * 2 )); (( p < 0 )) && p=0; (( p > len )) && p=$len
      local fill="${(l:$p::━:)Z}" rest="${(l:$(( len - p ))::─:)Z}"
      local mark=" "; (( p == len )) && mark="${WCOL[j]}✓$RST"
      print -r -- "$MARGIN$C_BOSS${B}boss$RST ${WCOL[j]}$fill$RST$C_GREY$rest$RST▶ ${WCOL[j]}w$j$RST $mark${e}[K"
    done
    (( FAST )) || zselect -t 3 || true; poll_key
  done
}

tour() {
  local dir=$1 i
  clear
  print; print
  banner
  print
  print -rn -- "$MARGIN"; type_out "$B$C_TXT" "one boss · $N worker$( (( N > 1 )) && print s) · one Tetris · one decision graph"
  dim "(press any key to fast-forward)"
  beat 120

  head_ "The team"
  print
  draw_map -1 0; beat 40
  draw_map 0 1;  beat 50
  for (( i = 1; i <= N; i++ )); do draw_map $i 1; beat 18; done
  print
  say "The boss is Opus. It writes the contract, hands out the work,"
  say "reviews every branch, and is the only one who merges to main."
  say "The workers are Sonnet, one module each, each in its own"
  say "git worktree, on its own branch."
  beat 80

  head_ "How the boss runs the team"
  print
  print -rn -- "$MARGIN"; type_out "$B$C_I" "1. The plan lives in the graph"
  dim "   every node carries the why, and every worker's goal hangs off"
  dim "   the boss's assignment: an edge across branches, on purpose."
  print -r -- "$MARGIN   $C_BOSS◆ goal$RST      swarm $TAG: one Tetris"; nap 25
  print -r -- "$MARGIN   $C_GREY└$RST $C_BOSS◆ decision$RST  module contract v1"; nap 25
  for (( i = 1; i <= (N < 3 ? N : 3); i++ )); do
    print -r -- "$MARGIN      $C_GREY├$RST $C_BOSS▸ action$RST  assign w$i: ${ROLE[i]}  $C_GREY◀──$RST ${WCOL[i]}◆ goal$RST ${WCOL[i]}(w$i)$RST"; nap 25
  done
  (( N > 3 )) && { print -r -- "$MARGIN      $C_GREY└$RST $DIM$C_GREY… and $(( N - 3 )) more$RST"; nap 25 }
  beat 60

  print
  print -rn -- "$MARGIN"; type_out "$B$C_O" "2. Direct orders"
  dim "   a message lands in one worker's conversation as it is sent."
  packet boss w$(( N < 3 ? N : 3 )) ${WCOL[$(( N < 3 ? N : 3 ))]}
  print -r -- "$MARGIN        $IT$C_TXT\"DAS 167ms, ARR 33ms, per node 4f2a. ship it.\"$RST"; nap 40
  print -rn -- "$CR$MARGIN   ${WCOL[1]}${B}w1  $RST$C_GREY$(printf '─%.0s' {1..10})●$(printf '─%.0s' {1..17})$RST▶ $C_BOSS${B}boss$RST"; nap 30; print
  print -r -- "$MARGIN        $IT$C_TXT\"w1 ready: 9c1e 7-bag and lock delay. tests: 14 pass\"$RST"
  beat 60

  print
  print -rn -- "$MARGIN"; type_out "$B$C_T" "3. Broadcast"
  dim "   one change, everyone: \"main moved: engine merged, git merge main\""
  broadcast
  beat 50

  print
  print -rn -- "$MARGIN"; type_out "$B$C_S" "4. The merge gate"
  dim "   code reaches main only after the boss reads it and the tests pass."
  print -r -- "$MARGIN   ${WCOL[1]}w1   ○──○──○$RST$C_GREY─╮$RST"; nap 25
  print -r -- "$MARGIN   ${WCOL[2]}w2      ○──○$RST$C_GREY─┼──╮$RST"; nap 25
  print -r -- "$MARGIN   $C_BOSS${B}main$RST $C_BOSS●───────────●──●──$RST  ${DIM}${C_GREY}each ● a reviewed, tested merge$RST"; nap 40
  beat 50

  print
  print -rn -- "$MARGIN"; type_out "$B$C_Z" "5. Status, reassign, halt"
  dim "   \"one line each\" · a stalled module moves to a free worker ·"
  dim "   \"stop\" freezes everyone while the plan changes in the graph."
  beat 90

  head_ "The quest"
  say "Last time, ten agents built ten Tetrises side by side and"
  say "linked none of their borrowing: 0 of 491 edges crossed a branch."
  say "This time the team builds one game: SRS with kicks, 7-bag, hold,"
  say "ghost, DAS and ARR, T-spins, synthesized sound, juice, and tests"
  say "that run. Every dependency between sessions is an edge."
  print
  dim "Every pane is being recorded with timestamps, for a replay."
  beat 120

  head_ "Launch"
  local d
  print -rn -- "$MARGIN"
  for d in 3 2 1; do
    print -rn -- "$B$C_BOSS  $d $RST"; (( FAST )) || zselect -t 70 || true
  done
  print -r -- "$B$C_S go$RST"
  : > $dir/.swarm/go
  print
  # Workers come online in a cascade; show it as it happens.
  local online=0 deadline=$(( EPOCHSECONDS + 30 ))
  for (( i = 1; i <= N; i++ )); do print; done
  while (( online < N && EPOCHSECONDS < deadline )); do
    print -rn -- "${e}[${N}F"
    online=0
    for (( i = 1; i <= N; i++ )); do
      if [[ -e $dir/.swarm/online/w$i ]]; then
        online=$(( online + 1 ))
        print -r -- "$MARGIN  ${WCOL[i]}●$RST w$i  ${(r:9:)ROLE[i]} ${C_TXT}online$RST  $DIM${C_GREY}sonnet$RST${e}[K"
      else
        print -r -- "$MARGIN  $C_GREY○ w$i  ${(r:9:)ROLE[i]} waiting$RST${e}[K"
      fi
    done
    zselect -t 10 || true
  done
  print
  say "The team is up. The boss is yours: talk to it here."
  dim "try: \"status from everyone\" · \"w6, make the line clear punchier\""
  dim "     \"stop everyone, we are switching to a dark theme\""
  beat 150
  clear
}

# ================================================================== main
case ${1:-launch} in
  launch) shift $(( $# ? 1 : 0 )); launch "$@" ;;
  run)    run $2 ;;
  -h|--help|help) usage ;;
  *) launch "$@" ;;
esac
