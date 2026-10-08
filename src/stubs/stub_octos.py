#!/usr/bin/env python3
"""Stub of `octos serve --stdio` for the dual-loop E2E test.

Speaks the JSON-RPC protocol OctoBuddy's `inner::Serve` sends. For every
`session/open` it remembers the peer id and its working directory (the
protocol passes `cwd` only on `session/open`); for the `turn/start` it
writes the slice's source file into that cwd, then streams the same
envelopes a real turn would: `turn/started`, a `progress/updated`
`thinking`, a `tool_start` + `tool_end` for the file write, an
`assistant_delta` "Done.", an `assistant_persisted` carrying an
`octobuddy-report` block, and `turn_terminal`.
"""
import json
import os
import sys
import uuid


def read_messages(stream):
    """Yield one JSON object per line from `stream`."""
    for raw in stream:
        line = raw.strip()
        if not line:
            continue
        try:
            yield json.loads(line)
        except json.JSONDecodeError:
            continue


def send(stream, obj):
    stream.write(json.dumps(obj) + "\n")
    stream.flush()


def envelope(seq, payload_type, **data):
    """One `projection/envelope` notification (the turn's body)."""
    return {
        "method": "projection/envelope",
        "params": {
            "session_id": data.pop("session_id"),
            "turn_id": data.pop("turn_id"),
            "seq": seq,
            "payload": {"type": payload_type, "data": data},
        },
    }


def main():
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")

    # session_id → its cwd (octos 3 passes `cwd` only on `session/open`,
    # so the cwd persists across the turns of that session).
    cwds: dict[str, str] = {}

    for msg in read_messages(sys.stdin):
        method = msg.get("method")
        params = msg.get("params") or {}

        if method is None and "id" in msg:
            # A response to one of our requests: nothing to do.
            continue

        if method == "session/open":
            session_id = params.get("session_id") or ("local:octobuddy:" + uuid.uuid4().hex[:8])
            cwd = params.get("cwd") or os.getcwd()
            cwds[session_id] = cwd
            send(sys.stdout, {"jsonrpc": "2.0", "id": msg["id"],
                              "result": {"opened": {"reasoning_effort": "medium"}}})

        elif method == "turn/start":
            turn_id = params.get("turn_id")
            session_id = params.get("session_id")
            cwd = cwds.get(session_id) or os.getcwd()
            # turn/started (the server's first notification).
            send(sys.stdout, {"method": "turn/started",
                              "params": {"session_id": session_id, "turn_id": turn_id}})
            # progress/updated · thinking — one round.
            send(sys.stdout, {"method": "progress/updated",
                              "params": {"session_id": session_id, "turn_id": turn_id,
                                         "metadata": {"kind": "thinking", "iteration": 1}}})
            # The inner loop's file tool writes the slice's source into its cwd.
            # Real octos would stream `progress/updated { kind: "file_mutation" }`;
            # the projection/envelope channel here is what the inner loop's
            # `handle_frame` translates into `PeerFileChanged` for the host.
            tool_call_id = "tool-" + uuid.uuid4().hex[:8]
            send(sys.stdout, envelope(1, "tool_start", session_id=session_id,
                                      turn_id=turn_id, tool_call_id=tool_call_id,
                                      name="write_file",
                                      arguments_preview="calc.py"))
            with open(os.path.join(cwd, "calc.py"), "w", encoding="utf-8") as f:
                f.write("def add(a, b):\n    return a + b\n")
            send(sys.stdout, {"method": "progress/updated",
                              "params": {"session_id": session_id, "turn_id": turn_id,
                                         "metadata": {"kind": "file_mutation",
                                                      "file_mutation": {"path": os.path.join(cwd, "calc.py"),
                                                                         "operation": "modify",
                                                                         "preview_id": "p1"}}}})
            send(sys.stdout, envelope(2, "tool_end", session_id=session_id,
                                      turn_id=turn_id, tool_call_id=tool_call_id,
                                      status="complete", duration_ms=12))
            # The peer's text (a delta and a persisted block), with the report.
            send(sys.stdout, envelope(3, "assistant_delta", session_id=session_id,
                                      turn_id=turn_id, text="Done."))
            report = (
                "Done.\n\n```octobuddy-report\n"
                "status: done\nverified: tests pass (stub)\n"
                "decide: none\nnotes: stub inner-loop wrote the slice's file\n```"
            )
            send(sys.stdout, envelope(4, "assistant_persisted", session_id=session_id,
                                      turn_id=turn_id, text=report))
            # turn_terminal — completes the turn.
            send(sys.stdout, {"method": "projection/envelope",
                              "params": {"session_id": session_id, "turn_id": turn_id,
                                         "seq": 5,
                                         "payload": {"type": "turn_terminal",
                                                     "data": {"outcome": "completed"}}}})

        elif method == "turn/interrupt":
            pass  # the test never interrupts.

        elif method in ("turn/steer", "approval/respond", "user_question/respond",
                        "permission/profile/set"):
            req_id = msg.get("id")
            if req_id is not None:
                send(sys.stdout, {"jsonrpc": "2.0", "id": req_id, "result": {"ok": True}})


if __name__ == "__main__":
    main()