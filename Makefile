.PHONY: check test package installers ci

check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features --locked -- -D warnings
	cargo check --all-targets --all-features --locked

test:
	cargo test --all-features --locked

package:
	cargo package --locked

installers:
	sh tests/install.sh

ci: check test package installers
