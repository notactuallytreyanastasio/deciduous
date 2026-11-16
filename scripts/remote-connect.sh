#!/usr/bin/env bash
# Connect this machine, and the repository you run it in, to a shared deciduous
# server. Nothing here is specific to one organization: the server URL and the
# token are derived at run time with the tools you already have, never typed
# into a file or a command line.
#
# USAGE
#   scripts/remote-connect.sh --url https://deciduous.example.com [options]
#   scripts/remote-connect.sh --containerapp NAME --resource-group RG [options]
#
#   Where the token comes from (first that applies):
#     --keyvault VAULT --secret NAME   az keyvault secret show, piped straight into deciduous
#     DECIDUOUS_MCP_TOKEN              already in the environment
#     stdin                            typed with echo off, or piped
#
#   Options
#     --workspace NAME     workspace for this repository (default: the git root's directory name)
#     --no-mcp             do not register the server with Claude Code
#     --all-worktrees      also pull and check every git worktree of this repository
#     --skip-worktree-config
#                          keep .deciduous/config.toml out of commits (git update-index
#                          --skip-worktree) when this repository tracks it and you do not
#                          want the URL committed
#     -h, --help
#
# WHAT IT DOES, IN ORDER
#   1. deciduous remote login --url URL     token from the source above, verified against
#                                           the server, stored mode 0600 outside every repo
#   2. deciduous remote init URL --workspace NAME   unless .deciduous/config.toml already
#                                           points at this URL (a committed [remote] wins)
#   3. deciduous remote pull; deciduous remote status
#   4. deciduous remote setup --url URL     registers the MCP server with Claude Code at
#                                           user scope (skipped with --no-mcp)
#
# Requires deciduous >= 1.0.8 on PATH; az only for --keyvault or --containerapp.
set -euo pipefail

die() { echo "remote-connect: $*" >&2; exit 1; }
info() { echo "remote-connect: $*"; }

URL=""; APP=""; RG=""; VAULT=""; SECRET=""; WORKSPACE=""; MCP=true; ALL_WT=false; SKIP_WT=false
while [ $# -gt 0 ]; do
  case "$1" in
    --url) URL="${2:-}"; shift 2 ;;
    --containerapp) APP="${2:-}"; shift 2 ;;
    --resource-group) RG="${2:-}"; shift 2 ;;
    --keyvault) VAULT="${2:-}"; shift 2 ;;
    --secret) SECRET="${2:-}"; shift 2 ;;
    --workspace) WORKSPACE="${2:-}"; shift 2 ;;
    --no-mcp) MCP=false; shift ;;
    --all-worktrees) ALL_WT=true; shift ;;
    --skip-worktree-config) SKIP_WT=true; shift ;;
    -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

command -v deciduous >/dev/null 2>&1 || die "deciduous is not on PATH (cargo install deciduous)"
version="$(deciduous --version | awk '{print $2}')"
case "$version" in
  0.*|1.0.[0-7]) die "deciduous $version is too old; remote login/setup need 1.0.8 or later" ;;
esac

# --- the URL: given, or read off the Container App -------------------------
if [ -z "$URL" ]; then
  [ -n "$APP" ] && [ -n "$RG" ] || die "give --url, or --containerapp NAME --resource-group RG"
  command -v az >/dev/null 2>&1 || die "az is needed to look up the Container App"
  fqdn="$(az containerapp show -n "$APP" -g "$RG" --query properties.configuration.ingress.fqdn -o tsv 2>/dev/null)" \
    || die "could not read Container App $APP in $RG (az login? right subscription?)"
  [ -n "$fqdn" ] || die "Container App $APP has no external ingress"
  URL="https://$fqdn"
fi
case "$URL" in https://*|http://127.0.0.1*|http://localhost*) ;; *) die "URL must be https (or loopback for a local server): $URL" ;; esac
URL="${URL%/}"
info "server: $URL"
curl -fsS -m 10 "$URL/health" >/dev/null 2>&1 || die "$URL/health does not answer; is the server up and reachable from here?"

# --- the token: Key Vault, environment, or stdin; never argv, never echoed --
token_source=""
if [ -n "$VAULT" ] || [ -n "$SECRET" ]; then
  [ -n "$VAULT" ] && [ -n "$SECRET" ] || die "--keyvault and --secret go together"
  command -v az >/dev/null 2>&1 || die "az is needed for --keyvault"
  az account show >/dev/null 2>&1 || die "not logged in to Azure (az login)"
  token_source="key vault $VAULT/$SECRET"
  login() { az keyvault secret show --vault-name "$VAULT" --name "$SECRET" --query value -o tsv | deciduous remote login --url "$URL"; }
elif [ -n "${DECIDUOUS_MCP_TOKEN:-}" ]; then
  token_source="DECIDUOUS_MCP_TOKEN"
  login() { printf '%s' "$DECIDUOUS_MCP_TOKEN" | deciduous remote login --url "$URL"; }
else
  token_source="stdin"
  if [ -t 0 ]; then
    login() { printf 'Token for %s (not echoed): ' "$URL" >&2; local t; IFS= read -r -s t; echo >&2; printf '%s' "$t" | deciduous remote login --url "$URL"; }
  else
    login() { deciduous remote login --url "$URL"; }
  fi
fi
info "1/4 storing the token (from $token_source), verified against the server"
login >/dev/null || die "remote login failed: wrong token, or the server refused it"

# --- this repository --------------------------------------------------------
root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [ -z "$root" ]; then
  info "not inside a git repository; token stored, nothing to point at the server"
else
  cd "$root"
  [ -n "$WORKSPACE" ] || WORKSPACE="$(basename "$(git rev-parse --path-format=absolute --git-common-dir | sed 's#/\.git$##')")"
  cfg=".deciduous/config.toml"
  if [ -f "$cfg" ] && grep -q "^url *= *\"$URL\"" "$cfg"; then
    info "2/4 $cfg already points at this server; keeping it"
  else
    info "2/4 pointing this checkout at the server (workspace $WORKSPACE)"
    deciduous remote init "$URL" --workspace "$WORKSPACE"
    if [ "$SKIP_WT" = true ] && git ls-files --error-unmatch "$cfg" >/dev/null 2>&1; then
      git update-index --skip-worktree "$cfg"
      info "    $cfg marked skip-worktree: the URL stays out of your commits"
    fi
  fi
  info "3/4 pulling the server's copy and comparing"
  deciduous remote pull
  deciduous remote status || info "    status exits 1 when anything differs; read the lines above"
  if [ "$ALL_WT" = true ]; then
    git worktree list --porcelain | awk '/^worktree /{print $2}' | while read -r wt; do
      [ "$wt" = "$root" ] && continue
      [ -d "$wt/.deciduous" ] || continue
      info "    worktree $wt"
      ( cd "$wt" && deciduous remote pull >/dev/null && deciduous remote status | tail -1 ) || true
    done
  fi
fi

# --- Claude Code ------------------------------------------------------------
if [ "$MCP" = true ]; then
  info "4/4 registering the MCP server with Claude Code (user scope)"
  deciduous remote setup --url "$URL" </dev/null | grep -E 'Registered|Unchanged|Claude Code' || true
  info "    restart Claude Code so it connects to $URL"
else
  info "4/4 skipped Claude Code registration (--no-mcp)"
fi
info "done"
