#!/bin/bash
# board-mentions.sh
# Shows this session what other agents asked it on the deciduous message board
# and it has not answered yet. Silent, and nearly free, unless
# DECIDUOUS_AGENT_LABEL names this session and something is waiting.
#   start   (SessionStart):      everything unanswered for the label
#   prompt  (UserPromptSubmit):  only what arrived since it last showed any
# Non-blocking: exit 0 always.

label="${DECIDUOUS_AGENT_LABEL:-}"
[ -n "$label" ] || exit 0
[ -d ".deciduous" ] || exit 0
command -v deciduous >/dev/null 2>&1 || exit 0

seen_file=".deciduous/.board-seen-$(printf '%s' "$label" | tr -c 'A-Za-z0-9_.-' '_')"
since=0
if [ "${1:-start}" = "prompt" ] && [ -f "$seen_file" ]; then
    since=$(cat "$seen_file" 2>/dev/null)
fi
case "$since" in ''|*[!0-9]*) since=0 ;; esac

out=$(deciduous board read --unanswered "$label" --since "$since" 2>/dev/null) || exit 0
case "$out" in
    ""|"Nothing addressed"*) exit 0 ;;
esac

latest=$(printf '%s\n' "$out" | grep -oE '^#[0-9]+' | tail -1 | tr -d '#')
[ -n "$latest" ] && printf '%s\n' "$latest" > "$seen_file"

echo "DECIDUOUS BOARD: messages for @$label you have not answered."
echo "Answer each with post_message reply_to=<id>, or: deciduous board post --as $label --reply-to <id> -s ... -m ..."
echo
printf '%s\n' "$out"
exit 0
