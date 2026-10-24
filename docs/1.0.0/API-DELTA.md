# API delta — 1.0.0

What changed in the daemon's protocol and in the plugin contract since 0.6.0. A window and its
daemon are the same build by construction — the socket is named for the version and `Hello`
refuses any other, so it is `kiki-1.0.0.sock` now — and plugins never send requests, so nothing
here is kept as an alias: there is nobody an alias would protect. The full tables are
`docs/0.1.0/API-DAEMON.md` and `docs/0.1.0/API-PLUGIN.md`, and both carry these.

Nothing in the **window's** protocol changed this release. Both entries below are the plugin
contract, and one of them is a contract that was always written down and is now kept.

## A name in a listing must be an ordinary relative path (`01-from-the-fork.md`, item 3)

The shapes are unchanged. What changed is what a plugin may put in them, and what the daemon
does when it does not.

- **`Scan`** — `name` on an entry is one component of a path and never a path: no `/` anywhere
  in it. `rel`, which entries carry when the daemon asked `recursive: true`, is a relative path
  whose **every** component is an ordinary name: nothing empty (which is a leading, trailing or
  doubled `/`), no `.`, no `..`, and no NUL. An absolute path is refused by the same rule, its
  first component being empty.
- **A listing that breaks it fails the whole scan**, with error **1224**, rather than the
  offending entry being quietly dropped. The mirror's compare, a transfer's listing and the
  `Scan` behind them all stop and say so.
- The same rule is applied again at every point a name reaches the filesystem — the mirror's
  `enforce_guards`, `run_action` and `copy_file`, and the six sites in `transfer.rs` — so a
  plan built by some other route is checked too. A plugin that obeys the contract never meets any of
  it.

**This is stricter than what `API-PLUGIN.md` used to promise, and it is also now true.** That
document said "the daemon skips `.` and `..` if present". It did not: there was no such code,
and `..` in a `rel` was joined to the local root like any other name. So the line described a
filter that did not exist, and it is now a refusal that does.

**Why a refusal and not a filter.** A listing is one answer. An entry naming
`../../.ssh/authorized_keys` is not a quirk to route around — it means the thing on the other
end is not doing what this contract says, and the rest of its answer cannot be trusted either.
Dropping that one entry and mirroring the remaining several thousand is the worse outcome: a
hostile or broken server would get to choose which of its files kiki acted on, and the person
would see a successful run.

**Error 1224** — now "Safety: refusing a path that is not inside the transfer root: {path}", in
all three catalogs. It said "refusing to delete outside the replica root" and was raised only
for a delete; the check is no longer about deleting, and the `{path}` placeholder is unchanged.

**Error 1225** — new: "Safety: the transfer source is not a regular file: {path}", in all three
catalogs. A transfer reads a regular file and nothing else, and does not reach one through a
symlink. (It shared 1224 for a few hours, which would have told a person a path was outside the
transfer root when the truth was that it pointed at a directory or a device.)

## `Connect` with an unknown host key: no change, and SFTP now obeys it (item 5)

**The contract is what it always was** (`API-PLUGIN.md`, Sessions): *if the server's host key or
certificate is unknown, return `Invalid` with `field: "fingerprint"` and the fingerprint in
`message`; the daemon asks the user and retries `Connect` with `config.trustedFingerprint` set.*
Nothing about that changed, and a third-party plugin written to it needs no edit.

What changed is that the **SFTP plugin now does it.** It had been completing the handshake and
returning an ordinary reply carrying the key, leaving the daemon's own `key_verdict` to refuse
the session afterwards. The outcome looked the same — no session, and the window offering to
verify — but the refusal arrived after authentication, so the password, or a signature from the
user's key, had already been offered to a host nobody had ever verified. FTPS always used the
refusal. The two now answer the same question the same way, and
`locations::unaccepted_fingerprint` reads one shape rather than two.

- **`Connect` reply, `knownHost?: bool`** — documented here for the first time, having been sent
  and read since 0.2. It is optional and defaults to `false`. It is the one case where an
  unverified key is a *reply* rather than a refusal: the key is new to this location but
  `~/.ssh/known_hosts` already vouches for it, so there is nothing to ask and the daemon pins it
  and carries on (`KeyVerdict::Pin`). `true` with no `fingerprint` says nothing and is ignored.

## `open-in.toml`: `reuse` may be the marker `nvim-rpc` (item 2)

Configuration rather than wire, but it is a contract someone can write by hand.

- **`reuse`** on a tool is a shell template, as before, with one reserved value: `nvim-rpc`
  means "hand the file to this tool over Neovim's own RPC", and the daemon builds
  `nvim --server <that tool's socket> --remote-expr <expr>` as argv, with no shell. The Neovim
  preset carries it. Any other value is a shell template and is honoured as one, including for
  Neovim — `tests/e2e/run.sh` replaces it with `true` so a test run starts no editor, and that
  goes on working.
- A `reuse` containing `--remote-send` now logs one line saying what that does with a filename.
  It is a warning and changes nothing: the value is still run.
- The marker is keyed on the value, not on the tool's id, so a second, differently-named Neovim
  entry can ask for it on purpose.

## Removed

Nothing.
