"""A throwaway FTPS server for the remote_transfers flow: explicit TLS, one user, one folder.

    python ftps_server.py <root> <port> <cert.pem> <key.pem> <user> <password>

Needs pyftpdlib and pyOpenSSL, which no distribution ships by default: `make e2e-servers` puts them
in tests/e2e/.venv. Prints READY once it is listening.
"""
import sys
from pyftpdlib.authorizers import DummyAuthorizer
from pyftpdlib.handlers import TLS_FTPHandler
from pyftpdlib.servers import ThreadedFTPServer

root, port, cert, key, user, password = sys.argv[1:7]
auth = DummyAuthorizer()
auth.add_user(user, password, root, perm="elradfmwMT")
handler = TLS_FTPHandler
handler.certfile, handler.keyfile = cert, key
handler.authorizer = auth
handler.tls_control_required = True
handler.tls_data_required = True
handler.passive_ports = range(int(port) + 1, int(port) + 60)
server = ThreadedFTPServer(("127.0.0.1", int(port)), handler)
print("READY", flush=True)
server.serve_forever()
