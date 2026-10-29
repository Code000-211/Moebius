#!/usr/bin/env python3
# A plugin process in tests. It behaves as its first argument says, and its second argument, if given, is the name its
# manifest gives (the rest it writes to standard error after "started"): answers (the first option), errors, crashes (exits when called), garbage (writes a line that is not a
# reply), hangs (never answers), silent (sends no manifest) or exits (exits at once).
import json
import sys

mode = sys.argv[1]
name = sys.argv[2] if len(sys.argv) > 2 else "test"


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


if mode == "exits":
    print("cannot start", file=sys.stderr, flush=True)
    sys.exit(3)
if mode == "silent":
    sys.stdin.read()
    sys.exit(0)
send({"type": "manifest", "manifest": {"name": name, "version": "1.2.3", "kinds": ["decider"]}})
print("started", *sys.argv[3:], file=sys.stderr, flush=True)
for line in sys.stdin:
    message = json.loads(line)
    id = message["id"]
    if mode == "crashes":
        sys.exit(1)
    elif mode == "garbage":
        sys.stdout.write("not json\n")
        sys.stdout.flush()
    elif mode == "hangs":
        continue
    elif mode == "errors":
        send({"type": "error", "id": id, "message": "no"})
    else:
        params = message["call"]["params"]
        send({"type": "reply", "id": id, "result": {"choice": params["options"][0], "reason": "first"}})
