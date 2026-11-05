# Contributing

Issues and pull requests are welcome.

- **Read [`AGENTS.md`](AGENTS.md) first.** It holds the porting rules: what the crate owns, what it
  must not do, and how it is checked.
- **The port is bit identical to the C++, and stays so.** A change must keep all 106 reference
  cases passing, bit for bit. A fix that makes it sound better but differ from Elements is not a
  fix here. An improvement beyond Elements belongs in the code that uses this crate.
- **Port from the C++, not from memory**, and not from another port: the commits are in
  `AGENTS.md`.
- **Check before you send:**

  ```bash
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  cargo test
  ```

By contributing you agree that your contribution is licensed under the MIT licence of this
repository.
