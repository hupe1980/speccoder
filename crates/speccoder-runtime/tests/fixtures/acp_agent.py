"""A scripted ACP agent over stdio for speccoder's tests (no dependencies).

Run as `python3 -I acp_agent.py SCENARIO.json`. The scenario says how it
answers `initialize` and `session/new`, and what it does in its one turn:

    {"init": {...merged into the initialize result...},
     "new_error": {"code": -32000, "message": "Authentication required"},
     "session": {...merged into the session/new result...},
     "steps": [
        {"say": "words"},
        {"permission": {"kind": "execute", "rawInput": {"command": "ls"}},
         "options": [...]},                        # default: allow/always/reject
        {"read": "src/lib.rs", "line": 1, "limit": 2},
        {"write": "src/lib.rs", "content": "..."},
        {"edit": "src/lib.rs", "content": "..."},    # the agent's own write
        {"terminal": "cargo", "args": ["test"]},   # create, wait, output, release
        {"plan": [{"content": "...", "status": "pending"}]},
        {"tool": {"kind": "read", "title": "...", "status": "completed"}},
        {"sleep": 5},
        {"exit": 3, "stderr": "out of credits"}],
     "stop": "end_turn",
     "log": "PATH"}

Every answer it got from the client is appended to `log` as a JSON line:
`{"step": i, "method": ..., "result": ...} | {"error": ...}`; the client's
requests to it too (`{"got": method, "params": ...}`). A step after a
refused or cancelled permission still runs: the test says what follows.
"""
import json
import os
import sys
import time

SCENARIO = json.load(open(sys.argv[1]))
LOG = SCENARIO.get("log")
CWD = os.getcwd()
NEXT = [100]
CANCELLED = [False]


def log(entry):
    if LOG:
        with open(LOG, "a") as f:
            f.write(json.dumps(entry) + "\n")


def send(msg):
    msg["jsonrpc"] = "2.0"
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def reply(msg_id, result=None, error=None):
    out = {"id": msg_id}
    if error is not None:
        out["error"] = error
    else:
        out["result"] = result
    send(out)


def notify(session, update):
    send({"method": "session/update", "params": {"sessionId": session, "update": update}})


def read():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)


def handle_side(msg):
    """A client message that is not the answer awaited."""
    method = msg.get("method")
    if method == "session/cancel":
        CANCELLED[0] = True
        log({"got": method})
    elif method == "session/set_config_option":
        params = msg.get("params", {})
        log({"got": method, "params": params})
        options = SCENARIO.get("session", {}).get("configOptions", [])
        for o in options:
            if o.get("id") == params.get("configId"):
                o["currentValue"] = params.get("value")
        reply(msg["id"], {"configOptions": options})
    elif method is not None and "id" in msg:
        log({"got": method, "params": msg.get("params")})
        reply(msg["id"], {})


def call(method, params):
    """Sends a request to the client and waits for its answer."""
    NEXT[0] += 1
    my_id = NEXT[0]
    send({"id": my_id, "method": method, "params": params})
    while True:
        msg = read()
        if msg.get("id") == my_id and "method" not in msg:
            return msg
        handle_side(msg)


def absolute(p):
    return p if os.path.isabs(p) else os.path.join(CWD, p)


def turn(session, prompt_id):
    for i, step in enumerate(SCENARIO.get("steps", [])):
        if CANCELLED[0]:
            break
        if "say" in step:
            notify(session, {"sessionUpdate": "agent_message_chunk",
                             "content": {"type": "text", "text": step["say"]}})
        elif "permission" in step:
            tool = dict(step["permission"])
            tool.setdefault("toolCallId", "call-%d" % i)
            tool.setdefault("title", "step %d" % i)
            if "locations" in tool:
                tool["locations"] = [{"path": absolute(l)} for l in tool["locations"]]
            options = step.get("options", [
                {"optionId": "allow", "kind": "allow_once", "name": "Allow once"},
                {"optionId": "always", "kind": "allow_always", "name": "Always allow"},
                {"optionId": "reject", "kind": "reject_once", "name": "Reject"},
            ])
            answer = call("session/request_permission",
                          {"sessionId": session, "toolCall": tool, "options": options})
            log({"step": i, "method": "session/request_permission",
                 "result": answer.get("result"), "error": answer.get("error")})
        elif "read" in step:
            params = {"sessionId": session, "path": absolute(step["read"])}
            for k in ("line", "limit"):
                if k in step:
                    params[k] = step[k]
            answer = call("fs/read_text_file", params)
            log({"step": i, "method": "fs/read_text_file",
                 "result": answer.get("result"), "error": answer.get("error")})
        elif "write" in step:
            answer = call("fs/write_text_file", {"sessionId": session,
                                                 "path": absolute(step["write"]),
                                                 "content": step["content"]})
            log({"step": i, "method": "fs/write_text_file",
                 "result": answer.get("result"), "error": answer.get("error")})
        elif "edit" in step:
            with open(absolute(step["edit"]), "w") as f:
                f.write(step["content"])
        elif "terminal" in step:
            created = call("terminal/create", {"sessionId": session,
                                               "command": step["terminal"],
                                               "args": step.get("args", []),
                                               "cwd": CWD})
            log({"step": i, "method": "terminal/create",
                 "result": created.get("result"), "error": created.get("error")})
            if "result" in created:
                tid = created["result"]["terminalId"]
                waited = call("terminal/wait_for_exit", {"sessionId": session, "terminalId": tid})
                out = call("terminal/output", {"sessionId": session, "terminalId": tid})
                call("terminal/release", {"sessionId": session, "terminalId": tid})
                log({"step": i, "method": "terminal/output",
                     "result": out.get("result"), "exit": waited.get("result")})
        elif "plan" in step:
            notify(session, {"sessionUpdate": "plan", "entries": [
                dict({"priority": "medium"}, **e) for e in step["plan"]]})
        elif "tool" in step:
            t = dict(step["tool"])
            t.setdefault("toolCallId", "tool-%d" % i)
            notify(session, dict({"sessionUpdate": "tool_call"}, **t))
        elif "sleep" in step:
            end = time.time() + step["sleep"]
            import select
            while time.time() < end and not CANCELLED[0]:
                ready, _, _ = select.select([sys.stdin], [], [], 0.05)
                if ready:
                    handle_side(read())
        elif "exit" in step:
            if step.get("stderr"):
                sys.stderr.write(step["stderr"] + "\n")
                sys.stderr.flush()
            sys.exit(step["exit"])
    stop = "cancelled" if CANCELLED[0] else SCENARIO.get("stop", "end_turn")
    reply(prompt_id, {"stopReason": stop})


def main():
    session = "scripted-1"
    while True:
        msg = read()
        method = msg.get("method")
        if method == "initialize":
            log({"got": method, "params": msg.get("params")})
            result = {"protocolVersion": 1,
                      "agentCapabilities": {"loadSession": False},
                      "agentInfo": {"name": "scripted", "version": "1.0.0"},
                      "authMethods": []}
            result.update(SCENARIO.get("init", {}))
            reply(msg["id"], result)
        elif method == "session/new":
            log({"got": method, "params": msg.get("params")})
            if "new_error" in SCENARIO:
                reply(msg["id"], error=SCENARIO["new_error"])
                continue
            result = {"sessionId": session}
            result.update(SCENARIO.get("session", {}))
            reply(msg["id"], result)
        elif method == "session/prompt":
            log({"got": method, "params": msg.get("params")})
            turn(session, msg["id"])
        elif method is not None:
            handle_side(msg)


main()
