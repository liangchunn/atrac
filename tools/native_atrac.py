#!/usr/bin/env python3
import argparse
import os
import shlex
import subprocess
import tempfile
from pathlib import Path


WORKSPACE = Path(__file__).resolve().parent.parent
DEFAULT_IMAGE = "atrac-native-ref"


def run(command: list[str]) -> None:
    subprocess.run(command, check=True)


def ensure_image(image: str, rebuild: bool) -> None:
    present = subprocess.run(
        ["docker", "image", "inspect", image],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ).returncode == 0
    if rebuild or not present:
        run([
            "docker",
            "build",
            "--platform",
            "linux/386",
            "-t",
            image,
            "-f",
            str(WORKSPACE / "docker" / "Dockerfile"),
            str(WORKSPACE),
        ])


def docker_run(operation: str, input_path: Path, output_path: Path, bitrate: int | None, rebuild: bool) -> None:
    image = os.environ.get("ATRAC_NATIVE_IMAGE", DEFAULT_IMAGE)
    ensure_image(image, rebuild)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    command = [
        "docker",
        "run",
        "--rm",
        "--platform",
        "linux/386",
        "-v",
        f"{input_path.parent}:/input:ro",
        "-v",
        f"{output_path.parent}:/output",
        image,
        "-d" if operation == "decode" else "-e",
    ]
    if operation == "encode":
        command.extend(["-br", str(bitrate)])
    command.extend([f"/input/{input_path.name}", f"/output/{output_path.name}"])
    run(command)


def remote_run(operation: str, input_path: Path, output_path: Path, bitrate: int | None) -> None:
    host = os.environ.get("ATRAC_NATIVE_REMOTE_HOST")
    remote_dir = os.environ.get("ATRAC_NATIVE_REMOTE_DIR")
    if not host or not remote_dir:
        raise SystemExit("remote backend requires ATRAC_NATIVE_REMOTE_HOST and ATRAC_NATIVE_REMOTE_DIR")
    remote_tmp = subprocess.run(
        ["ssh", host, "mktemp -d /tmp/atrac-native.XXXXXX"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    remote_input = f"{remote_tmp}/input.wav"
    remote_output = f"{remote_tmp}/output.wav"
    try:
        run(["scp", str(input_path), f"{host}:{remote_input}"])
        args = [shlex.quote(f"{remote_dir}/at3tool"), "-d" if operation == "decode" else "-e"]
        if operation == "encode":
            args.extend(["-br", str(bitrate)])
        args.extend([shlex.quote(remote_input), shlex.quote(remote_output)])
        run(["ssh", host, " ".join(args)])
        output_path.parent.mkdir(parents=True, exist_ok=True)
        run(["scp", f"{host}:{remote_output}", str(output_path)])
    finally:
        subprocess.run(["ssh", host, f"rm -rf {shlex.quote(remote_tmp)}"], check=False)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("operation", choices=("encode", "decode"))
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("-b", "--bitrate", type=int)
    parser.add_argument("--backend", choices=("docker", "remote"), default="docker")
    parser.add_argument("--rebuild-image", action="store_true")
    args = parser.parse_args()
    if args.operation == "encode" and args.bitrate is None:
        parser.error("encode requires --bitrate")
    input_path = args.input.resolve()
    output_path = args.output.resolve()
    if args.backend == "docker":
        docker_run(args.operation, input_path, output_path, args.bitrate, args.rebuild_image)
    else:
        remote_run(args.operation, input_path, output_path, args.bitrate)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
