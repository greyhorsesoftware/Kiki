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

.PHONY: all build test test-rust test-qml test-e2e run daemon shell fmt lint clean

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
