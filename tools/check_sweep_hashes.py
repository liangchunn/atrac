#!/usr/bin/env python3
import argparse
import hashlib
import subprocess
import tempfile
from pathlib import Path


WORKSPACE = Path(__file__).resolve().parent.parent
CASES = (
    ("at3", 132, "encoded_atrac3.sha1", "./stereo/132/1-01 Intro.at3"),
    ("at3p", 352, "encoded_atrac3plus.sha1", "./stereo/352/1-01 Intro.at3"),
)


def expected_hash(manifest: Path, relative: str) -> str:
    for line in manifest.read_text().splitlines():
        digest, path = line.split(maxsplit=1)
        if path == relative:
            return digest
    raise SystemExit(f"missing {relative} in {manifest}")


def sha1(path: Path) -> str:
    digest = hashlib.sha1()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("sweep_root", type=Path)
    parser.add_argument("--binary", type=Path, default=WORKSPACE / "target/release/atrac")
    args = parser.parse_args()
    sweep = args.sweep_root.resolve()
    binary = args.binary.resolve()
    input_path = sweep / "stereo" / "1-01 Intro.wav"
    with tempfile.TemporaryDirectory(prefix="atrac-sweep-") as directory:
        for codec, bitrate, manifest_name, relative in CASES:
            output = Path(directory) / f"{codec}.at3"
            subprocess.run(
                [str(binary), codec, "encode", "-b", str(bitrate), str(input_path), str(output)],
                check=True,
            )
            expected = expected_hash(sweep / manifest_name, relative)
            actual = sha1(output)
            if actual != expected:
                raise SystemExit(f"{codec} SHA-1 mismatch: expected {expected}, got {actual}")
            print(f"{codec}: {actual} PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
