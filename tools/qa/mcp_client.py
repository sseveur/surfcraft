"""Minimal MCP stdio client for the SurfCraft QA drivers (stdlib only).

One JSON-RPC message per line on stdin/stdout, as surfcraft-mcp speaks it.
"""

import json
import os
import select
import subprocess
import time


class McpError(Exception):
    pass


class Mcp:
    def __init__(self, binary, env=None):
        full_env = dict(os.environ)
        full_env.update(env or {})
        self.proc = subprocess.Popen(
            [binary],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=full_env,
        )
        self.next_id = 1
        self.log = []

    def send_raw(self, text):
        self.proc.stdin.write(text.encode() + b"\n")
        self.proc.stdin.flush()

    def read_line(self, timeout):
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            return None
        line = self.proc.stdout.readline()
        if not line:
            raise McpError("server closed stdout (exit code %r)" % self.proc.poll())
        return json.loads(line)

    def request(self, method, params=None, timeout=180):
        rid = self.next_id
        self.next_id += 1
        msg = {"jsonrpc": "2.0", "id": rid, "method": method}
        if params is not None:
            msg["params"] = params
        self.send_raw(json.dumps(msg))
        reply = self.read_line(timeout)
        if reply is None:
            raise McpError("no reply to %s within %ss" % (method, timeout))
        if reply.get("id") != rid:
            raise McpError("reply id %r for request %r" % (reply.get("id"), rid))
        return reply

    def notify(self, method, params=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        self.send_raw(json.dumps(msg))

    def initialize(self):
        r = self.request(
            "initialize",
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "surfcraft-qa", "version": "1"},
            },
        )
        self.notify("notifications/initialized")
        return r

    def call(self, name, args=None, timeout=180):
        """Returns (ok, body, raw_result). body is parsed JSON text, raw text, or the image item."""
        t0 = time.time()
        r = self.request("tools/call", {"name": name, "arguments": args or {}}, timeout)
        dt = time.time() - t0
        self.log.append({"tool": name, "args": args, "seconds": round(dt, 3)})
        if "error" in r:
            return False, r["error"], r
        res = r["result"]
        content = res.get("content") or [{}]
        item = content[0]
        if item.get("type") == "image":
            body = item
        else:
            text = item.get("text", "")
            try:
                body = json.loads(text)
            except ValueError:
                body = text
        return (res.get("isError") is False), body, r

    def alive(self):
        try:
            return "result" in self.request("ping", {}, timeout=10)
        except McpError:
            return False

    def close(self):
        try:
            self.proc.stdin.close()
        except OSError:
            pass
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
