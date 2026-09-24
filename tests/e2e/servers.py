"""Throwaway servers for the flows that need a real one — the real ones: OpenSSH's `sshd`,
`vsftpd` and Samba's `smbd`, each run as the user running the tests, on a high port, with its own
keys and config, no root. Each serves a folder of the fixture, so a transfer is checked where it
matters — on disk.

    sudo pacman -S openssh vsftpd samba

A flow whose server is not installed skips those pairs by name. There is no stand-in: an FTP
server written in Python was used here once, and under a few hundred files in quick succession it
stopped answering — which looked for all the world like kiki hanging on a large upload.
"""
import getpass, os, shutil, socket, subprocess
from harness import wait_for

HERE = os.path.dirname(os.path.abspath(__file__))


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def vsftpd_bin():
    return shutil.which("vsftpd") or next((p for p in ["/usr/bin/vsftpd", "/usr/sbin/vsftpd"] if os.path.exists(p)), None)


def sshd_bin():
    return shutil.which("sshd") or next((p for p in ["/usr/bin/sshd", "/usr/sbin/sshd"] if os.path.exists(p)), None)


def smbd_bin():
    return shutil.which("smbd") or next((p for p in ["/usr/bin/smbd", "/usr/sbin/smbd"] if os.path.exists(p)), None)


def pdbedit_bin():
    return shutil.which("pdbedit") or next((p for p in ["/usr/bin/pdbedit", "/usr/sbin/pdbedit"] if os.path.exists(p)), None)


def gvfsd_bin():
    """GVfs's session daemon, which is what kiki's SMB really talks to (plan 25)."""
    return next((p for p in ["/usr/lib/gvfsd", "/usr/libexec/gvfsd", "/usr/lib/gvfs/gvfsd"] if os.path.exists(p)), None)


def gvfsd_smb_bin():
    return next((p for p in ["/usr/lib/gvfsd-smb", "/usr/libexec/gvfsd-smb", "/usr/lib/gvfs/gvfsd-smb"] if os.path.exists(p)), None)


def missing():
    """Why no server can be started here, or None when at least one can."""
    if not sshd_bin() and not vsftpd_bin():
        return "neither sshd nor vsftpd is installed (sudo pacman -S openssh vsftpd)"
    return None


# Who a flow signs in to the FTPS server as. vsftpd runs here as an ordinary user, so it cannot
# check a system password (that is PAM's, and root's): the account is its anonymous one, given
# the run of the fixture's folder. kiki still sends a password — anonymous FTP takes any — so
# "the password is in no log" is still a test of something.
FTPS_USER, FTPS_PASSWORD = "anonymous", "s3cret"

# And the SMB server. `smbd` without root cannot change uid, so every share is forced to the user
# running the tests and the account is that user's name — but the password is Samba's own, kept in
# a passdb of the run's, never the machine's, so a wrong one is really refused.
SMB_PASSWORD = "kiki-e2e-smb"
SMB_SHARE, SMB_GUEST_SHARE = "files", "pub"


def smb_user():
    return getpass.getuser()


def remember(pid):
    """Note a server's pid where the harness will find it: a flow that raises, or is killed with its
    compositor, never reaches its own `stop()`, and a server left behind lives for ever. `run.sh`
    kills whatever is listed here when the run ends, however it ends."""
    path = os.environ.get("KIKI_E2E_PIDS")
    if path and pid:
        with open(path, "a") as f:
            f.write(f"{pid}\n")


class Servers:
    def __init__(self, base):
        self.base = base
        self.procs = []
        self.sshd_pid = None

    def start_sftp(self):
        d = os.path.join(self.base, "sshd")
        os.makedirs(d)
        for name in ("host_key", "client_key"):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", os.path.join(d, name)], check=True)
        shutil.copy(os.path.join(d, "client_key.pub"), os.path.join(d, "authorized_keys"))
        os.chmod(os.path.join(d, "authorized_keys"), 0o600)
        port = free_port()
        with open(os.path.join(d, "sshd_config"), "w") as f:
            f.write(f"Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/host_key\nPidFile {d}/sshd.pid\n"
                    f"AuthorizedKeysFile {d}/authorized_keys\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n"
                    "UsePAM no\nStrictModes no\nSubsystem sftp internal-sftp\n")
        subprocess.run([sshd_bin(), "-f", os.path.join(d, "sshd_config"), "-E", os.path.join(d, "sshd.log")], check=True)
        pid = wait_for(lambda: os.path.exists(f"{d}/sshd.pid") and open(f"{d}/sshd.pid").read().strip(), what="sshd up")
        self.sshd_pid = int(pid) if pid else None
        remember(self.sshd_pid)
        return port, os.path.join(d, "client_key")

    def start_ftps(self, root):
        """vsftpd over explicit TLS, as the launching user, serving `root` read-write.

        Unprivileged, it cannot chroot: `anon_root` is only where a session starts, and "/" is the
        machine's own. A flow addresses the fixture by its full path, as it does over SFTP."""
        import getpass
        d = os.path.join(self.base, "ftpsd")
        os.makedirs(d)
        cert, key = os.path.join(d, "cert.pem"), os.path.join(d, "key.pem")
        # An RSA certificate, as nearly every FTPS server in the wild has: vsftpd's default TLS 1.2
        # cipher is ECDHE-RSA-…, and with an ECDSA certificate there is nothing the two ends share
        # once TLS 1.3 is off the table (which it is, for a server that demands session reuse).
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "2",
                        "-subj", "/CN=localhost", "-addext", "subjectAltName=IP:127.0.0.1", "-keyout", key, "-out", cert], check=True, capture_output=True)
        port = free_port()
        me = getpass.getuser()
        conf = os.path.join(d, "vsftpd.conf")
        with open(conf, "w") as f:
            f.write("\n".join([
                "listen=YES", "listen_ipv6=NO", "listen_address=127.0.0.1", f"listen_port={port}", "background=NO",
                # No root: no chroot, no switching user, no privileged port, no PAM.
                "run_as_launching_user=YES", f"ftp_username={me}", f"nopriv_user={me}", "seccomp_sandbox=NO",
                "local_enable=NO", "anonymous_enable=YES", f"anon_root={root}", "allow_writeable_chroot=YES",
                "write_enable=YES", "anon_upload_enable=YES", "anon_mkdir_write_enable=YES", "anon_other_write_enable=YES",
                "anon_world_readable_only=NO", "anon_umask=022", "hide_ids=NO",
                # TLS on the control and the data connection, required of everyone.
                "ssl_enable=YES", "allow_anon_ssl=YES", "force_anon_logins_ssl=YES", "force_anon_data_ssl=YES",
                # (Only options this vsftpd knows: it exits at an unknown one — `ssl_tlsv1_2` and
                # `utf8_filesystem` among them — with status 2 and not a word.)
                "ssl_sslv2=NO", "ssl_sslv3=NO", f"rsa_cert_file={cert}", f"rsa_private_key_file={key}",
                # vsftpd's own default, and how ProFTPD and FileZilla Server behave too: a data
                # connection must resume the control connection's TLS session. kiki used to manage
                # two files against such a server and then "522 session reuse required"; it is the
                # fixture's default so that it stays fixed. KIKI_E2E_VSFTPD_NO_REUSE=1 for the other
                # kind of server, which must keep working as well.
                "require_ssl_reuse=" + ("NO" if os.environ.get("KIKI_E2E_VSFTPD_NO_REUSE") else "YES"),
                "pasv_enable=YES", "pasv_address=127.0.0.1", "pasv_min_port=30000", "pasv_max_port=60000",
                "use_localtime=NO", "xferlog_enable=NO", "dual_log_enable=NO", f"vsftpd_log_file={d}/vsftpd.log",
                "max_clients=50", "max_per_ip=50", "idle_session_timeout=600", "data_connection_timeout=120", "",
            ]))
        log = open(os.path.join(d, "vsftpd.out"), "w")
        p = subprocess.Popen([vsftpd_bin(), conf], stdout=log, stderr=subprocess.STDOUT)
        self.procs.append(p)
        remember(p.pid)

        def up():
            if p.poll() is not None:
                raise RuntimeError(f"vsftpd did not start (status {p.returncode}; 2 with no message is an option it does not know): " + open(os.path.join(d, "vsftpd.out")).read())
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.5).close()
                return True
            except OSError:
                return None
        if not wait_for(up, timeout=10, what="vsftpd up"):
            raise RuntimeError("vsftpd did not listen: " + open(os.path.join(d, "vsftpd.out")).read())
        return port

    def start_smb(self, root, guest_root, tag="smbd", protocol="SMB2"):
        """Samba serving `root` (signed in to) and `guest_root` (open to all), as the launching
        user, on a high port, with a passdb, a lock directory and a log of its own.

        What a non-root `smbd` actually needs, learnt the hard way:

        - **every** directory it keeps state in named in the config — private, lock, state, cache,
          pid and ncalrpc — or it writes to `/var/lib/samba` and dies without a word;
        - the account made with `pdbedit -s <conf> -a -u <user> -t`. `smbpasswd -a` wants root and
          `smbpasswd -L` says so in as many words; `pdbedit` against a private passdb does not,
          and the unix account has to exist, which is why it is this one;
        - `-F -s <conf> --debug-stdout`: in the foreground, with its debug where it can be read,
          and **without** `--no-process-group`. That flag is a trap here: `smbd` ends by sending
          `SIGTERM` to its own process group to take its children with it, and the flag is what
          stops it from making a group of its own first — so `stop()` terminated the server, the
          server terminated the process group, and the whole test run died with status 143 at the
          first tear-down;
        - `force user`, because a server that cannot become anybody serves everything as itself;
        - a **short** path. Samba's messaging socket is `<lock directory>/msg.lock/<pid>` and a
          unix socket path is 107 bytes: under a deep temp directory `smbd` starts, prints two
          lines and exits with `messaging_dgm_ref failed: File name too long`.

        `protocol="NT1"` is the second server, which offers nothing but SMB1 (plan 25: kiki
        refuses it by name).
        """
        if not smbd_bin() or not pdbedit_bin():
            raise RuntimeError("no smbd/pdbedit")
        d = os.path.join(self.base, tag)
        for sub in ("private", "lock", "state", "cache", "run", "ncalrpc", "log"):
            os.makedirs(os.path.join(d, sub))
        os.chmod(os.path.join(d, "private"), 0o700)
        sock = os.path.join(d, "lock", "msg.lock", "0" * 7)
        if len(sock) > 100:
            raise RuntimeError(f"the fixture path is too long for Samba's messaging socket ({len(sock)} bytes): {sock}")
        port, me, conf = free_port(), smb_user(), os.path.join(d, "smb.conf")
        protocols = f"server min protocol = {protocol}\n" + (f"   server max protocol = {protocol}\n" if protocol != "SMB2" else "")
        with open(conf, "w") as f:
            f.write(
                f"""[global]
   workgroup = WORKGROUP
   server string = kiki e2e
   smb ports = {port}
   bind interfaces only = yes
   interfaces = 127.0.0.1
   {protocols}   security = user
   # DOS attributes out of the unix mode instead of an xattr, so a flow can make a file Hidden
   # with `chmod o+x` and no Samba tool at all (Samba's own mapping: owner x is Archive, group x
   # System, other x Hidden). That is the attribute kiki carries to `Meta.hidden` (plan 25).
   store dos attributes = no
   map hidden = yes
   map archive = no
   map system = no
   create mask = 0755
   map to guest = Bad User
   guest account = {me}
   passdb backend = tdbsam:{d}/private/passdb.tdb
   private dir = {d}/private
   lock directory = {d}/lock
   state directory = {d}/state
   cache directory = {d}/cache
   pid directory = {d}/run
   ncalrpc dir = {d}/ncalrpc
   log file = {d}/log/smbd.log
   # Enough to say why it stopped: at 1, an smbd that fails to start prints "started" and
   # nothing else before it ends its own process group (status -15). Only smbd.out gets it.
   log level = 3
   load printers = no
   printing = bsd
   printcap name = /dev/null
   disable spoolss = yes
   panic action = /bin/true

[{SMB_SHARE}]
   path = {root}
   read only = no
   guest ok = no
   force user = {me}

[{SMB_GUEST_SHARE}]
   path = {guest_root}
   read only = no
   guest ok = yes
   force user = {me}
"""
            )
        made = subprocess.run([pdbedit_bin(), "-s", conf, "-a", "-u", me, "-t"], input=f"{SMB_PASSWORD}\n{SMB_PASSWORD}\n", text=True, capture_output=True)
        if made.returncode != 0:
            raise RuntimeError(f"pdbedit could not make the account: {made.stdout}{made.stderr}")
        log = open(os.path.join(d, "smbd.out"), "w")
        # A pipe for its stdin, held open for as long as the server is wanted: in the foreground
        # smbd takes EOF on stdin as "the parent has gone" and exits (`Server exit (EOF on
        # stdin)`). On a developer's machine stdin is the terminal and never closes; under CI it
        # is closed or /dev/null, and the server was gone the moment it said "waiting for
        # connections". Closing the pipe is also the polite way to stop it (see `stop`).
        p = subprocess.Popen([smbd_bin(), "-F", "-s", conf, "--debug-stdout"], stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT)
        self.procs.append(p)
        remember(p.pid)

        def up():
            if p.poll() is not None:
                out = open(os.path.join(d, "smbd.out")).read()
                # The tail is where the reason is; the head is thirty lines of parameters.
                raise RuntimeError(f"smbd did not start (status {p.returncode}); its last lines:\n" + "\n".join(out.strip().splitlines()[-25:]))
            try:
                socket.create_connection(("127.0.0.1", port), timeout=0.5).close()
                return True
            except OSError:
                return None
        if not wait_for(up, timeout=15, what="smbd up"):
            raise RuntimeError("smbd did not listen: " + open(os.path.join(d, "smbd.out")).read())
        return port

    def stop(self):
        for p in self.procs:
            if p.stdin:
                try:
                    p.stdin.close()      # smbd: EOF on stdin is its own cue to leave
                except OSError:
                    pass
            p.terminate()
        for p in self.procs:
            # Waited for, and killed if it will not go: a server left behind holds its port and
            # its folder for ever, and the next run inherits the mess.
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                p.kill()
        if self.sshd_pid:
            try:
                os.kill(self.sshd_pid, 15)
            except ProcessLookupError:
                pass
        # And whatever `smbd` started beside itself. It runs `samba-dcerpcd`, which runs a
        # `rpcd_classic` and a `rpcd_winreg`, and those are not in its process group and do not go
        # when it does: three processes per server left behind holding this run's config file
        # open. They are known by that path, which no process outside this run has.
        kill_by_cmdline(self.base)

    # Every server here is meant to be gone by the end of the run; this is how a flow proves it.
    def strays(self):
        return by_cmdline(self.base)


def by_cmdline(needle):
    """Live processes whose command line mentions `needle` — this run's own, and nobody else's."""
    out = []
    for d in os.listdir("/proc"):
        if not d.isdigit() or int(d) == os.getpid():
            continue
        try:
            with open(f"/proc/{d}/cmdline", "rb") as f:
                cmd = f.read().replace(b"\0", b" ").decode("utf-8", "replace")
        except OSError:
            continue
        if needle in cmd:
            out.append((int(d), cmd.strip()))
    return out


def kill_by_cmdline(needle):
    for pid, _ in by_cmdline(needle):
        try:
            os.kill(pid, 15)
        except OSError:
            pass


def add_location(d, location, secrets):
    """Add it the way the dialog does: the first answer is the server's key, the second trusts it."""
    r = d.call("AddLocation", location=location, secrets=secrets)
    if "ok" in r and r["ok"].get("verify"):
        r = d.call("AddLocation", location=location, secrets=secrets, trust=r["ok"]["verify"])
    return r


