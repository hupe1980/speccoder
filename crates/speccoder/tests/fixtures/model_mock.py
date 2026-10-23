"""A scripted OpenAI-compatible chat-completions server for crash tests.

Usage: model_mock.py <dir> <script.json>

Answers request n with step n of the script, streamed like a real server:
  {"tool": "<name>", "args": {...}}  a tool call
  {"content": "<text>"}               a message (a structured answer is JSON text)
A step with "hang_once": true hangs the first time it is asked (the test
kills the client there) and answers when asked again. Writes <dir>/port
when listening, <dir>/hung while hanging, and <dir>/requests (one line per
request received).
"""
import http.server
import json
import os
import sys
import threading
import time

DIR, SCRIPT = sys.argv[1], sys.argv[2]
STEPS = json.load(open(SCRIPT))
STATE = {"next": 0, "hung": False}
LOCK = threading.Lock()


def chunk(delta, finish=None):
    return {"id": "mock", "object": "chat.completion.chunk", "model": "mock",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}


class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", 0)))
        with LOCK:
            with open(os.path.join(DIR, "requests"), "a") as f:
                f.write(self.path + "\n")
            step = STEPS[min(STATE["next"], len(STEPS) - 1)]
            hang = step.get("hang_once") and not STATE["hung"]
            if hang:
                STATE["hung"] = True
            else:
                STATE["next"] += 1
        if hang:
            open(os.path.join(DIR, "hung"), "w").close()
            time.sleep(300)
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        if "tool" in step:
            events = [
                chunk({"role": "assistant", "tool_calls": [{"index": 0, "id": "call_%d" % STATE["next"], "type": "function",
                       "function": {"name": step["tool"], "arguments": json.dumps(step["args"])}}]}),
                chunk({}, "tool_calls"),
            ]
        else:
            events = [chunk({"role": "assistant", "content": step["content"]}), chunk({}, "stop")]
        events.append({"id": "mock", "object": "chat.completion.chunk", "model": "mock", "choices": [],
                       "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
        for e in events:
            self.wfile.write(b"data: " + json.dumps(e).encode() + b"\n\n")
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()

    def log_message(self, *args):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(os.path.join(DIR, "port.tmp"), "w") as f:
    f.write(str(server.server_address[1]))
os.rename(os.path.join(DIR, "port.tmp"), os.path.join(DIR, "port"))
server.serve_forever()
