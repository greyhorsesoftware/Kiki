#!/usr/bin/env python3
"""Runs the end-to-end flows (plan 28).

Each flow gets its own folder under the fixture root and its own listing ids, so one flow can
never see another's files. A flow that needs something this run does not have — a shell, a
virtual keyboard — is skipped by name rather than failed, and the summary says so.

    driver.py <socket> <out-dir> [--daemon-only] [--flow NAME ...]
"""
import importlib, os, shutil, sys, traceback  # noqa: F401

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import Checks, Daemon, Pointer, Shell, make_tree  # noqa: E402

FLOWS = [
    # daemon only
    "listing", "trash", "archive", "collisions", "edge_cases", "cross_fs", "git_status", "remote_transfers",
    # through the window
    "ops_menu", "ops_keyboard", "confirm_ops", "trash_view", "archive_ui", "view_state",
    "side_by_side", "launcher",
    "pointer_ops",
]


class Ctx:
    """What a flow is handed: a daemon, a shell, a place to build fixtures, somewhere to record."""

    def __init__(self, name, daemon, shell, root):
        self.checks = Checks(name)
        self.daemon = daemon
        self.shell = shell
        self.pointer = Pointer(shell)
        self.base = root
        self.root = root
        self._lids = []
        self._fixtures = 0

    def fixture(self, spec):
        """Build a fixture under a path no run has used before.

        Never reuse a path: listings are cached and shared, so a folder deleted and rebuilt under
        the same name can be served from the cache as it was."""
        self._fixtures += 1
        self.root = os.path.join(self.base, str(self._fixtures))
        return make_tree(self.root, spec)

    def lid(self, back=None):
        """A fresh listing id, or one already handed out (0 is the first)."""
        if back is None:
            self._lids.append(1000 * (len(self._lids) + 1) + len(self._lids))
            return self._lids[-1]
        return self._lids[back]


def have(capability, args):
    if capability == "daemon":
        return True
    if capability == "shell":
        return "--daemon-only" not in args
    if capability == "keyboard":
        return "--daemon-only" not in args and shutil.which("wtype") is not None
    if capability == "pointer":
        return "--daemon-only" not in args and shutil.which("wlrctl") is not None
    return False


def main():
    args = sys.argv[1:]
    sock_path, out_dir = args[0], args[1]
    wanted = [args[i + 1] for i, a in enumerate(args) if a == "--flow"] or FLOWS
    home = os.environ["HOME_FIXTURE"]
    os.makedirs(out_dir, exist_ok=True)

    daemon = Daemon(sock_path)
    shell = Shell()
    failures, skipped, passes = [], [], 0

    for name in wanted:
        mod = importlib.import_module("flows." + name)
        missing = [n for n in getattr(mod, "NEEDS", set()) if not have(n, args)]
        title = getattr(mod, "TITLE", name)
        if missing:
            skipped.append(f"{name} ({', '.join(missing)} not available)")
            print(f"SKIP {title}: needs {', '.join(missing)}")
            continue
        probe = getattr(mod, "probe", None)
        if probe:
            ctx0 = Ctx(name, daemon, shell, os.path.join(home, f"{name}-probe-{os.getpid()}"))
            shell.call("dismiss")
            reason = probe(ctx0)
            if reason:
                skipped.append(f"{name} ({reason})")
                print(f"SKIP {title}: {reason}")
                continue
        print(f"\n{title} [{name}]")
        if "shell" in getattr(mod, "NEEDS", set()):
            shell.call("dismiss")      # no flow inherits an editor or a menu from the last one
        # The pid keeps a path from ever being reused between runs: the daemon caches listings
        # by URI, and a folder deleted and rebuilt at the same path can be served as it was.
        ctx = Ctx(name, daemon, shell, os.path.join(home, f"{name}-{os.getpid()}"))
        try:
            mod.run(ctx)
        except Exception:
            ctx.checks.failures.append(f"{name} raised")
            traceback.print_exc()
        passes += ctx.checks.passes
        failures += [f"{name}: {f}" for f in ctx.checks.failures]

    print("\n" + "-" * 60)
    print(f"{passes} passed, {len(failures)} failed, {len(skipped)} skipped")
    for f in failures:
        print("  FAILED  " + f)
    for s in skipped:
        print("  skipped " + s)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
