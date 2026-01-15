# Repository Guidelines

## Project Structure & Module Organization
- `kernel/`: main kernel implementation and architecture-specific logic.
- `hal/`: hardware abstraction layer (pure logic + hardware drivers).
- `bootloader/`: bootloader placeholder (uses external bootloader crate).
- `userland/`: user-mode code and experiments.
- `kernel/tests/`: integration/QEMU tests (e.g., `*_smoke.rs`).
- `docs/` plus `ARCHITECTURE.md`, `IMPLEMENTATION.md`, `TESTING_GUIDE.md`: design and status docs.
- `scripts/`: developer helpers like `quality-gate.sh` and `qemu-test.sh`.

## Build, Test, and Development Commands
- `make build`: build the kernel (debug).
- `make release`: build optimized artifacts.
- `make bootimage`: create a bootable disk image.
- `make run`: build and run in QEMU.
- `make test`: run host tests + kernel tests.
- `make test-hal` / `make test-kernel`: focused test suites.
- `make fmt` / `make fmt-check`: format or verify formatting.
- `make clippy`: run lints with `-D warnings`.
- `./scripts/quality-gate.sh`: formatting, clippy, host tests, unsafe checks.
- `make install-deps`: install `bootimage` (QEMU optional).

## Coding Style & Naming Conventions
- Rust 2021 edition; `rustfmt.toml` enforces 4-space indentation and 100-char lines.
- Follow standard Rust naming: `snake_case` for functions/modules, `CamelCase` for types.
- Unsafe code is restricted to arch/drivers; every unsafe block requires a `// SAFETY:` comment.
- Avoid globals for core subsystems and avoid allocation before heap init.

## Testing Guidelines
- Unit tests live alongside code (`#[cfg(test)] mod tests`) and run on host.
- Integration tests live in `kernel/tests/` and typically use `*_smoke.rs` naming.
- QEMU tests should emit `TEST PASS <name>` / `TEST FAIL <name>` and use `#[test_case]`.
- Quick run: `make test` or `cargo test --lib --workspace --target x86_64-unknown-linux-gnu`.

## Commit & Pull Request Guidelines
- Use conventional commits (e.g., `feat:`, `fix:`, `docs:`, `test:`, `refactor:`).
- Run the quality gate before pushing.
- PRs should explain scope, link issues when applicable, and include test results.
- Architecture changes require updating `ARCHITECTURE.md` and prior discussion.

## Knowledge Graph Memory (MCP)
- Use the memory server to persist stable, user-approved facts that improve future coding help.
- Good candidates: long-lived preferences (tooling, formatting, testing), environment details (OS, shell),
  project conventions, and recurring decisions or constraints.
- Avoid: secrets, tokens, passwords, or transient context tied to a single task.
- Keep observations atomic (one fact per observation) and add/modify only after confirming with the user.
- Prefer searching existing nodes before creating new entities or relations to avoid duplicates.
