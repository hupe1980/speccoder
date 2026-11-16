"""The showcase's model: a local mock of the OpenAI Responses API (streamed)
that answers by what is asked, from plan.json. No real model, no network.

    python3 -I mock.py <plan.json> <dir>   # writes <dir>/port, logs <dir>/r-NNN.json
"""
import http.server, json, os, re, socketserver, sys, threading

PLAN = json.load(open(sys.argv[1]))
DIR = sys.argv[2]
N = {"i": 0}
LOCK = threading.Lock()


def task_of(body):
    m = re.search(r"# Task ([\w-]+)", json.dumps(body))
    return m.group(1) if m else None


def answer(body):
    tools = [t for t in body.get("tools") or [] if t.get("name") != "respond"]
    p = PLAN.get(task_of(body) or "", {})
    schema = json.dumps(body.get("text", {}).get("format", {}))
    if not tools:
        if '"argv"' in schema and '"files"' in schema:
            return {"content": json.dumps(p.get("acceptance"))}
        return {"content": json.dumps(p.get("done") or {})}
    # An attempt: the step is the number of tool results already given.
    done = sum(1 for i in body.get("input", []) if isinstance(i, dict) and i.get("type") == "function_call_output")
    steps = p.get("steps", [])
    if done < len(steps):
        return steps[done]
    return {"content": json.dumps(p.get("done"))}


class Server(http.server.ThreadingHTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "127.0.0.1", self.server_address[1]


class H(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        with LOCK:
            N["i"] += 1
            i = N["i"]
        json.dump(body, open(os.path.join(DIR, "r-%03d.json" % i), "w"), indent=1)
        step = answer(body)
        out = []
        if step.get("reasoning") and body.get("reasoning", {}).get("summary"):
            out.append({"type": "reasoning", "id": "rs_%d" % i, "encrypted_content": "opaque-%d" % i,
                        "summary": [{"type": "summary_text", "text": t} for t in step["reasoning"].split("\n\n")]})
        if step.get("say"):
            out.append({"type": "message", "id": "msg_c%d" % i, "role": "assistant",
                        "content": [{"type": "output_text", "text": step["say"]}]})
        if "tool" in step:
            out.append({"type": "function_call", "id": "fc_%d" % i, "call_id": "call_%d" % i,
                        "name": step["tool"], "arguments": json.dumps(step["args"])})
        else:
            out.append({"type": "message", "id": "msg_%d" % i, "role": "assistant",
                        "content": [{"type": "output_text", "text": step["content"]}]})
        resp = {"id": "resp_%d" % i, "status": "completed", "output": out,
                "usage": {"input_tokens": 8200 + 37 * i, "output_tokens": 410 + 3 * i, "total_tokens": 0,
                          "input_tokens_details": {"cached_tokens": 6000}}}
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        ev = [{"type": "response.created", "response": {"id": resp["id"], "status": "in_progress"}}]
        for item in out:
            if item["type"] == "reasoning":
                for part in item["summary"]:
                    ev.append({"type": "response.reasoning_summary_text.delta", "delta": part["text"]})
        ev.append({"type": "response.completed", "response": resp})
        for n, e in enumerate(ev):
            e["sequence_number"] = n
            self.wfile.write(("event: %s\ndata: %s\n\n" % (e["type"], json.dumps(e))).encode())

    def log_message(self, *a):
        pass


s = Server(("127.0.0.1", 0), H)
open(os.path.join(DIR, "port"), "w").write(str(s.server_address[1]))
s.serve_forever()
