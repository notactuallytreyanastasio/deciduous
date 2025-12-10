# Deciduous Roadmap

## Backlog

### Clean up legacy "losselot" references
- [ ] Update `.claude/commands/decision.md` to use `deciduous` binary instead of `./target/release/losselot`
- [ ] Update `.claude/commands/context.md` to use `deciduous` binary instead of `./target/release/losselot`
- [ ] Update `CLAUDE.md` to remove any losselot-specific references
- [ ] Ensure templates in `src/init.rs` use installed binary (`deciduous`) rather than build paths
- [ ] Update live graph URLs to point to correct project

### Future Enhancements
- [ ] Support for additional editors (Cursor, Copilot, etc.)
- [ ] `deciduous init --all` to bootstrap for all supported editors at once

### Commit-Centric Graph Navigation
- [ ] More robust per-commit tooling
  - Link nodes to commits more reliably
  - Auto-detect commit context when logging actions/outcomes
  - `deciduous add action "..." --commit HEAD` should be the default flow
- [ ] Commit-centric browsing in the web viewer
  - Browse graph starting from commits (not just nodes)
  - Show which nodes are associated with each commit
  - Timeline view organized by commit history
  - Click a commit → see the decision subgraph around it
- [ ] `deciduous log` command to show commits with their linked nodes
- [ ] Integration with `git log` to annotate commits with decision context
