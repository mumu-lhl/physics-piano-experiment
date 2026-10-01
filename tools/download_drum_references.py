#!/usr/bin/env python3
"""Download and prepare the acoustic one-shots used by physics-drum evals."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request
import wave
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PACKAGES = {
    "core": {
        "filename": "ferrosintesis-samples-drumkit-0.2.0.crate",
        "url": "https://static.crates.io/crates/ferrosintesis-samples-drumkit/ferrosintesis-samples-drumkit-0.2.0.crate",
        "sha256": "1b57b772660d0eabdb2d766c91de6018e494fcdc8ca532b6aae4edd3b77f0452",
        "member_root": "ferrosintesis-samples-drumkit-0.2.0/samples",
    },
    "cymbals": {
        "filename": "ferrosintesis-samples-drumkit2-0.2.0.crate",
        "url": "https://static.crates.io/crates/ferrosintesis-samples-drumkit2/ferrosintesis-samples-drumkit2-0.2.0.crate",
        "sha256": "a93a2c70d04d923381f97c190ae4a397cf8ca30eb2efe7ba4c5f21d38c03a3a3",
        "member_root": "ferrosintesis-samples-drumkit2-0.2.0/samples",
    },
}

STEMGMD = {
    "filename": "StemGMD_single_hits.zip",
    "url": "https://zenodo.org/records/7882857/files/StemGMD_single_hits.zip?download=1",
    "md5": "ec91b926e23dabbc011fd7eaba482214",
    "member_root": "StemGMD_single_hits",
    "velocity_layer": 5,
}

STEMGMD_KITS = (
    "bluebird",
    "brooklyn",
    "detroit_garage",
    "east_bay",
    "heavy",
    "motown_revisited",
    "portland",
    "retro_rock",
    "roots",
    "socal",
)

STEMGMD_INSTRUMENTS = {
    "kick": "kick",
    "snare": "snare",
    "hi_tom": "hi_tom",
    "mid_tom": "mid_tom",
    "low_tom": "low_tom",
    "hihat_open": "hihat_open",
    "hihat_closed": "hihat_closed",
    "crash": "crash_left",
    "ride": "ride",
}

REFERENCES = {
    "kick": ("core", "kick_vl3_rr1.flac"),
    "snare": ("core", "snare_vl4_rr1.flac"),
    "tom_low": ("core", "tomlo_vl3_rr1.flac"),
    "tom_high": ("core", "tomhi_vl3_rr1.flac"),
    "hihat_closed": ("core", "hhc_vl3_rr1.flac"),
    "hihat_open": ("core", "hho_vl3_rr1.flac"),
    "crash": ("cymbals", "crash_vl2_rr1.flac"),
    "ride": ("core", "ride_vl2_rr1.flac"),
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def md5_file(path: Path) -> str:
    digest = hashlib.md5(usedforsecurity=False)
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def get_archive(package: dict[str, str], archive_dir: Path) -> Path:
    archive_dir.mkdir(parents=True, exist_ok=True)
    archive = archive_dir / package["filename"]
    hash_name = "sha256" if "sha256" in package else "md5"
    hash_file = sha256_file if hash_name == "sha256" else md5_file
    if archive.exists() and hash_file(archive) == package[hash_name]:
        print(f"Using cached {archive.name}")
        return archive

    request = urllib.request.Request(
        package["url"], headers={"User-Agent": "physics-drum-timbre-eval/1.0"}
    )
    temporary: Path | None = None
    try:
        with urllib.request.urlopen(request, timeout=90) as response:
            with tempfile.NamedTemporaryFile(
                prefix=f"{archive.name}.", suffix=".tmp", dir=archive_dir, delete=False
            ) as output:
                temporary = Path(output.name)
                shutil.copyfileobj(response, output)
        actual_hash = hash_file(temporary)
        if actual_hash != package[hash_name]:
            raise RuntimeError(
                f"{hash_name.upper()} mismatch for {archive.name}: "
                f"expected {package[hash_name]}, got {actual_hash}"
            )
        temporary.replace(archive)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    print(f"Downloaded {archive.name}")
    return archive


def read_sample(archive: Path, member_name: str) -> bytes:
    with tarfile.open(archive, mode="r:gz") as package:
        member = package.getmember(member_name)
        if not member.isfile() or member.size > 32 * 1024 * 1024:
            raise RuntimeError(f"Unexpected sample archive member: {member_name}")
        contents = package.extractfile(member)
        if contents is None:
            raise RuntimeError(f"Unable to read archive member: {member_name}")
    return contents.read()


def read_stemgmd_sample(archive: Path, kit: str, instrument: str) -> bytes:
    sample_name = f"{instrument}-{STEMGMD['velocity_layer']}.wav"
    member_name = f"{STEMGMD['member_root']}/{kit}/{sample_name}"
    with zipfile.ZipFile(archive) as package:
        try:
            member = package.getinfo(member_name)
        except KeyError as error:
            raise RuntimeError(f"Missing StemGMD reference: {member_name}") from error
        if not member.filename.endswith(".wav") or member.file_size > 12 * 1024 * 1024:
            raise RuntimeError(f"Unexpected StemGMD archive member: {member_name}")
        contents = package.read(member)

    with wave.open(io.BytesIO(contents), "rb") as decoded:
        if (
            decoded.getnchannels() != 2
            or decoded.getsampwidth() != 2
            or decoded.getframerate() != 44100
            or decoded.getnframes() == 0
        ):
            raise RuntimeError(f"Unexpected StemGMD WAV format: {member_name}")
    return contents


def prepare_stemgmd(archive: Path, output_dir: Path) -> dict[str, object]:
    references: dict[str, dict[str, object]] = {}
    for kit in STEMGMD_KITS:
        kit_dir = output_dir / kit
        kit_dir.mkdir(parents=True, exist_ok=True)
        for name, source_name in STEMGMD_INSTRUMENTS.items():
            sample = read_stemgmd_sample(archive, kit, source_name)
            output_path = kit_dir / f"{name}.wav"
            output_path.write_bytes(sample)
            references[f"{kit}/{name}"] = {
                "file": f"{kit}/{name}.wav",
                "source": f"{source_name}-{STEMGMD['velocity_layer']}.wav",
                "sha256": sha256_file(output_path),
            }
        print(f"Prepared StemGMD references for {kit}")

    manifest: dict[str, object] = {
        "source": "StemGMD single hits",
        "source_url": "https://zenodo.org/records/7882857",
        "archive": {
            "file": STEMGMD["filename"],
            "md5": STEMGMD["md5"],
        },
        "velocity_layer": STEMGMD["velocity_layer"],
        "kits": list(STEMGMD_KITS),
        "references": references,
    }
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    return manifest


def convert_flac(name: str, flac_data: bytes, output_dir: Path, ffmpeg: str) -> dict[str, object]:
    output_path = output_dir / f"{name}.wav"
    result = subprocess.run(
        [
            ffmpeg,
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-i",
            "pipe:0",
            "-map_metadata",
            "-1",
            "-ac",
            "1",
            "-ar",
            "44100",
            "-c:a",
            "pcm_s16le",
            "-f",
            "wav",
            str(output_path),
        ],
        input=flac_data,
        stdout=subprocess.DEVNULL,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"ffmpeg failed to decode {name}; install ffmpeg and retry")

    with wave.open(str(output_path), "rb") as decoded:
        if (
            decoded.getnchannels() != 1
            or decoded.getsampwidth() != 2
            or decoded.getframerate() != 44100
            or decoded.getnframes() == 0
        ):
            raise RuntimeError(f"Unexpected decoded WAV format: {output_path}")
        frame_count = decoded.getnframes()
    return {
        "file": output_path.name,
        "source": REFERENCES[name][1],
        "sha256": sha256_file(output_path),
        "frames": frame_count,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "target" / "drum-reference" / "virtuosity",
        help="where to place decoded reference WAVs (default: ignored target/ cache)",
    )
    parser.add_argument(
        "--archive-dir",
        type=Path,
        help="reuse downloaded .crate and .zip archives from this directory instead of the output cache",
    )
    parser.add_argument(
        "--stemgmd-output-dir",
        type=Path,
        default=ROOT / "target" / "drum-reference" / "stemgmd",
        help="where to place selected StemGMD hits (default: ignored target/ cache)",
    )
    parser.add_argument(
        "--skip-virtuosity",
        action="store_true",
        help="prepare only the multi-kit StemGMD references",
    )
    parser.add_argument("--ffmpeg", default="ffmpeg", help="ffmpeg executable")
    args = parser.parse_args()

    output_dir = args.output_dir.resolve()
    archive_dir = (args.archive_dir or output_dir / "archives").resolve()
    stemgmd_output_dir = args.stemgmd_output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    if not args.skip_virtuosity:
        archives = {
            key: get_archive(package, archive_dir) for key, package in PACKAGES.items()
        }
        manifest: dict[str, object] = {
            "source": "Virtuosity Drums",
            "source_revision": "9f04cf9a734527edfbb0a4eee1f674e45bbf71bc",
            "archives": {
                key: {"file": PACKAGES[key]["filename"], "sha256": PACKAGES[key]["sha256"]}
                for key in PACKAGES
            },
            "references": {},
        }
        references: dict[str, object] = {}
        for name, (package_key, sample_name) in REFERENCES.items():
            member = f"{PACKAGES[package_key]['member_root']}/{sample_name}"
            flac_data = read_sample(archives[package_key], member)
            details = convert_flac(name, flac_data, output_dir, args.ffmpeg)
            references[name] = details
            print(f"Prepared {details['file']} ({details['frames']} frames)")

        manifest["references"] = references
        (output_dir / "manifest.json").write_text(
            json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
        )
        print(f"Reference audio ready in {output_dir}")

    stemgmd_archive = get_archive(STEMGMD, archive_dir)
    prepare_stemgmd(stemgmd_archive, stemgmd_output_dir)
    print(f"Multi-kit references ready in {stemgmd_output_dir}")


if __name__ == "__main__":
    main()
