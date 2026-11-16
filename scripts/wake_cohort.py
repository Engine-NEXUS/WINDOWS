"""Collect and validate the multi-user wake cohort without invoking the app or trainer."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import wave

import voice_eval_set as voice


SPLITS = ("train", "validation", "test")
TARGET_SPEAKERS = dict(zip(SPLITS, (15, 5, 10)))
TARGET_DEVICES = dict(zip(SPLITS, (7, 3, 5)))
CONDITIONS = (
    ("near", 30, "normal", "quiet"),
    ("working", 70, "normal", "quiet"),
    ("far", 150, "normal", "quiet"),
    ("fan", 70, "normal", "fan"),
    ("soft", 70, "quiet", "quiet"),
)
DEFAULT_ROOT = voice.DEFAULT_DIR / "cohort"


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,39}", value):
        raise ValueError("Use lowercase letters, digits and hyphens for identifiers (1–40 characters).")
    return value


def load_roster(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    roster = {}
    devices = {}
    for person in data["participants"]:
        speaker = identifier(person["speaker_id"])
        device = identifier(person["device_id"])
        split = person["split"]
        model = person["laptop_model"].strip().casefold()
        if speaker in roster or split not in SPLITS or not model:
            raise ValueError(f"Invalid or repeated speaker assignment: {speaker}")
        if devices.setdefault(device, (split, model)) != (split, model):
            raise ValueError(f"Laptop {device} has conflicting model or split assignments")
        for field in ("device_class", "accent"):
            if not isinstance(person[field], str) or not person[field].strip():
                raise ValueError(f"{speaker}: missing {field}")
        roster[speaker] = {**person, "laptop_model": model}
    if not roster:
        raise ValueError("The roster is empty")
    return roster


def slots(person):
    """Ten takes per phrase and condition, in three sittings of 60/45/45 clips."""
    speaker = person["speaker_id"]
    for sitting, takes in enumerate((range(1, 5), range(5, 8), range(8, 11)), 1):
        for condition, distance, volume, environment in CONDITIONS:
            for phrase in voice.WAKE_PHRASES:
                for take in takes:
                    yield make_row(person, f"{speaker}-s{sitting}-{condition}-{voice.slug(phrase)}-{take:02d}",
                                   f"s{sitting}", "wake", environment, phrase=phrase,
                                   condition=condition, distance_cm=distance, volume=volume, take=take)


def make_row(person, sample_id, session, kind, environment, **extra):
    return {
        **{field: person[field] for field in ("speaker_id", "device_id", "device_class",
                                             "laptop_model", "accent", "split")},
        "id": sample_id, "session_id": session, "kind": kind, "environment": environment,
        "source_group_id": f"{person['speaker_id']}:{session}",
        "label": "positive" if kind == "wake" else "background", **extra,
    }


def pcm_info(path, kind):
    digest = hashlib.sha256()
    has_signal = False
    with wave.open(str(path), "rb") as clip:
        rate, channels, frames = clip.getframerate(), clip.getnchannels(), clip.getnframes()
        if clip.getsampwidth() != 2 or rate not in (16000, 44100, 48000) or channels not in (1, 2, 4):
            raise ValueError("Expected PCM16 WAV at 16, 44.1 or 48 kHz with 1, 2 or 4 channels")
        duration = frames / rate
        if not (1 <= duration <= (10 if kind == "wake" else 1800)):
            raise ValueError("Wake clips must be 1–10 seconds and room clips 1–1800 seconds")
        # Ignore WAV metadata when checking for duplicate audio. Include format in the digest.
        digest.update(f"{rate}/{channels}/".encode())
        remaining = frames
        while remaining:
            data = clip.readframes(min(remaining, 65536))
            if not data or len(data) % (channels * 2):
                raise ValueError("Truncated PCM data")
            digest.update(data)
            has_signal = has_signal or any(data)
            remaining -= len(data) // (channels * 2)
    if not has_signal:
        raise ValueError("Recording contains only digital silence")
    return {"pcm_sha256": digest.hexdigest(), "sample_rate": rate, "channels": channels,
            "frames": frames}


def expected_room(person, row):
    session = identifier(row["session_id"])
    tag = identifier(row["room_id"])
    if row["environment"] not in voice.ENVIRONMENTS | {"music", "playback"}:
        raise ValueError("Unknown room environment")
    return make_row(person, f"{person['speaker_id']}-room-{session}-{tag}", session,
                    "room", row["environment"], room_id=tag)


def save_sample(root, row, write_audio):
    """Publish the WAV and metadata together so an interrupted capture can be retried."""
    samples = root / "samples"
    samples.mkdir(parents=True, exist_ok=True)
    destination = samples / row["id"]
    if destination.exists():
        raise ValueError(f"Sample already exists: {row['id']}")
    # A failed/interrupted staging directory is outside samples and is never exported.
    staging = Path(tempfile.mkdtemp(prefix=".pending-", dir=root))
    path = staging / "audio.wav"
    meta = staging / "row.json"
    try:
        write_audio(path)
        info = pcm_info(path, row["kind"])
        meta.write_text(json.dumps({**row, **info, "consent": True}) + "\n", encoding="utf-8")
        staging.rename(destination)
    finally:
        if staging.exists():
            path.unlink(missing_ok=True)
            meta.unlink(missing_ok=True)
            staging.rmdir()


def audit(root, roster):
    expected = {row["id"]: row for person in roster.values() for row in slots(person)}
    rows = []
    errors = []
    hashes = {}
    sample_root = root / "samples"
    for folder in sorted(sample_root.iterdir()) if sample_root.exists() else ():
        try:
            if not folder.is_dir() or folder.is_symlink() or folder.is_junction():
                raise ValueError("Expected a real sample directory")
            path = folder / "audio.wav"
            meta = folder / "row.json"
            if path.is_symlink() or meta.is_symlink():
                raise ValueError("Linked sample files are not accepted")
            row = json.loads(meta.read_text(encoding="utf-8"))
            planned = expected_room(roster[row["speaker_id"]], row) if row["kind"] == "room" else expected[folder.name]
            if folder.name != planned["id"] or any(row.get(key) != value for key, value in planned.items()):
                raise ValueError("Sample metadata disagrees with roster or recording plan")
            if row.get("consent") is not True:
                raise ValueError("Missing recording consent")
            info = pcm_info(path, row["kind"])
            if any(row.get(key) != value for key, value in info.items()):
                raise ValueError("Audio bytes or format changed since collection")
            other = hashes.setdefault(info["pcm_sha256"], folder.name)
            if other != folder.name:
                raise ValueError(f"Duplicate PCM audio also stored as {other}")
            rows.append({**planned, **info, "consent": True,
                         "audio": path.relative_to(root).as_posix()})
        except (OSError, ValueError, KeyError, TypeError, AttributeError, EOFError, wave.Error) as exc:
            errors.append(f"{folder.name}: {exc}")
    speakers = Counter(p["split"] for p in roster.values())
    devices = {s: len({p["device_id"] for p in roster.values() if p["split"] == s}) for s in SPLITS}
    models = {s: {p["laptop_model"] for p in roster.values() if p["split"] == s} for s in SPLITS}
    positives = Counter(row["split"] for row in rows if row["kind"] == "wake")
    room = defaultdict(float)
    for row in rows:
        if row["split"] == "test" and row["kind"] == "room":
            room[row["device_id"]] += row["frames"] / row["sample_rate"] / 3600
    test_devices = {p["device_id"] for p in roster.values() if p["split"] == "test"}
    gaps = []
    for split in SPLITS:
        if speakers[split] < TARGET_SPEAKERS[split] or devices[split] < TARGET_DEVICES[split]:
            gaps.append(f"{split}: need {TARGET_SPEAKERS[split]} people and {TARGET_DEVICES[split]} laptops")
    if len(set.union(*models.values())) < 8 or len(models["test"] - models["train"]) < 3:
        gaps.append("Need eight laptop models overall and three acceptance models absent from training")
    if sum(positives.values()) != len(expected):
        gaps.append(f"Collected {sum(positives.values())}/{len(expected)} positive slots")
    if sum(room.values()) < 48 or any(room[device] < 8 for device in test_devices):
        gaps.append("Need 48 pooled acceptance room hours and eight hours on each acceptance laptop")
    return rows, {"speakers": dict(speakers), "devices": devices, "positive_clips": dict(positives),
                  "test_room_hours_by_device": dict(room), "errors": errors, "collection_gaps": gaps,
                  "collection_complete": not errors and not gaps}


def record(root, person, sitting):
    for row in slots(person):
        if row["session_id"] != f"s{sitting}" or (root / "samples" / row["id"]).exists():
            continue
        while True:
            input(voice._prompt_text(row) + "\nEnter to record, then speak. ")
            pcm = voice._record(2.5)
            answer = input("Keep, retry or skip [k/r/s]? ").strip().lower()
            if answer == "s":
                break
            if answer == "k":
                try:
                    save_sample(root, row, lambda path: voice._write_wav(path, pcm))
                    break
                except ValueError as exc:
                    print(f"Capture rejected: {exc}")


def write_manifest(path, rows):
    with path.open("x", encoding="utf-8") as out:
        for row in rows:
            out.write(json.dumps(row, sort_keys=True) + "\n")


def replay_metrics(expected, results, threshold, dsp):
    by_id = {row["id"]: row for row in expected}
    seen = set()
    counts = defaultdict(Counter)
    for result in results:
        sample_id = result["id"]
        row = by_id.get(sample_id)
        if row is None or sample_id in seen or "error" in result:
            raise ValueError(f"Unexpected, duplicate or failed replay sample: {sample_id}")
        if any(result.get(key) != value for key, value in row.items()):
            raise ValueError(f"Replay metadata mismatch: {sample_id}")
        if result.get("cohort_replay") is not True:
            raise ValueError("Evaluator lacks explicit cohort mode. Use an evaluator built from the updated source.")
        if result.get("dsp") != dsp or not math.isclose(result["threshold"], threshold, abs_tol=1e-6):
            raise ValueError(f"Replay operating point mismatch: {sample_id}")
        seen.add(sample_id)
        groups = ["overall", *(f"{key}={row[key]}" for key in
                              ("speaker_id", "device_id", "laptop_model", "environment"))]
        if row["kind"] == "wake":
            if type(result.get("fired")) is not bool:
                raise ValueError(f"Missing wake decision: {sample_id}")
            groups.extend(f"{key}={row[key]}" for key in ("phrase", "condition", "accent"))
            for group in groups:
                counts[group].update(attempts=1, detections=int(result["fired"]))
        else:
            fires = result.get("n_fires")
            if type(fires) is not int or fires < 0:
                raise ValueError(f"Invalid room detection count: {sample_id}")
            for group in groups:
                counts[group].update(room_detections=fires, room_seconds=row["frames"] / row["sample_rate"])
    if seen != set(by_id):
        raise ValueError(f"Replay omitted {len(by_id) - len(seen)} samples")
    return {
        group: {**count, "recall": count["detections"] / count["attempts"] if count["attempts"] else None,
                "raw_detections_per_hour": count["room_detections"] * 3600 / count["room_seconds"]
                if count["room_seconds"] else None}
        for group, count in sorted(counts.items())
    }


def file_sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def replay(args, selected, report):
    if not 0 < args.threshold < 1:
        raise ValueError("Threshold must be between zero and one")
    if args.evaluator.stem.lower() != "wake_eval" or not args.evaluator.is_file():
        raise ValueError("Supply a prebuilt wake_eval executable")
    oww = args.resources / "oww"
    required = [oww / name for name in ("nexus.onnx", "melspectrogram.onnx", "embedding_model.onnx",
                                        "acoustic_profile.json", "dsp_spec.json")]
    files = [args.evaluator, *required, *sorted(oww.glob("*.onnx.data"))]
    before = {str(path.resolve()): file_sha256(path) for path in files}
    args.out.mkdir()
    manifest = args.out / "manifest.jsonl"
    results = args.out / "results.jsonl"
    write_manifest(manifest, selected)
    manifest_hash = file_sha256(manifest)
    subprocess.run([str(args.evaluator.resolve()), "--cohort-replay", "--resources", str(args.resources.resolve()),
                    "--manifest", str(manifest.resolve()), "--root", str(args.root.resolve()),
                    "--out", str(results.resolve()), "--dsp", args.dsp,
                    "--threshold", str(args.threshold)], check=True)
    if before != {str(path.resolve()): file_sha256(path) for path in files} or file_sha256(manifest) != manifest_hash:
        raise ValueError("Evaluator, model assets or manifest changed during replay")
    for row in selected:
        info = pcm_info(args.root / row["audio"], row["kind"])
        if any(info[key] != row[key] for key in info):
            raise ValueError(f"Audio changed during replay: {row['id']}")
    metrics = replay_metrics(selected, voice.read_manifest(results), args.threshold, args.dsp)
    evidence = {"measurement": "neural_replay", "split": args.split,
                "threshold": args.threshold, "dsp": args.dsp, "input_file_sha256": before,
                "manifest_sha256": manifest_hash, "results_sha256": file_sha256(results),
                "collection_complete": report["collection_complete"], "groups": metrics}
    (args.out / "report.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--roster", required=True, type=Path)
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    sub = parser.add_subparsers(dest="command", required=True)
    for command in ("plan", "record", "import-wake", "room"):
        child = sub.add_parser(command)
        child.add_argument("--speaker", required=True)
        if command in ("plan", "record"):
            child.add_argument("--sitting", choices=(1, 2, 3), type=int, required=True)
        if command != "plan":
            child.add_argument("--consent", action="store_true", required=True,
                               help="Confirm consent to collect or import this session's audio")
        if command in ("import-wake", "room"):
            child.add_argument("--wav", type=Path, required=True)
        if command == "import-wake":
            child.add_argument("--id", required=True)
        if command == "room":
            child.add_argument("--session", required=True)
            child.add_argument("--room-id", required=True)
            child.add_argument("--environment", required=True)
    sub.add_parser("audit").add_argument("--require-complete", action="store_true")
    export = sub.add_parser("export")
    export.add_argument("--split", choices=("validation", "test"), required=True)
    export.add_argument("--out", type=Path, required=True)
    replay_parser = sub.add_parser("replay")
    replay_parser.add_argument("--split", choices=("validation", "test"), required=True)
    replay_parser.add_argument("--out", type=Path, required=True)
    replay_parser.add_argument("--evaluator", type=Path, required=True)
    replay_parser.add_argument("--resources", type=Path, required=True)
    replay_parser.add_argument("--dsp", choices=("legacy", "aligned"), required=True)
    replay_parser.add_argument("--threshold", type=float, required=True)
    args = parser.parse_args()
    try:
        roster = load_roster(args.roster)
        if args.command in ("plan", "record", "import-wake", "room"):
            person = roster[args.speaker]
            if args.command == "plan":
                for row in slots(person):
                    if row["session_id"] == f"s{args.sitting}":
                        print(f"{row['id']}  {voice._prompt_text(row)}")
            elif args.command == "record":
                record(args.root, person, args.sitting)
            elif args.command == "import-wake":
                row = next((row for row in slots(person) if row["id"] == args.id), None)
                if row is None:
                    raise ValueError("Unknown wake slot for this speaker")
                save_sample(args.root, row, lambda path: shutil.copyfile(args.wav, path))
            else:
                row = expected_room(person, {"session_id": args.session, "room_id": args.room_id,
                                             "environment": args.environment})
                save_sample(args.root, row, lambda path: shutil.copyfile(args.wav, path))
        else:
            rows, report = audit(args.root, roster)
            print(json.dumps(report, indent=2))
            if report["errors"]:
                return 1
            if args.command == "audit" and args.require_complete and not report["collection_complete"]:
                return 1
            if args.command in ("export", "replay"):
                if args.split == "test" and not report["collection_complete"]:
                    raise ValueError("Complete the acceptance collection before exporting or replaying it")
                selected = [row for row in rows if row["split"] == args.split]
                if not selected:
                    raise ValueError("The selected split has no recordings")
                if args.command == "export":
                    write_manifest(args.out, selected)
                else:
                    replay(args, selected, report)
    except (OSError, ValueError, TypeError, AttributeError, KeyError, EOFError, wave.Error,
            subprocess.CalledProcessError) as exc:
        parser.exit(1, f"Wake cohort: {exc}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
