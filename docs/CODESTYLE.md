# Code style

## Formatting and lints

- `cargo fmt` with [`rustfmt.toml`](../rustfmt.toml): edition 2024, Unix newlines, field-init and
  `?` shorthand.
- `cargo clippy --all-targets --all-features -- -D warnings` with [`clippy.toml`](../clippy.toml);
  a warning fails CI.
- Markdown follows [`.markdownlint-cli2.jsonc`](../.markdownlint-cli2.jsonc).

## Layout

- One crate per concern, in a directory with the short name (`secrets/`) and a package name with
  the `sliplib-` prefix (`sliplib-secrets`): a bare `secrets` would collide with crates of that
  name in a consumer's dependency graph.
- A crate holds only what more than one component needs, and only what no maintained crate already
  does. The logic of a single component stays in that component's repository; otherwise every fix
  to it is a release here before one there.
- Tests sit in a `#[cfg(test)] mod tests` at the end of the file they test.

## Naming

- Types and functions say what they are for, not how: `PrivateKey::public_key`, not
  `derive_x25519`.
- Tests are named for the behaviour they pin, as a sentence:
  `a_key_equals_its_clamped_form`.

## Error handling

- A public function returns a typed error of the crate's own, with `Display` and `source()`: no
  `anyhow` in a public signature, so the caller adds the context it knows.
- `unwrap`/`expect` only in tests.

## Tests

- A test pins behaviour of this repository's code: not another crate's, not an error message's
  wording, nothing that holds by construction.
- `cargo mutants` leaves no viable mutant missed; what it may skip is listed with the reason in
  [`.cargo/mutants.toml`](../.cargo/mutants.toml).
- A parser of outside input written here, not one a dependency provides, gets a
  [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) target that asserts a property, not only
  the absence of a panic.

## Comments

Code says what it does; a comment says only why it has to be that way - an external constraint or
a non-obvious consequence, including how an external component behaves. No history of how the code
got here, and no pointers to files or functions in other repositories.
