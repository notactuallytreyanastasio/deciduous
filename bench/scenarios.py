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
                was made about a different file
  prompt        the task, which never names either approach
  target        the file the task should change
  commits       [(message, {path: content})], applied in order
  hard_last     a replacement for the last commit in which the approach that
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


SCENARIOS = [CACHE, ORDER, CLOCK, REVOKE]


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


def distractors(count):
    return (DISTRACTORS + MORE_DISTRACTORS)[:count]


def distractor_adrs(count=4):
    out = {}
    for i, (title, rejected, chosen, why) in enumerate(distractors(count), start=1):
        slug = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
        out[f"{i:04d}-{slug}.md"] = (
            f"# {i}. {title}\n\nStatus: accepted\n\n## Decision\n\n{chosen}. "
            f"Rejected: {rejected.lower()}, because {why}.\n"
        )
    return out


def distractor_graph(g, count=4):
    for title, rejected, chosen, why in distractors(count):
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
