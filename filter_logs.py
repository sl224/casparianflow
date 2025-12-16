
import sys

try:
    with open("test_output_full.log", "r", encoding="utf-8") as f:
        # We also try reading as cp1252 if utf-8 fails, but open(..., errors='replace') suffices
        content = f.read()
except Exception:
    with open("test_output_full.log", "r", encoding="cp1252", errors="replace") as f:
        content = f.read()

print("--- SystemDeployer Log Entries ---")
found = False
for line in content.splitlines():
    if "SystemDeployer" in line or "Error" in line:
        print(line)
        found = True

if not found:
    print("No SystemDeployer logs found.")
