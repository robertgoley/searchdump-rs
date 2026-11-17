#!/usr/bin/env python3
"""Tiny logging HTTP reverse proxy for request traces (method, path, status, ms). Plain HTTP only.
Usage: trace_proxy.py LISTEN_PORT UPSTREAM_URL LOGFILE. Bodies and headers are not logged."""
import http.server, socketserver, sys, time, urllib.request, threading
port, up, logf = int(sys.argv[1]), sys.argv[2].rstrip("/"), sys.argv[3]
lock = threading.Lock()
class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *a): pass
    def _do(self):
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else None
        hdr = {k: v for k, v in self.headers.items() if k.lower() in ("content-type", "accept", "accept-encoding", "content-encoding")}
        t = time.monotonic()
        r = urllib.request.Request(up + self.path, data=body, method=self.command, headers=hdr)
        try:
            resp = urllib.request.urlopen(r, timeout=300); code = resp.status; data = resp.read(); rh = resp.headers
        except urllib.error.HTTPError as e:
            code, data, rh = e.code, e.read(), e.headers
        ms = (time.monotonic() - t) * 1000
        with lock, open(logf, "a") as f:
            f.write(f"{time.time():.3f}\t{self.command}\t{self.path.split('?')[0]}\t{'?' in self.path and self.path.split('?',1)[1] or ''}\t{code}\t{ms:.1f}\t{n}\t{len(data)}\n")
        self.send_response(code)
        for k, v in rh.items():
            if k.lower() in ("content-type", "content-encoding"): self.send_header(k, v)
        self.send_header("Content-Length", str(len(data))); self.end_headers()
        if self.command != "HEAD": self.wfile.write(data)
    do_GET = do_POST = do_PUT = do_DELETE = do_HEAD = _do
class S(socketserver.ThreadingMixIn, http.server.HTTPServer): daemon_threads = True
S(("127.0.0.1", port), H).serve_forever()
