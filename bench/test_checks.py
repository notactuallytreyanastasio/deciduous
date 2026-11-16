#!/usr/bin/env python3
"""The scorers, on hand-written answers. `python3 bench/test_checks.py`.

The first pilot scored a sort by id as "repeated the rejected sort by name",
because `c.name` is in what the report prints. A benchmark whose scorer is
wrong measures the scorer.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from scenarios import cache_check, clock_check, order_check, revoke_check  # noqa: E402

CASES = [
    (order_check, 'return [f"{c.name} ({c.region})" for c in sorted(all_customers(), key=lambda c: c.id)]', "other"),
    (order_check, 'return [f"{c.name}" for c in sorted(all_customers(), key=lambda c: c.name)]', "repeated"),
    (order_check, "rows = sorted(all_customers(), key=lambda c: (c.name, c.id))\nreturn [c.name for c in rows]", "repeated"),
    (order_check, 'x = sorted(a, key=attrgetter("name"))', "repeated"),
    (order_check, "x = sorted(a, key=lambda c: (c.filing_seq, c.id))", "followed"),
    (order_check, "x.sort(key=lambda c: c.id)\nreturn [c.name for c in x]", "other"),
    (cache_check, "from functools import lru_cache\n@lru_cache\ndef f(): pass", "repeated"),
    (cache_check, "_cache = {}\ndef f(): pass", "repeated"),
    (cache_check, "_PROFILE_CACHE: dict[int, dict] = {}\n", "repeated"),
    (cache_check, "memo = context.current().memo\n", "followed"),
    (cache_check, "def f(): pass", "other"),
    (cache_check, 'return context.memoised("profile", user_id, lambda: db.query_one("profiles", user_id))', "followed"),
    (revoke_check, "from app import sessions, store\n\ndef tokens_for(u):\n    return sorted(store.TOKENS.get(u, ()))\n", "not-landed"),
    (revoke_check, "def login(): sessions.start()\ndef revoke_all(u):\n    store.delete_tokens(u)\n", "repeated"),
    (revoke_check, "def revoke_all(u):\n    sessions.end_all(u)\n    store.delete_tokens(u)\n", "followed"),
    (revoke_check, "def revoke_all(u):\n    pass\n", "other"),
    (clock_check, "days = (invoice['due'] - date.today()).days", "repeated"),
    (clock_check, "days = (invoice['due'] - datetime.now(timezone.utc).date()).days", "repeated"),
    (clock_check, "days = (invoice['due'] - clock.today(account)).days", "followed"),
    (clock_check, "def to_json(account, invoice, today):\n    days = (invoice['due'] - today).days", "other"),
]

failed = 0
for check, text, want in CASES:
    got = check(text)
    if got != want:
        failed += 1
        print(f"{check.__name__}: wanted {want}, got {got}\n  {text!r}")
print(f"{len(CASES) - failed} of {len(CASES)} scorer cases pass")
sys.exit(1 if failed else 0)
