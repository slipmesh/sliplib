# Code style

## Formatting and lints

- `cargo fmt` with [`rustfmt.toml`](../rustfmt.toml): edition 2024, Unix newlines, field-init and
  `?` shorthand.
- `cargo clippy --all-targets --all-features -- -D warnings` with [`clippy.toml`](../clippy.toml);
  a warning fails CI.
- Markdown follows [`.markdownlint-cli2.jsonc`](../.markdownlint-cli2.jsonc).

## Layout

- One crate per concern, in a directory with the short name (`runtime/`) and a package name with
  the `sliplib-` prefix (`sliplib-runtime`): a bare `runtime` would collide with crates of that name
  in a consumer's dependency graph.
- A crate holds only what more than one component needs. The logic of a single component stays in
  that component's repository; otherwise every fix to it is a release here before one there.
- Code that works only on one platform sits behind `#[cfg(...)]` on its module, so the rest of the
  crate still builds for a consumer that runs elsewhere.
- Tests sit in a `#[cfg(test)] mod tests` at the end of the file they test. A test that acts on the
  whole process, such as raising a signal, is a test binary of its own under `tests/`.

## Naming

- Types and functions say what they are for, not how: `Shutdown::requested`, not `recv_sigterm`.
- Tests are named for the behaviour they pin, as a sentence:
  `sigterm_is_reported_instead_of_ending_the_process`.

## Error handling

- A public function returns a typed error: `std::io::Error` where the failure is the operating
  system's, otherwise an error type of the crate's own. No `anyhow` in a public signature; the
  caller adds the context it knows.
- A panic only for misuse the caller can avoid, documented under the item, as Tokio does for a
  call outside its runtime.
- `unwrap`/`expect` only in tests.

## Comments

Code says what it does; a comment says only why it has to be that way - an external constraint or
a non-obvious consequence, including how an external component behaves. No history of how the code
got here, and no pointers to files or functions in other repositories.
