"""Does a normal exit withdraw the automation endpoint file?

Runs the app with `--automation --screenshot` (it renders a few frames, saves the PNG and closes itself through
the normal eframe shutdown), watches the endpoint file appear, and checks it is gone after the exit.

    python3 -I tools/qa/exit_cleanup.py --app target/release/surfcraft --out <scratch dir>
"""

import argparse
import os
import stat
import subprocess
import sys
import time


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--app", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    out = os.path.abspath(a.out)
    home = os.path.join(out, "home")
    os.makedirs(home, exist_ok=True)
    endpoint = os.path.join(out, "endpoint.json")
    if os.path.exists(endpoint):
        os.remove(endpoint)
    env = dict(os.environ, SURFCRAFT_AUTOMATION_FILE=endpoint, HOME=home)
    shot = os.path.join(out, "exit.png")
    p = subprocess.Popen([os.path.abspath(a.app), "--automation", "--screenshot", shot], env=env)
    seen = None
    t0 = time.time()
    while p.poll() is None and time.time() - t0 < 60:
        if seen is None and os.path.exists(endpoint):
            seen = time.time() - t0
            mode = stat.S_IMODE(os.stat(endpoint).st_mode)
            print(f"endpoint file appeared after {seen:.2f} s, mode {oct(mode)}")
        time.sleep(0.01)
    if p.poll() is None:
        p.kill()
        print("app did not exit within 60 s")
        return 1
    print(f"app exited with {p.returncode} after {time.time() - t0:.2f} s; screenshot {os.path.exists(shot)}")
    left = os.path.exists(endpoint)
    print("endpoint file after normal exit:", "STILL PRESENT" if left else "removed")
    return 1 if (seen is None or left) else 0


if __name__ == "__main__":
    sys.exit(main())
