# Development

## Setup

```bash
uv sync --group dev         # install Python tooling
uv run maturin develop
```

`maturin develop` builds and installs the Rust extension into the uv environment so mkdocstrings can import `bitalino_rs`.

## Run tests

```bash
uv run cargo test --all
```

## Build docs locally

```bash
cargo doc --no-deps --all-features
mkdir -p docs/rust-docs
cp -r target/doc/* docs/rust-docs/
echo "*" > docs/rust-docs/.gitignore  # ignore generated files
uv run mkdocs serve
```
