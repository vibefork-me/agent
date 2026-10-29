import json, os, subprocess, sys, time
exe = os.path.join(os.getcwd(), "cli", "node_modules", "@github", "copilot-win32-x64", "copilot.exe")
env = {k: v for k, v in os.environ.items() if k not in ("GH_TOKEN", "GITHUB_TOKEN", "COPILOT_GITHUB_TOKEN")}
env["COPILOT_HOME"] = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.getcwd(), "home")
p = subprocess.Popen([exe, "--server", "--stdio", "--no-auto-update"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env)
def send(i, m):
    b = json.dumps({"jsonrpc": "2.0", "id": i, "method": m, "params": {}}).encode()
    p.stdin.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); p.stdin.flush()
def recv():
    n = 0
    while True:
        line = p.stdout.readline()
        if not line: return None
        if line.lower().startswith(b"content-length:"): n = int(line.split(b":")[1])
        if line in (b"\r\n", b"\n"): break
    return json.loads(p.stdout.read(n))
t = time.time(); send(1, "auth.getStatus")
while True:
    r = recv()
    if r is None or r.get("id") == 1: break
print(round(time.time() - t, 1), "s", json.dumps(r)[:400]); p.kill()
