"""Run an initialization probe with a real deadline; timeout is a failure."""
import subprocess, sys
try:
    result = subprocess.run(sys.argv[2:], timeout=float(sys.argv[1]))
    sys.exit(result.returncode)
except subprocess.TimeoutExpired:
    print("FAIL: initialization probe exceeded its deadline", file=sys.stderr)
    sys.exit(124)
