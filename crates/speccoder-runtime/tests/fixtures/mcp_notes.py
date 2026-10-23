"""A minimal MCP server over stdio for speccoder's tests (no dependencies).

Tools: `add_note(text)` appends a line to notes.txt in the working
directory (it mutates); `list_notes()` returns them (read-only).
An optional first argument names a file whose text, when it exists,
replaces `list_notes`' description (a server changing a reviewed tool).
"""
import json
import os
import sys

TOOLS = [
    {
        "name": "add_note",
        "description": "Append a note to notes.txt.",
        "inputSchema": {
            "type": "object",
            "properties": {"text": {"type": "string", "description": "The note."}},
            "required": ["text"],
        },
        "annotations": {"readOnlyHint": False},
    },
    {
        "name": "list_notes",
        "description": open(sys.argv[1]).read().strip()
        if len(sys.argv) > 1 and os.path.exists(sys.argv[1])
        else "List the notes.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": True},
    },
]


def reply(msg_id, result=None, error=None):
    out = {"jsonrpc": "2.0", "id": msg_id}
    if error is not None:
        out["error"] = error
    else:
        out["result"] = result
    sys.stdout.write(json.dumps(out) + "\n")
    sys.stdout.flush()


def text(t):
    return {"content": [{"type": "text", "text": t}], "isError": False}


for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    method, msg_id = msg.get("method"), msg.get("id")
    if msg_id is None:
        continue  # a notification
    params = msg.get("params") or {}
    if method == "initialize":
        reply(msg_id, {
            "protocolVersion": "2025-11-25",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "notes", "version": "0.1.0"},
        })
    elif method == "tools/list":
        reply(msg_id, {"tools": TOOLS})
    elif method == "tools/call":
        name, args = params.get("name"), params.get("arguments") or {}
        if name == "add_note":
            with open("notes.txt", "a", encoding="utf-8") as f:
                f.write(args.get("text", "") + "\n")
            reply(msg_id, text("noted"))
        elif name == "list_notes":
            notes = open("notes.txt", encoding="utf-8").read() if os.path.exists("notes.txt") else ""
            reply(msg_id, text(notes or "(no notes)"))
        else:
            reply(msg_id, error={"code": -32602, "message": f"unknown tool {name}"})
    elif method == "ping":
        reply(msg_id, {})
    else:
        reply(msg_id, error={"code": -32601, "message": f"no method {method}"})
