# nvmod-fixtures removed (#190)

- **Question:** does anything still use the crate that wrote the world for `nvmod setup` and the TTW installer?
- **Decision:** the crate `nvmod-fixtures` and its workspace entry are deleted. Test fixtures are built in code by `nvmod-testkit`.
- **Why:** both commands are gone (#164, #165), the crate only emitted arguments for a command that exits 5, and nothing in this repository, the Toolbox or the other repositories under the owner's clones uses it (grepped). Git history has it back.
- **Revisit:** no.
