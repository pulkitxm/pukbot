# Contributing

Run the following checks before opening a pull request:

    cargo fmt --all -- --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo test --locked
    cargo package --locked
    sh tests/install.sh

Keep pull request descriptions to one line and use conventional commit
subjects.
