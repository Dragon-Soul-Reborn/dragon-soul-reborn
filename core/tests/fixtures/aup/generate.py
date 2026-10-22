"""Writes the .aup golden vectors and the e2e payload .aup files from synthetic inputs.

Run once with the private repo's codec: python3 generate.py /path/to/tools/aup
Every input is generated here, so no game data is involved.
"""
import os
import sys

sys.path.insert(0, sys.argv[1])
import aup  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
E2E = os.path.join(HERE, "..", "e2e", "payload", "data")


def lcg_bytes(n: int, seed: int) -> bytes:
    out = bytearray()
    x = seed
    for _ in range(n):
        x = (x * 1103515245 + 12345) & 0x7FFFFFFF
        out.append((x >> 16) & 0xFF)
    return bytes(out)


INPUTS = {
    "empty": b"",
    "one": b"A",
    "two": b"AB",
    "three": b"ABC",
    "text": "".join(f"The quick brown fox jumps over the lazy dog {i}. " for i in range(1200)).encode(),
    "random": lcg_bytes(40000, 7),
    "runs": b"\x00" * 5000 + b"\xff" * 5000 + b"abcabcabc" * 2000,
    "xml": "".join(f'<Item id="{i}" name="item{i % 37}" level="{i % 90}" />\n' for i in range(700)).encode(),
}

for name, data in INPUTS.items():
    with open(os.path.join(HERE, name + ".bin"), "wb") as f:
        f.write(data)
    with open(os.path.join(HERE, name + ".aup"), "wb") as f:
        f.write(aup.encode(data))

os.makedirs(E2E, exist_ok=True)
for name, data in {"hello.txt": b"hello world v1\n", "same.txt": b"unchanged\n"}.items():
    with open(os.path.join(E2E, name + ".aup"), "wb") as f:
        f.write(aup.encode(data))
print("ok")
