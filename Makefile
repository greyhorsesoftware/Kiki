# kiki — one entry point for building, running and testing (plans 11 and 28).
#
#   make            build the daemon and the plugins this build ships
#   make test       every suite: Rust, QML, end to end
#   make run        a daemon and a shell against this checkout
#
# `make test` is what CI runs and what a release needs to be green.

QMLTESTRUNNER ?= /usr/lib/qt6/bin/qmltestrunner
CARGO ?= cargo
QS ?= qs
ROOT := $(shell pwd)

.PHONY: all build test test-rust test-qml test-e2e coverage run daemon shell fmt lint clean

all: build

## Build the default members: kikid, the share and service plugins, and the location kinds in
## plugin::LOCATION_KINDS. Other location plugins build with `cargo build -p …`.
build:
	$(CARGO) build --release

test: test-rust test-qml test-e2e

test-rust:
	$(CARGO) test

## Leaf and interaction tests: no compositor, about five seconds.
test-qml:
	$(QMLTESTRUNNER) -import tests/qml/stubs -input tests/qml

## End to end against a real daemon and a real tree. Without cage it runs the daemon-only flows
## and says which it skipped.
test-e2e: build
	tests/e2e/run.sh

run: build
	KIKI_PLUGIN_DIR=$(ROOT)/target/release ./target/release/kikid & \
	sleep 0.5; $(QS) -p qml/shell.qml

daemon: build
	KIKI_PLUGIN_DIR=$(ROOT)/target/release ./target/release/kikid

shell:
	$(QS) -p qml/shell.qml

fmt:
	$(CARGO) fmt --all

lint:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --all-targets -- -D warnings

clean:
	$(CARGO) clean
	rm -rf tests/e2e/out

# Line coverage for the daemon, with the LLVM tools that ship with the system toolchain
# (no cargo-llvm-cov needed). Writes a summary and leaves the profile data for `llvm-cov show`.
coverage:
	@rm -rf target/coverage && mkdir -p target/coverage
	@RUSTFLAGS="-C instrument-coverage" LLVM_PROFILE_FILE="$(PWD)/target/coverage/k-%p-%m.profraw" \
		$(CARGO) test -p kikid --tests --no-run --message-format=json 2>/dev/null \
		| python3 -c "import json,sys;[print(m['executable']) for m in (json.loads(l) for l in sys.stdin if l.startswith('{')) if m.get('profile',{}).get('test') and m.get('executable')]" \
		> target/coverage/bins.txt
	@LLVM_PROFILE_FILE="$(PWD)/target/coverage/k-%p-%m.profraw" KIKI_PLUGIN_DIR="$(PWD)/target/debug" \
		sh -c 'while read b; do "$$b" --include-ignored >/dev/null 2>&1; done < target/coverage/bins.txt'
	@llvm-profdata merge -sparse target/coverage/*.profraw -o target/coverage/all.profdata
	@llvm-cov report --instr-profile=target/coverage/all.profdata \
		$$(sed 's/^/-object /' target/coverage/bins.txt | tr '\n' ' ') \
		--ignore-filename-regex='(/\.cargo/|/rustc/|/tests?\.rs$$)'
