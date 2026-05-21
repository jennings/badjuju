.PHONY: fmt build test

fmt:
	pnpm biome format --write .
	cargo fmt --manifest-path server/Cargo.toml

build:
	pnpm turbo build

test:
	pnpm turbo test
