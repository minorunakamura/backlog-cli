"""Local-only HTTPS fixture. No Backlog service is contacted."""
import http.server
import json
import os
import pathlib
import shutil
import socketserver
import ssl
import subprocess
import sys
import time
import threading
import urllib.parse

root = pathlib.Path(sys.argv[1])


class LoopbackServer(http.server.ThreadingHTTPServer):
    def server_bind(self):
        # The fixture has a fixed local name; HTTPServer's reverse DNS is unnecessary.
        socketserver.TCPServer.server_bind(self)
        self.server_name = "localhost"
        self.server_port = self.server_address[1]


class Handler(http.server.BaseHTTPRequestHandler):
    sequences = {}
    sequence_lock = threading.Lock()
    def log_message(self, *_):
        pass

    def do_POST(self):
        self.do_GET()

    def do_PATCH(self):
        self.do_GET()

    def do_GET(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0))).decode("utf-8")
        with (root / "requests").open("a") as output:
            output.write(json.dumps({"at": time.time(), "method": self.command, "path": self.path, "headers": dict(self.headers), "body": body, "form": urllib.parse.parse_qs(body, keep_blank_values=True)}) + "\n")
        (root / f"{self.command.lower()}-received").touch()
        response = json.loads((root / "response.json").read_text())
        if "routes" in response:
            path = self.path.split("?", 1)[0]
            routes = response["routes"]
            response = routes.get(f"{self.command} {path}", routes.get(path, {"status": 404}))
        if "sequence" in response:
            key = (self.path.split("?", 1)[0], json.dumps(response))
            with self.sequence_lock:
                index = self.sequences.get(key, 0)
                self.sequences[key] = index + 1
            response = response["sequence"][min(index, len(response["sequence"]) - 1)]
        if "offsets" in response:
            query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
            response = response["offsets"].get(query.get("offset", ["0"])[0], {"body": "[]"})
        if "commentItems" in response:
            query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
            items = response["commentItems"]
            if "minId" in query:
                items = [item for item in items if item["id"] > int(query["minId"][0])]
            if "maxId" in query:
                items = [item for item in items if item["id"] < int(query["maxId"][0])]
            items = sorted(items, key=lambda item: item["id"], reverse=query.get("order") == ["desc"])
            response["body"] = json.dumps(items[:int(query.get("count", [20])[0])])
        if response.get("disconnect"):
            self.connection.close()
            return
        time.sleep(response.get("header_delay", 0))
        self.send_response(response.get("status", 200))
        if "location" in response:
            self.send_header("Location", response["location"])
        body = response.get("body", "[]").encode()
        self.send_header("Content-Length", str(len(body) + (10 if response.get("truncate") else 0)))
        self.end_headers()
        self.wfile.flush()
        (root / "headers-sent").touch()
        (root / f"{self.command.lower()}-headers-sent").touch()
        time.sleep(response.get("delay", 0))
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ssl.SSLError):
            pass


os.umask(0o077)
tls = root / "tls"
tls.mkdir(mode=0o700)
try:
    config = tls / "openssl.cnf"
    config.write_text("""\
[req]
prompt = no
distinguished_name = subject
[subject]
CN = localhost
[server]
subjectAltName = DNS:localhost
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment
extendedKeyUsage = serverAuth
""")
    certificate = tls / "localhost-cert.pem"
    key = tls / "localhost-key.pem"
    result = subprocess.run(
        ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
         "-sha256", "-days", "1", "-config", str(config), "-extensions", "server",
         "-keyout", str(key), "-out", str(certificate)],
        capture_output=True, text=True,
    )
    if result.returncode:
        raise RuntimeError(f"OpenSSL certificate generation failed: {result.stderr}")
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(certificate, key)
    with LoopbackServer(("127.0.0.1", 0), Handler) as server:
        # Handshake in handler I/O so a silent peer cannot block the accept loop.
        server.socket = context.wrap_socket(server.socket, server_side=True, do_handshake_on_connect=False)
        print(server.server_port, flush=True)
        server.serve_forever()
finally:
    # Rust owns root and removes it even when it kills this server without Python cleanup.
    shutil.rmtree(tls)
