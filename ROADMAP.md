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
