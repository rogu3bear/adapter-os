# 🔒 Safety Checklist: Preventing Code Breakage

## Pre-Change Verification
- [ ] **Understand the codebase structure** - Map dependencies and data flow
- [ ] **Identify impact scope** - Which components will be affected?
- [ ] **Check existing patterns** - How do similar features work?
- [ ] **Verify assumptions** - Test hypotheses before implementing
- [ ] **Backup current state** - Git commit before major changes

## Change Implementation
- [ ] **Small, incremental changes** - One logical unit at a time
- [ ] **Preserve existing behavior** - Don't break working functionality
- [ ] **Add tests first** - Write tests before implementation
- [ ] **Type safety** - Use strong typing to catch errors early
- [ ] **Error handling** - Comprehensive error handling for edge cases

## Post-Change Validation
- [ ] **Compile successfully** - No new compilation errors
- [ ] **Tests pass** - All existing and new tests pass
- [ ] **Manual verification** - Test the actual functionality
- [ ] **Peer review** - Get feedback on changes
- [ ] **Revert plan** - Know how to undo changes if needed

## Ongoing Safety Practices
- [ ] **Regular commits** - Small, frequent commits with clear messages
- [ ] **Documentation updates** - Keep docs in sync with code changes
- [ ] **Communication** - Explain changes and rationale
- [ ] **Monitoring** - Watch for unexpected side effects
- [ ] **Continuous learning** - Learn from mistakes and improve process
