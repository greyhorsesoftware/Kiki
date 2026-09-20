"""A large folder up to a real server and down again, over SFTP (`sshd`) and over FTPS (`vsftpd`) (plan 31, phase 4).
Not in the default run:

    tests/e2e/run.sh --flow transfer_remote
    KIKI_TRANSFER_PROFILE=transfer tests/e2e/run.sh --flow transfer_remote

What a server cannot hold is part of the test: FTP has no way to name a file with a newline in it,
and neither protocol here makes a symlink — those must fail or be left out BY NAME, with everything
else arriving, rather than ending the transfer or arriving as something else.
"""
import getpass, os
from servers import FTPS_PASSWORD, FTPS_USER, Servers, add_location, missing, sshd_bin, vsftpd_bin
from transfer_common import generate, run_transfer, cancel_midway

NEEDS = {"daemon"}


def probe(ctx):
    return missing()
TITLE = "a large transfer, up to a server and down again"

# The generator's awkward folder, by what each kind of server can and cannot take.
NEWLINE = "awkward/line\nbreak.txt"
LINK = "awkward/link-to-target"
# A name that is not valid UTF-8 cannot be transferred in 0.1.0 (a transfer knows files by text
# paths): it must fail by name, saying why, with everything else arriving.
NOT_UTF8 = "awkward/not-utf8-\udcff\udcfe.txt"


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    base = ctx.base
    os.makedirs(base, exist_ok=True)
    src = os.path.join(base, "big")
    manifest = generate(src)
    print(f"  fixture: {manifest['files']} files in {manifest['folders']} folders, {int(manifest['large_bytes']) >> 20} MB in {manifest['large']} large files")
    servers = Servers(base)
    ends = {}
    try:
        if sshd_bin():
            root = os.path.join(base, "sftp-root"); os.makedirs(root)
            port, key = servers.start_sftp()
            r = add_location(d, {"name": "big-sftp", "plugin": "sftp", "remoteUri": "sftp://big-sftp" + root, "localUri": "",
                                 "config": {"host": "127.0.0.1", "port": str(port), "username": getpass.getuser(), "auth": "key", "identityFile": key}}, {})
            if "ok" in r:
                ends["sftp"] = (root, lambda rel, root=root: "sftp://big-sftp" + os.path.join(root, rel))
        if vsftpd_bin():
            root = os.path.join(base, "ftps-root"); os.makedirs(root)
            port = servers.start_ftps(root)
            r = add_location(d, {"name": "big-ftps", "plugin": "ftps", "remoteUri": "ftps://big-ftps" + root, "localUri": "",
                                 "config": {"host": "127.0.0.1", "port": str(port), "username": FTPS_USER, "encryption": "Explicit TLS (AUTH TLS)"}}, {"password": FTPS_PASSWORD})
            if "ok" in r:
                ends["ftps"] = (root, lambda rel, root=root: "ftps://big-ftps" + os.path.join(root, rel))
        c.check("at least one server could be started", bool(ends), missing() or "")

        only = os.environ.get("KIKI_TRANSFER_ONLY")
        for tag, (root, uri) in ends.items():
            if only and tag != only:
                continue
            os.makedirs(os.path.join(root, "up"))
            # Up. A symlink is not followed and not made; FTP cannot say a name with a newline.
            up_dst = os.path.join(root, "up")
            run_transfer(ctx, f"{tag} upload", "file://" + src, uri("up"), src, os.path.join(up_dst, "big"),
                         expect_missing=[NOT_UTF8] + ([NEWLINE] if tag == "ftps" else []), tolerate=[LINK])

            # Down again, from what arrived.
            down = os.path.join(base, f"down-{tag}"); os.makedirs(down)
            run_transfer(ctx, f"{tag} download", uri("up/big"), "file://" + down, os.path.join(up_dst, "big"), os.path.join(down, "big"), tolerate=[])

            os.makedirs(os.path.join(root, "part"))
            cancel_midway(ctx, f"{tag} upload", "file://" + src, uri("part"), os.path.join(root, "part"))
            lid = ctx.lid()
            ok = "ok" in d.call("Open", lid=lid, uri=uri("up"))
            c.check(f"{tag}: and the location still lists after the cancel", ok)
            d.call("Close", lid=lid)
    finally:
        for name in ("big-sftp", "big-ftps"):
            d.call("RemoveLocation", name=name)
        servers.stop()
