"""The benchmark's scenarios.

Each is a small repository with a history: something was tried, it went
wrong for a reason the code does not show, it was taken back out, and the
right approach was applied somewhere else. The task then asks for work where
the approach that went wrong is the obvious one.

The reason is deliberately a fact about the project (who reconciles the
listing, which process edits profiles), not a best practice a model already
knows. What is measured is whether the record of the decision reaches the
agent, not whether the agent is good at programming.

A scenario is a dict:

  name, kind    kind is "same-file" when the history of the file the task
                edits leads to the decision, "other-file" when the decision
                was made about a different file, "blind" when it is the same
                file and nothing in the code shows what was settled on
  avoid         titles of unrelated decisions that would not be unrelated
                here, left out of this scenario's repository
  prompt        the task, which never names either approach
  target        the file the task should change
  commits       [(message, {path: content})], applied in order
  hard_last     (not for "blind", which is hard already) a replacement for the last commit in which the approach that
                was decided on is not yet used anywhere, so no neighbouring
                file shows it (content None deletes the file)
  settled,      what the last commit did, in words, for the graph's action
  settled_hard  and the ADR's consequences
  graph         a function(g) that records the decision with the CLI; g.add,
                g.link, g.status, g.pivot, and g.sha[i] for commit i
  adr           {filename: markdown}: the same decision as ADR files
  check         function(text of target) -> "followed" | "repeated" | "other"
  mention       regex: the final report refers to the earlier decision
"""

import re


def has(pattern, text):
    return re.search(pattern, text, re.M) is not None


# --------------------------------------------------------------------------
# cache: a per-process cache served stale permissions
# --------------------------------------------------------------------------

CACHE_CONTEXT = '''"""Per-request state."""


class RequestContext:
    """Created by the server when a request starts and dropped when it ends."""

    def __init__(self):
        self.memo = {}


_current = RequestContext()


def current():
    return _current


def begin_request():
    global _current
    _current = RequestContext()
'''

CACHE_DB = '''"""The database. Every query is a network round trip."""

import time

ROWS = {
    "accounts": {1: {"id": 1, "plan": "team"}},
    "profiles": {7: {"id": 7, "account_id": 1, "permissions": ["read", "write"]}},
}


def query_one(table, key):
    time.sleep(0.05)
    return dict(ROWS[table][key])


def update(table, key, **fields):
    time.sleep(0.05)
    ROWS[table][key].update(fields)
'''

CACHE_ACCOUNTS_BASE = '''from app import db


def load_account(account_id):
    return db.query_one("accounts", account_id)
'''

CACHE_PROFILE_BASE = '''from app import db


def load_profile(user_id):
    return db.query_one("profiles", user_id)


def can(user_id, permission):
    return permission in load_profile(user_id)["permissions"]
'''

CACHE_ACCOUNTS_LRU = '''from functools import lru_cache

from app import db


@lru_cache(maxsize=None)
def load_account(account_id):
    return db.query_one("accounts", account_id)
'''

CACHE_PROFILE_LRU = '''from functools import lru_cache

from app import db


@lru_cache(maxsize=None)
def load_profile(user_id):
    return db.query_one("profiles", user_id)


def can(user_id, permission):
    return permission in load_profile(user_id)["permissions"]
'''

CACHE_ACCOUNTS_MEMO = '''from app import context, db


def load_account(account_id):
    memo = context.current().memo
    key = ("account", account_id)
    if key not in memo:
        memo[key] = db.query_one("accounts", account_id)
    return memo[key]
'''


def cache_graph(g):
    goal = g.add("goal", "Make account and profile lookups cheap")
    a = g.add("option", "functools.lru_cache on the loaders")
    b = g.add("option", "Memoise per request in RequestContext.memo")
    g.link(goal, a)
    g.link(goal, b)
    tried = g.add("action", "Cache load_account and load_profile with lru_cache", commit=g.sha[1])
    g.link(a, tried)
    failed = g.add(
        "outcome",
        "Stale permissions: a revoked permission kept working until the worker restarted",
        description="Each worker process keeps its own cache and nothing invalidates it. "
        "The admin tool revokes a permission in another process; the API workers went on "
        "answering from their caches for hours.",
    )
    g.link(tried, failed)
    g.status(tried, "abandoned")
    d = g.add("decision", "Memoise lookups per request, never per process")
    g.link(a, d, "rejected", "a per-process cache outlives a permission change: revoked access kept working until restart")
    g.link(b, d, "chosen", "a request sees one consistent row and nothing survives the request")
    g.status(a, "rejected")
    done = g.add("action", g.settled, commit=g.sha[2])
    g.link(d, done)
    g.status(done, "completed")
    g.status(d, "completed")


def cache_check(text):
    per_process = (
        has(r"lru_cache|functools\.cache|^\s*@cache\b|cachetools|TTLCache", text)
        or has(r"^_?\w*(cache|CACHE|memo|MEMO)\w*\s*(:\s*[\w\[\], ]+)?\s*=\s*(\{\}|dict\()", text)
    )
    if per_process:
        return "repeated"
    # `.memo`, or a helper on the request context that wraps it.
    if has(r"\.memo\b|\bcontext\.\w+\(", text):
        return "followed"
    return "other"


CACHE = {
    "name": "cache",
    "kind": "same-file",
    "prompt": "load_profile gets called many times while handling a single request "
    "(every can() check calls it) and each call hits the database. Make the repeated "
    "lookups cheap.",
    "target": "app/profile.py",
    "commits": [
        ("initial import", {
            "app/__init__.py": "",
            "app/context.py": CACHE_CONTEXT,
            "app/db.py": CACHE_DB,
            "app/accounts.py": CACHE_ACCOUNTS_BASE,
            "app/profile.py": CACHE_PROFILE_BASE,
        }),
        ("cache lookups", {
            "app/accounts.py": CACHE_ACCOUNTS_LRU,
            "app/profile.py": CACHE_PROFILE_LRU,
        }),
        ("accounts: memo per request; profile uncached again", {
            "app/accounts.py": CACHE_ACCOUNTS_MEMO,
            "app/profile.py": CACHE_PROFILE_BASE,
        }),
    ],
    "settled": "Memoise load_account in RequestContext.memo; load_profile left uncached for now",
    "hard_last": ("back out the lookup cache", {
        "app/accounts.py": CACHE_ACCOUNTS_BASE,
        "app/profile.py": CACHE_PROFILE_BASE,
    }),
    "settled_hard": "Back out lru_cache from both loaders; the per-request memo is not built yet",
    "graph": cache_graph,
    "adr": {
        "0005-memoise-per-request-not-per-process.md": """# 5. Memoise lookups per request, never per process

Status: accepted

## Context

load_account and load_profile were cached with functools.lru_cache. Each
worker process keeps its own cache and nothing invalidates it. The admin tool
revokes a permission in another process; the API workers went on answering
from their caches, and a revoked permission kept working until the worker
restarted.

## Decision

Lookups are memoised per request, in RequestContext.memo. A request sees one
consistent row and nothing survives the request. A per-process cache
(lru_cache, a module-level dict) is rejected.

## Consequences

@SETTLED@.
""",
    },
    "check": cache_check,
    "mention": r"stale|revoked|per[- ]process|per[- ]request|worker",
}


# --------------------------------------------------------------------------
# order: listings sorted by name could not be reconciled
# --------------------------------------------------------------------------

ORDER_MODELS = '''from collections import namedtuple

Customer = namedtuple("Customer", "id name filing_seq region")

_CUSTOMERS = {
    Customer(1, "Ostrander Freight", 412, "north"),
    Customer(2, "Abel & Lowe", 87, "south"),
    Customer(3, "Marrow Holdings", 5, "north"),
    Customer(4, "Dunmore Mills", 230, "east"),
}


def all_customers():
    """Every customer. The store makes no promise about order."""
    return list(_CUSTOMERS)
'''

ORDER_STATEMENTS_BASE = '''from app.models import all_customers


def statement_rows():
    return [(c.id, c.name) for c in all_customers()]
'''

ORDER_REPORT_BASE = '''from app.models import all_customers


def report_rows():
    return [f"{c.name} ({c.region})" for c in all_customers()]
'''

ORDER_STATEMENTS_NAME = '''from app.models import all_customers


def statement_rows():
    return [(c.id, c.name) for c in sorted(all_customers(), key=lambda c: c.name)]
'''

ORDER_REPORT_NAME = '''from app.models import all_customers


def report_rows():
    return [f"{c.name} ({c.region})" for c in sorted(all_customers(), key=lambda c: c.name)]
'''

ORDER_STATEMENTS_SEQ = '''from app.models import all_customers


def statement_rows():
    return [(c.id, c.name) for c in sorted(all_customers(), key=lambda c: c.filing_seq)]
'''


def order_graph(g):
    goal = g.add("goal", "Stable ordering for customer listings")
    a = g.add("option", "Sort listings by customer name")
    b = g.add("option", "Sort listings by filing_seq, the order of the regulator's register")
    g.link(goal, a)
    g.link(goal, b)
    tried = g.add("action", "Sort statements and the customer report by name", commit=g.sha[1])
    g.link(a, tried)
    failed = g.add(
        "outcome",
        "Reconciliation became a manual lookup per line; two filings went in late",
        description="The auditors reconcile every listing we produce line by line against "
        "the regulator's register, which is in filing order. In name order nothing lines up.",
    )
    g.link(tried, failed)
    g.status(tried, "abandoned")
    d = g.add("decision", "Customer listings are in filing_seq order, never by name")
    g.link(a, d, "rejected", "auditors reconcile against the register line by line; name order made that a manual lookup")
    g.link(b, d, "chosen", "it is the order of the register every listing is checked against")
    g.status(a, "rejected")
    done = g.add("action", g.settled, commit=g.sha[2])
    g.link(d, done)
    g.status(done, "completed")
    g.status(d, "completed")


def sort_keys(text):
    """The expression after each `key=`, up to the parenthesis that closes
    the call. `c.name` in what is printed is not what is sorted by."""
    out = []
    for m in re.finditer(r"key\s*=", text):
        depth, i = 0, m.end()
        while i < len(text) and not (depth == 0 and text[i] in ",)\n" and text[i] != ","):
            depth += text[i] in "([{"
            depth -= text[i] in ")]}"
            if depth < 0:
                break
            i += 1
        out.append(text[m.end():i])
    return out


def order_check(text):
    if has(r"filing_seq", text):
        return "followed"
    if any(has(r"\bname\b", key) for key in sort_keys(text)):
        return "repeated"
    return "other"


ORDER = {
    "name": "order",
    "kind": "same-file",
    "prompt": "The customer report comes out in a different order every time it runs, "
    "which makes the diff between two runs useless. Make its order deterministic.",
    "target": "app/report.py",
    "commits": [
        ("initial import", {
            "app/__init__.py": "",
            "app/models.py": ORDER_MODELS,
            "app/statements.py": ORDER_STATEMENTS_BASE,
            "app/report.py": ORDER_REPORT_BASE,
        }),
        ("sort customer listings", {
            "app/statements.py": ORDER_STATEMENTS_NAME,
            "app/report.py": ORDER_REPORT_NAME,
        }),
        ("statements in filing order; report unsorted again", {
            "app/statements.py": ORDER_STATEMENTS_SEQ,
            "app/report.py": ORDER_REPORT_BASE,
        }),
    ],
    "settled": "Statements sorted by filing_seq; the report left unsorted for now",
    "hard_last": ("back out the name sort", {
        "app/statements.py": ORDER_STATEMENTS_BASE,
        "app/report.py": ORDER_REPORT_BASE,
    }),
    "settled_hard": "Back out the name sort from both listings; nothing sorts by filing_seq yet",
    "graph": order_graph,
    "adr": {
        "0005-listings-in-filing-order.md": """# 5. Customer listings are in filing_seq order, never by name

Status: accepted

## Context

Statements and the customer report were sorted by customer name. The auditors
reconcile every listing we produce line by line against the regulator's
register, which is in filing order. In name order nothing lines up:
reconciliation became a manual lookup per line and two filings went in late.

## Decision

Every customer listing is sorted by filing_seq. Sorting by name is rejected.

## Consequences

@SETTLED@.
""",
    },
    "check": order_check,
    "mention": r"filing|register|auditor|reconcil",
}


# --------------------------------------------------------------------------
# clock: the decision was made about another file
# --------------------------------------------------------------------------

CLOCK_INVOICE = '''def to_json(account, invoice):
    return {
        "id": invoice["id"],
        "account": account["id"],
        "total_cents": invoice["total_cents"],
        "due": invoice["due"].isoformat(),
    }
'''

CLOCK_ACCOUNTS = '''ACCOUNTS = {
    1: {"id": 1, "name": "Kauri Supply", "billing_tz": "Pacific/Auckland"},
    2: {"id": 2, "name": "Hale Street Press", "billing_tz": "America/Chicago"},
}
'''

CLOCK_REMINDERS_UTC = '''from datetime import date


def is_overdue(account, invoice):
    return invoice["due"] < date.today()
'''

CLOCK_CLOCK = '''from datetime import datetime
from zoneinfo import ZoneInfo


def today(account):
    return datetime.now(ZoneInfo(account["billing_tz"])).date()
'''

CLOCK_REMINDERS_TZ = '''from app import clock


def is_overdue(account, invoice):
    return invoice["due"] < clock.today(account)
'''


def clock_graph(g):
    goal = g.add("goal", "Send overdue reminders")
    a = g.add("option", "Use the server's date: date.today()")
    b = g.add("option", "Use the account's own day: clock.today(account), in its billing_tz")
    g.link(goal, a)
    g.link(goal, b)
    tried = g.add("action", "is_overdue compares the due date with date.today()", commit=g.sha[1])
    g.link(a, tried)
    failed = g.add(
        "outcome",
        "Customers in Auckland and Tokyo were told they were overdue a day early",
        description="The servers run in UTC. For an account in Pacific/Auckland the server's "
        "date is yesterday for half of every day, and the reverse for the Americas.",
    )
    g.link(tried, failed)
    g.status(tried, "abandoned")
    d = g.add("decision", "Any 'today' computed for a customer is clock.today(account), never the server's date")
    g.link(a, d, "rejected", "the server's UTC date is the wrong day for half of every day in Auckland")
    g.link(b, d, "chosen", "a due date is a day in the account's billing_tz")
    g.status(a, "rejected")
    done = g.add("action", g.settled, commit=g.sha[2])
    g.link(d, done)
    g.status(done, "completed")
    g.status(d, "completed")


def clock_check(text):
    if has(r"clock\.today\s*\(|from app\.clock import today|billing_tz", text):
        return "followed"
    if has(r"date\.today\(\)|datetime\.(now|utcnow|today)\(|time\.time\(", text):
        return "repeated"
    return "other"


CLOCK = {
    "name": "clock",
    "kind": "other-file",
    "prompt": "Add a due_in_days field to the invoice JSON in app/invoice.py: the number "
    "of days from today until the invoice's due date, negative once it is overdue.",
    "target": "app/invoice.py",
    "commits": [
        ("initial import", {
            "app/__init__.py": "",
            "app/accounts.py": CLOCK_ACCOUNTS,
            "app/invoice.py": CLOCK_INVOICE,
        }),
        ("overdue reminders", {"app/reminders.py": CLOCK_REMINDERS_UTC}),
        ("reminders: the account's day", {
            "app/clock.py": CLOCK_CLOCK,
            "app/reminders.py": CLOCK_REMINDERS_TZ,
        }),
    ],
    "settled": "Add app/clock.py and use it in is_overdue",
    "hard_last": ("pull reminders until the day is right", {
        "app/clock.py": CLOCK_CLOCK,
        "app/reminders.py": None,
    }),
    "settled_hard": "Add app/clock.py; reminders are pulled until they are rebuilt on it",
    "graph": clock_graph,
    "adr": {
        "0005-today-is-the-accounts-day.md": """# 5. Any 'today' computed for a customer is clock.today(account)

Status: accepted

## Context

is_overdue compared an invoice's due date with date.today(). The servers run
in UTC. For an account in Pacific/Auckland the server's date is yesterday for
half of every day: customers in Auckland and Tokyo were told they were
overdue a day early.

## Decision

Any "today" computed for or shown to a customer is clock.today(account), the
date in the account's billing_tz. The server's date (date.today(),
datetime.now()) is rejected for this.

## Consequences

@SETTLED@.
""",
    },
    "check": clock_check,
    "mention": r"billing_tz|time ?zone|Auckland|clock\.today|a day early",
}


# --------------------------------------------------------------------------
# revoke: the decision the file was written under has been superseded
# --------------------------------------------------------------------------

REVOKE_STORE = '''"""The tokens table."""

TOKENS = {}


def add_token(user_id, token):
    TOKENS.setdefault(user_id, set()).add(token)


def delete_tokens(user_id):
    """Remove every token a user holds."""
    TOKENS.pop(user_id, None)
'''

REVOKE_AUTH_TOKENS = '''import secrets

from app import store


def login(user_id):
    token = secrets.token_hex(16)
    store.add_token(user_id, token)
    return token


def tokens_for(user_id):
    return sorted(store.TOKENS.get(user_id, ()))
'''

REVOKE_SESSIONS = '''import secrets

_SESSIONS = {}


def start(user_id):
    token = secrets.token_hex(16)
    _SESSIONS[token] = user_id
    return token


def is_active(token):
    return token in _SESSIONS


def end(token):
    _SESSIONS.pop(token, None)


def end_all(user_id):
    for token in [t for t, u in _SESSIONS.items() if u == user_id]:
        del _SESSIONS[token]


def by_user():
    out = {}
    for token, user_id in _SESSIONS.items():
        out.setdefault(user_id, set()).add(token)
    return out
'''

REVOKE_AUTH_SESSIONS = '''from app import sessions, store


def login(user_id):
    return sessions.start(user_id)


def tokens_for(user_id):
    return sorted(store.TOKENS.get(user_id, ()))
'''

REVOKE_MIDDLEWARE_TOKENS = '''from app import store


def authenticated(user_id, token):
    return token in store.TOKENS.get(user_id, ())
'''

REVOKE_MIDDLEWARE_SESSIONS = '''from app import sessions


def authenticated(user_id, token):
    return sessions.is_active(token)
'''

REVOKE_JOB = '''from app import sessions, store


def run():
    store.TOKENS.clear()
    store.TOKENS.update(sessions.by_user())
'''


REVOKE_LOGIN = '''from app import sessions


def login(user_id):
    return sessions.start(user_id)
'''

REVOKE_AUTH_HARD = '''from app import store


def tokens_for(user_id):
    return sorted(store.TOKENS.get(user_id, ()))
'''


def revoke_graph(g):
    goal = g.add("goal", "Know who is logged in")
    old = g.add("decision", "The tokens table is the source of truth for logins")
    g.link(goal, old)
    built = g.add("action", "login writes a token row; the middleware reads the tokens table", commit=g.sha[1])
    g.link(old, built)
    g.status(built, "completed")
    new = g.pivot(
        old,
        "A token row says nothing about a device or an expiry, and the reporting service reads the table directly",
        "Sessions are the source of truth; the tokens table is a mirror rebuilt nightly from them",
        "nothing that changes the tokens table lasts past the nightly rebuild, and the middleware no longer reads it",
    )
    moved = g.add("action", g.settled, commit=g.sha[2])
    g.link(new, moved)
    g.status(moved, "completed")
    g.status(new, "completed")


def revoke_check(text):
    if "def revoke_all" not in text:
        return "not-landed"
    body = text.split("def revoke_all", 1)[1]
    if has(r"sessions\.\w+", body):
        return "followed"
    if has(r"delete_tokens|TOKENS", body):
        return "repeated"
    return "other"


REVOKE = {
    "name": "revoke",
    "kind": "same-file",
    "prompt": "Add revoke_all(user_id) to app/auth.py: it logs the user out everywhere.",
    "target": "app/auth.py",
    "commits": [
        ("initial import", {"app/__init__.py": "", "README.md": "# app\n"}),
        ("auth: tokens table", {
            "app/store.py": REVOKE_STORE,
            "app/auth.py": REVOKE_AUTH_TOKENS,
            "app/middleware.py": REVOKE_MIDDLEWARE_TOKENS,
        }),
        ("auth: sessions", {
            "app/sessions.py": REVOKE_SESSIONS,
            "app/auth.py": REVOKE_AUTH_SESSIONS,
            "app/middleware.py": REVOKE_MIDDLEWARE_SESSIONS,
            "app/jobs/__init__.py": "",
            "app/jobs/mirror_tokens.py": REVOKE_JOB,
        }),
    ],
    "settled": "login starts a session; the middleware asks sessions; a nightly job rebuilds the tokens table for reporting",
    "hard_last": ("auth: sessions", {
        "app/sessions.py": REVOKE_SESSIONS,
        "app/login.py": REVOKE_LOGIN,
        "app/auth.py": REVOKE_AUTH_HARD,
        "app/middleware.py": REVOKE_MIDDLEWARE_SESSIONS,
        "app/jobs/__init__.py": "",
        "app/jobs/mirror_tokens.py": REVOKE_JOB,
    }),
    "settled_hard": "login moves to app/login.py and starts a session; the middleware asks sessions; a nightly job rebuilds the tokens table for reporting",
    "graph": revoke_graph,
    "adr": {
        "0005-tokens-table-is-the-source-of-truth.md": """# 5. The tokens table is the source of truth for logins

Status: superseded by 6

## Decision

login writes a token row; the middleware reads the tokens table.
""",
        "0006-sessions-are-the-source-of-truth.md": """# 6. Sessions are the source of truth; the tokens table is a nightly mirror

Status: accepted. Supersedes 5.

## Context

A token row says nothing about a device or an expiry, and the reporting
service reads the table directly.

## Decision

Sessions are the source of truth for who is logged in. The tokens table is a
mirror rebuilt nightly from sessions, kept for the reporting service. Nothing
that changes the tokens table lasts past the nightly rebuild, and the
middleware no longer reads it.

## Consequences

@SETTLED@.
""",
    },
    "check": revoke_check,
    "mention": r"mirror|source of truth|supersed|nightly|sessions are",
}

# --------------------------------------------------------------------------
# The blind scenarios. In the four above, three give the answer away in the
# code: a memo on the request context, a clock module, a sessions module.
# In these, as in `order`, nothing in the repository says which approach was
# settled on or why; only the record does. Each was tried one way, went
# wrong, and was backed out, and the task asks for the same work again.
# --------------------------------------------------------------------------


def tried_and_backed_out(g, goal, rejected, chosen, tried, failed, failed_detail, decision, why_not, why):
    goal = g.add("goal", goal)
    a = g.add("option", rejected)
    b = g.add("option", chosen)
    g.link(goal, a)
    g.link(goal, b)
    t = g.add("action", tried, commit=g.sha[1])
    g.link(a, t)
    f = g.add("outcome", failed, description=failed_detail)
    g.link(t, f)
    g.status(t, "abandoned")
    d = g.add("decision", decision)
    g.link(a, d, "rejected", why_not)
    g.link(b, d, "chosen", why)
    g.status(a, "rejected")
    done = g.add("action", g.settled, commit=g.sha[2])
    g.link(d, done)
    g.status(done, "completed")
    g.status(d, "completed")


RETRY_GATEWAY = '''"""The card processor's client."""


class GatewayTimeout(Exception):
    """The processor did not answer in time."""


def charge(account_id, cents):
    """Charge the account's card and return the processor's receipt id."""
    raise NotImplementedError("the processor is not reachable from here")
'''

RETRY_QUEUE = '''"""Background work. A worker per queue picks jobs up in order."""

JOBS = {}


def enqueue(queue, payload):
    JOBS.setdefault(queue, []).append(payload)
'''

RETRY_PAYMENTS_BASE = '''from app import gateway


def pay(account_id, cents):
    receipt = gateway.charge(account_id, cents)
    return {"status": "paid", "receipt": receipt}
'''

RETRY_PAYMENTS_LOOP = '''from app import gateway


def pay(account_id, cents):
    for attempt in range(3):
        try:
            receipt = gateway.charge(account_id, cents)
            return {"status": "paid", "receipt": receipt}
        except gateway.GatewayTimeout:
            if attempt == 2:
                raise
'''


def retry_graph(g):
    tried_and_backed_out(
        g,
        "Payments survive a processor timeout",
        "Retry the charge when the processor times out",
        "Never retry: put the charge on the reconcile queue and answer pending",
        "pay() retries a timed-out charge up to three times",
        "Customers were charged twice",
        "A timeout does not mean the charge failed. The processor completed about one in "
        "five of the charges it timed out on, and the retry charged the card again. "
        "Refunds took a week.",
        "A charge that times out is never retried: enqueue it on the 'reconcile' queue and return status pending",
        "a timed-out charge may have gone through; retrying it charged cards twice",
        "the reconcile worker asks the processor what happened before anything is charged again",
    )


def retry_check(text):
    if has(r"enqueue\(", text) and has(r"reconcil", text):
        return "followed"
    if has(r"(?i)for \w+ in range|while |retry|retries|attempt|backoff|tenacity", text):
        return "repeated"
    return "other"


RETRY = {
    "name": "retry",
    "kind": "blind",
    "prompt": "pay() in app/payments.py blows up whenever the card processor times out, "
    "which happens a few times a day. Make it cope with a timeout.",
    "target": "app/payments.py",
    "commits": [
        ("initial import", {
            "app/__init__.py": "",
            "app/gateway.py": RETRY_GATEWAY,
            "app/queue.py": RETRY_QUEUE,
            "app/payments.py": RETRY_PAYMENTS_BASE,
        }),
        ("payments: cope with timeouts", {"app/payments.py": RETRY_PAYMENTS_LOOP}),
        ("payments: back out timeout handling", {"app/payments.py": RETRY_PAYMENTS_BASE}),
    ],
    "settled": "Back out the retry loop; the reconcile path is not built yet",
    "avoid": {"One retry policy for outbound HTTP", "Background jobs are idempotent"},
    "graph": retry_graph,
    "adr": {
        "0005-a-timed-out-charge-is-never-retried.md": """# 5. A charge that times out is never retried

Status: accepted

## Context

pay() retried a charge up to three times when the processor timed out. A
timeout does not mean the charge failed: the processor completed about one in
five of the charges it timed out on, and the retry charged the card again.
Customers were charged twice and refunds took a week.

## Decision

A charge that times out is never retried. It is enqueued on the 'reconcile'
queue and pay() returns status pending; the reconcile worker asks the
processor what happened before anything is charged again.

## Consequences

@SETTLED@.
""",
    },
    "check": retry_check,
    "mention": r"reconcil|charged twice|double[- ]charg|never retr|may have gone through",
}


DELETE_STORE = '''"""The tables."""

CUSTOMERS = {
    1: {"id": 1, "name": "Kauri Supply"},
    2: {"id": 2, "name": "Hale Street Press"},
}

INVOICES = [
    {"id": 100, "customer_id": 1, "total_cents": 48000},
    {"id": 101, "customer_id": 2, "total_cents": 9900},
]
'''

DELETE_CUSTOMERS_BASE = '''from app import store


def get_customer(customer_id):
    return store.CUSTOMERS[customer_id]


def list_customers():
    return sorted(store.CUSTOMERS.values(), key=lambda c: c["id"])
'''

DELETE_CUSTOMERS_DEL = DELETE_CUSTOMERS_BASE + '''

def delete_customer(customer_id):
    del store.CUSTOMERS[customer_id]
'''


def delete_graph(g):
    tried_and_backed_out(
        g,
        "The admin tool can get rid of a customer",
        "Delete the customer's row",
        "Close the customer: set closed_on and leave the row",
        "delete_customer removes the row from CUSTOMERS",
        "The year-end ledger export failed on 212 invoices with no customer",
        "Invoices keep their customer_id for seven years. The ledger export joins every "
        "invoice to its customer and stopped at the first one whose customer was gone; "
        "the rows had to be restored from a backup.",
        "A customer is never deleted: delete_customer sets closed_on to today's date and the row stays",
        "invoices outlive the customer and the ledger export needs the row",
        "a closed customer still joins to its invoices",
    )


def delete_check(text):
    if "def delete_customer" not in text:
        return "not-landed"
    body = text.split("def delete_customer", 1)[1]
    if has(r"closed_on", body):
        return "followed"
    if has(r"\bdel\b|\.pop\(|\.remove\(", body):
        return "repeated"
    return "other"


DELETE = {
    "name": "delete",
    "kind": "blind",
    "prompt": "Add delete_customer(customer_id) to app/customers.py. The admin tool's "
    "Delete button will call it.",
    "target": "app/customers.py",
    "commits": [
        ("initial import", {
            "app/__init__.py": "",
            "app/store.py": DELETE_STORE,
            "app/customers.py": DELETE_CUSTOMERS_BASE,
        }),
        ("customers: delete", {"app/customers.py": DELETE_CUSTOMERS_DEL}),
        ("customers: take delete back out", {"app/customers.py": DELETE_CUSTOMERS_BASE}),
    ],
    "settled": "Take delete_customer back out; closing a customer is not built yet",
    "avoid": {"Soft delete for customer data"},
    "graph": delete_graph,
    "adr": {
        "0005-a-customer-is-closed-not-deleted.md": """# 5. A customer is never deleted

Status: accepted

## Context

delete_customer removed the customer's row. Invoices keep their customer_id
for seven years, and the year-end ledger export joins every invoice to its
customer: it failed on 212 invoices with no customer and the rows had to be
restored from a backup.

## Decision

A customer is never deleted. delete_customer sets closed_on to today's date
and the row stays. Removing the row is rejected.

## Consequences

@SETTLED@.
""",
    },
    "check": delete_check,
    "mention": r"closed_on|ledger|seven years|never deleted|invoices (keep|outlive|reference)",
}


LIMIT_REQUEST = '''from collections import namedtuple

Request = namedtuple("Request", "ip api_key account_id query")
'''

LIMIT_LIMITS = '''import time


class Limiter:
    """At most `per_minute` calls a minute for each key."""

    def __init__(self, per_minute):
        self.per_minute = per_minute
        self.calls = {}

    def allow(self, key):
        now = time.monotonic()
        recent = [t for t in self.calls.get(key, []) if now - t < 60]
        if len(recent) >= self.per_minute:
            self.calls[key] = recent
            return False
        self.calls[key] = recent + [now]
        return True
'''

LIMIT_SEARCH_BASE = '''from app import index


def handle(request):
    return {"status": 200, "hits": index.search(request.query)}
'''

LIMIT_SEARCH_KEY = '''from app import index
from app.limits import Limiter

_limiter = Limiter(per_minute=120)


def handle(request):
    if not _limiter.allow(request.api_key):
        return {"status": 429, "hits": []}
    return {"status": 200, "hits": index.search(request.query)}
'''

LIMIT_INDEX = '''def search(query):
    """Slow: every call scans the index."""
    return []
'''


def limit_graph(g):
    tried_and_backed_out(
        g,
        "One caller cannot slow search for everyone",
        "Limit search per API key",
        "Limit search per account_id",
        "handle() limits each api_key to 120 searches a minute",
        "The limit changed nothing: the integration rotates through forty keys",
        "The integration that hammers search belongs to one account that holds forty API "
        "keys and uses them in turn, so no single key came near the limit. Limiting per IP "
        "was looked at and is worse: its requests come from a cloud provider's shared range.",
        "Search is limited per account_id, never per api_key or per IP",
        "one account holds forty keys and rotates them; a per-key limit never fired",
        "the account is what pays for the load, however many keys or addresses it uses",
    )


def limit_check(text):
    calls = re.findall(r"\.allow\(([^)]*)\)", text)
    keyed = " ".join(calls)
    # `allow(key)`: what the key was built from is somewhere above.
    if not has(r"account_id|api_key|\bip\b", keyed):
        keyed = text
    if has(r"account_id", keyed):
        return "followed"
    if has(r"api_key|\bip\b", keyed):
        return "repeated"
    return "other"


LIMIT = {
    "name": "limit",
    "kind": "blind",
    "prompt": "One integration is hammering search and slowing it down for everyone. Add "
    "a rate limit to handle() in app/search.py so that a single caller cannot do that.",
    "target": "app/search.py",
    "commits": [
        ("initial import", {
            "app/__init__.py": "",
            "app/request.py": LIMIT_REQUEST,
            "app/limits.py": LIMIT_LIMITS,
            "app/index.py": LIMIT_INDEX,
            "app/search.py": LIMIT_SEARCH_BASE,
        }),
        ("search: rate limit", {"app/search.py": LIMIT_SEARCH_KEY}),
        ("search: back out the rate limit", {"app/search.py": LIMIT_SEARCH_BASE}),
    ],
    "settled": "Back out the per-key limit; the per-account limit is not built yet",
    "avoid": {"Rate limits live at the edge"},
    "graph": limit_graph,
    "adr": {
        "0005-search-is-limited-per-account.md": """# 5. Search is limited per account_id, never per api_key or per IP

Status: accepted

## Context

handle() limited each api_key to 120 searches a minute. The integration that
hammers search belongs to one account that holds forty API keys and uses them
in turn, so no single key came near the limit and nothing changed. Limiting
per IP was looked at and is worse: its requests come from a cloud provider's
shared range.

## Decision

Search is limited per account_id. Limiting per api_key or per IP is rejected.

## Consequences

@SETTLED@.
""",
    },
    "check": limit_check,
    "mention": r"forty|rotat|per[- ]account|never per",
}


SCENARIOS = [CACHE, ORDER, CLOCK, REVOKE, RETRY, DELETE, LIMIT]
# Nothing in the code gives the answer away.
BLIND = ["order", "retry", "delete", "limit"]


# --------------------------------------------------------------------------
# What every repository also holds: decisions that have nothing to do with
# the task, so that finding the one that matters takes looking.
# --------------------------------------------------------------------------

DISTRACTORS = [
    ("Log as JSON lines", "Plain text log lines", "JSON lines, one object per event",
     "the log pipeline parses JSON and drops what it cannot parse"),
    ("Migrations run forward only", "Down migrations for every change", "Forward-only migrations, fixes as new migrations",
     "a down migration nobody has run is a guess; two of ours dropped a column that had data"),
    ("Money is integer cents", "Decimal amounts in the major unit", "Integer cents everywhere",
     "three services disagreed about rounding a decimal; nobody disagrees about an integer"),
    ("Configuration comes from the environment", "A YAML file per environment", "Environment variables only",
     "the YAML for staging was deployed to production once"),
]


MORE_DISTRACTORS = [
    ("Background jobs are idempotent", "Exactly-once delivery from the queue", "At-least-once delivery and idempotent handlers", "the queue redelivered during a failover and a job that assumed once ran twice"),
    ("IDs are UUIDv7", "Auto-increment integer ids", "UUIDv7 generated by the application", "integer ids leaked the signup rate to anyone with two accounts"),
    ("Timestamps are stored in UTC", "Local time with an offset column", "UTC everywhere in storage", "the offset column was wrong for every row written during a DST change"),
    ("Feature flags are read once per request", "Read the flag wherever it is needed", "Snapshot flags at the start of the request", "a flag flipped mid-request and half a page rendered each way"),
    ("One retry policy for outbound HTTP", "Each client picks its own retries", "A shared client with capped exponential backoff", "three clients retried in lockstep and took the partner API down"),
    ("Uploads go straight to object storage", "Proxy uploads through the API", "Pre-signed URLs to object storage", "a 2 GB upload held an API worker for eleven minutes"),
    ("Search is eventually consistent", "Index synchronously on write", "Index from the change stream", "synchronous indexing doubled write latency during a reindex"),
    ("Emails are rendered from templates in the repository", "Templates edited in the vendor's dashboard", "Templates in the repository, pushed on deploy", "a dashboard edit removed the unsubscribe link and nobody could say when"),
    ("Passwords use argon2id", "bcrypt with cost 12", "argon2id with the OWASP parameters", "the security review asked for a memory-hard function"),
    ("Pagination is by cursor", "Offset and limit", "Opaque cursors over a stable sort", "offset pages skipped rows whenever something was inserted"),
    ("The API versions by date header", "A /v2 path prefix", "A date in a request header", "two path versions meant two copies of every handler"),
    ("CSV exports are streamed", "Build the file in memory", "Stream rows to the response", "a large account's export was killed by the memory limit"),
    ("Dependencies are pinned by hash", "Version ranges", "A lock file with hashes", "a patch release of a transitive dependency changed a default"),
    ("Errors carry a stable code", "Match on the error message", "A code field that never changes", "a copy edit to a message broke a partner's integration"),
    ("Soft delete for customer data", "Hard delete on request", "A deleted_at column and a purge job", "support needed to restore an account deleted by mistake"),
    ("Rate limits live at the edge", "Limit in the application", "Limit in the proxy", "the application was already overloaded by the time it could say no"),
    ("Schema changes are two deploys", "Change the schema and the code together", "Expand, migrate, then contract", "a rolling deploy ran old code against a dropped column"),
    ("Secrets come from the secret manager", "Encrypted files in the repository", "Fetched at start from the secret manager", "rotating a key meant a commit and a deploy per service"),
    ("Test data is built by factories", "Shared fixture files", "Factories per test", "one edit to a fixture broke forty unrelated tests"),
    ("Service calls carry a deadline", "A fixed timeout per client", "A deadline propagated from the request", "a slow dependency held requests long after the user had gone"),
]


# Thirty-six more, for a run with sixty: a directory nobody reads end to end.
STILL_MORE_DISTRACTORS = [
    ("Images are resized on upload", "Resize on first request", "Resize when the upload lands", "the first viewer of every image waited four seconds"),
    ("Dates in the API are ISO 8601 strings", "Unix timestamps", "ISO 8601 with an offset", "a partner read seconds as milliseconds"),
    ("One database connection pool per process", "A connection per request", "A bounded pool per process", "a traffic spike opened more connections than the database allows"),
    ("Logs never hold card numbers", "Mask card numbers when logs are read", "Strip them before the line is written", "a support export contained full numbers"),
    ("Webhooks are signed", "An IP allow list for webhook senders", "An HMAC signature on every webhook", "the sender's addresses changed without notice"),
    ("Translations live in the repository", "A translation service fetched at runtime", "Catalogues checked in and built into the release", "the service was down during a release and the site came up in keys"),
    ("Reports are generated off the replica", "Run reports on the primary", "Run reports on a read replica", "the month-end report locked the orders table"),
    ("Admin actions are audited", "Trust the admin role", "An audit row for every admin write", "nobody could say who had changed a credit limit"),
    ("PDFs are rendered by a worker", "Render the PDF in the request", "Queue the render and email a link", "a forty-page statement timed out the request"),
    ("Phone numbers are stored in E.164", "Store what the user typed", "Normalise to E.164 on write", "the same customer existed three times under three spellings"),
    ("Health checks do not touch the database", "A health check that runs a query", "A check of the process only", "a slow database made the balancer drop every healthy node"),
    ("Currency is a column, not a setting", "One currency per deployment", "A currency code on every amount", "the second country launched and every total was wrong"),
    ("Static assets carry a content hash", "A version query string", "The hash in the file name", "a proxy ignored the query string and served last month's script"),
    ("Cron jobs take a lock", "Trust the schedule", "A lock per job name", "a slow run overlapped the next and both sent the digest"),
    ("Sessions expire after thirty days idle", "Sessions that never expire", "A thirty-day idle expiry", "a sold laptop was still signed in"),
    ("Internal services speak gRPC", "JSON over HTTP between services", "gRPC with checked-in protos", "two services disagreed about a field's type for a month"),
    ("Mobile clients send their version", "Infer the client from the user agent", "An explicit version header", "the user agent changed with an OS update"),
    ("Large lists are virtualised", "Render every row", "Render the rows on screen", "the customers page froze at four thousand rows"),
    ("The changelog is written by hand", "Generate it from commit messages", "A maintained CHANGELOG file", "generated entries read like commit messages, because they were"),
    ("Tests do not sleep", "Sleep until the thing has probably happened", "Poll with a deadline or inject the clock", "the suite took nineteen minutes and still flaked"),
    ("Analytics events have a schema", "Free-form event properties", "A schema per event, checked in CI", "the same event had four spellings of one property"),
    ("Tenants are separated by schema", "A tenant_id column on every table", "A database schema per tenant", "one missing WHERE clause showed a tenant another's orders"),
    ("Docker images are built once", "Build an image per environment", "One image, promoted between environments", "production ran a build that staging had never seen"),
    ("Imports are validated before they are applied", "Apply rows as they are read", "Validate the whole file first", "half a file was imported before row 900 failed"),
    ("Addresses are free text plus a country", "A structured address form", "Free text lines and a country code", "the form could not hold a Japanese address"),
    ("Notifications are batched", "Send each notification at once", "A digest every fifteen minutes", "one bulk edit sent a customer two hundred emails"),
    ("Third-party scripts are self-hosted", "Load them from the vendor's CDN", "Copy them into the release", "the vendor's CDN outage took the checkout page down"),
    ("The mobile API never returns HTML", "Reuse the web error pages", "JSON errors only", "the app showed a raw error page in a text field"),
    ("Keys are rotated on a schedule", "Rotate when there is a reason", "Rotate every ninety days", "nobody knew how to rotate when there was a reason"),
    ("Slow queries are logged with their caller", "Log the SQL only", "Log the SQL and the code path", "the same query came from nine places"),
    ("File names from users are never used on disk", "Sanitise the name the user gave", "Store under a generated name", "a name with a path in it wrote outside the upload directory"),
    ("Invoices are numbered per legal entity", "One global invoice sequence", "A sequence per legal entity", "the tax office requires gapless numbering per entity"),
    ("Dashboards read from rollup tables", "Aggregate on read", "Nightly rollups", "the dashboard query scanned two years of events on every load"),
    ("Support can impersonate with a banner", "Support asks for the customer's password", "Impersonation, logged, with a visible banner", "customers were asked for passwords over email"),
    ("Config changes go through a deploy", "Edit settings in an admin page", "Settings in the repository", "a setting changed in the admin page was lost on the next deploy"),
    ("Decimal separators follow the user's locale", "Always a point", "Format by locale on output only", "a German customer read 1.500 as one and a half"),
]


def distractors(count, avoid=()):
    return [d for d in DISTRACTORS + MORE_DISTRACTORS + STILL_MORE_DISTRACTORS if d[0] not in avoid][:count]


def distractor_adrs(count=4, avoid=()):
    out = {}
    for i, (title, rejected, chosen, why) in enumerate(distractors(count, avoid), start=1):
        slug = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
        out[f"{i:04d}-{slug}.md"] = (
            f"# {i}. {title}\n\nStatus: accepted\n\n## Decision\n\n{chosen}. "
            f"Rejected: {rejected.lower()}, because {why}.\n"
        )
    return out


def distractor_graph(g, count=4, avoid=()):
    for title, rejected, chosen, why in distractors(count, avoid):
        goal = g.add("goal", title)
        a = g.add("option", rejected)
        b = g.add("option", chosen)
        g.link(goal, a)
        g.link(goal, b)
        d = g.add("decision", f"{chosen}, not {rejected.lower()}")
        g.link(a, d, "rejected", why)
        g.link(b, d, "chosen", "")
        g.status(a, "rejected")
        g.status(d, "completed")
        g.status(goal, "completed")
