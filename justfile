set shell := ["fish", "-Nc"]

fmt:
	-rustfmt --edition 2021 (fd -e rs)
	-ruff format (fd -e py -E cbor2)

lint: clippy ruff mypy

clippy:
	#!/usr/bin/env -S fish -N
	for crate in (fd -a cargo.toml)
		cd (dirname $crate)
		cargo clippy
	end

test:
	#!/usr/bin/env -S fish -N
	for crate in (fd -a cargo.toml)
		cd (dirname $crate)
		cargo test
	end

ruff:
	-ruff check (fd -e py -E cbor2)

mypy:
	#!/usr/bin/env -S fish -N
	cd components/analysis
	source .venv/bin/activate.fish
	mypy .
