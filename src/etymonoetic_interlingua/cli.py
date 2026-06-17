from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from etymonoetic_interlingua.templates import make_capsule_template
from etymonoetic_interlingua.training import training_records
from etymonoetic_interlingua.validator import CapsuleValidationError, validate_capsule, validate_file


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="ei",
        description="Validate and inspect etymonoetic semantic capsules.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate_parser = subparsers.add_parser("validate", help="Validate one or more capsule files.")
    validate_parser.add_argument("paths", nargs="+", type=Path)
    validate_parser.set_defaults(func=run_validate)

    new_parser = subparsers.add_parser("new", help="Create a valid starter capsule.")
    new_parser.add_argument("form")
    new_parser.add_argument("--language", default="en")
    new_parser.add_argument("--part-of-speech", default="unknown")
    new_parser.add_argument("--output", "-o", type=Path)
    new_parser.set_defaults(func=run_new)

    show_parser = subparsers.add_parser("show", help="Print a compact capsule summary.")
    show_parser.add_argument("path", type=Path)
    show_parser.set_defaults(func=run_show)

    expand_parser = subparsers.add_parser("expand", help="Print the capsule expansion paragraph.")
    expand_parser.add_argument("path", type=Path)
    expand_parser.add_argument("--trace", action="store_true", help="Include trace steps.")
    expand_parser.set_defaults(func=run_expand)

    export_parser = subparsers.add_parser(
        "export-training",
        help="Export validated capsules as JSONL text-to-capsule and capsule-to-expansion records.",
    )
    export_parser.add_argument("paths", nargs="+", type=Path)
    export_parser.add_argument("--output", "-o", type=Path)
    export_parser.set_defaults(func=run_export_training)

    schema_parser = subparsers.add_parser("schema", help="Print the bundled capsule JSON Schema.")
    schema_parser.set_defaults(func=run_schema)

    return parser


def run_validate(args: argparse.Namespace) -> int:
    ok = True
    for path in args.paths:
        try:
            validate_file(path)
            print(f"OK {path}")
        except (CapsuleValidationError, OSError, json.JSONDecodeError) as exc:
            ok = False
            print(f"FAIL {path}", file=sys.stderr)
            print(exc, file=sys.stderr)
    return 0 if ok else 1


def run_new(args: argparse.Namespace) -> int:
    try:
        capsule = validate_capsule(
            make_capsule_template(
                args.form,
                language=args.language,
                part_of_speech=args.part_of_speech,
            )
        )
    except (CapsuleValidationError, ValueError) as exc:
        print(exc, file=sys.stderr)
        return 1

    content = json.dumps(capsule, indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(content, encoding="utf-8")
        print(f"WROTE {args.output}")
    else:
        print(content, end="")
    return 0


def run_show(args: argparse.Namespace) -> int:
    try:
        capsule = validate_file(args.path)
    except (CapsuleValidationError, OSError, json.JSONDecodeError) as exc:
        print(exc, file=sys.stderr)
        return 1

    surface = capsule["surface"]
    print(f"{surface['form']} ({surface['language']})")
    print(capsule["capsule_summary"])
    print()
    print("Present senses:")
    for sense in capsule["present_usage"]["senses"]:
        print(f"- {sense['id']}: {sense['definition']}")
    return 0


def run_expand(args: argparse.Namespace) -> int:
    try:
        capsule = validate_file(args.path)
    except (CapsuleValidationError, OSError, json.JSONDecodeError) as exc:
        print(exc, file=sys.stderr)
        return 1

    print(capsule["expansion"]["paragraph"])
    if args.trace:
        print()
        print("Trace:")
        for step in capsule["expansion"]["trace"]:
            print(f"- {step['layer']}: {step['contribution']}")
    return 0


def run_export_training(args: argparse.Namespace) -> int:
    try:
        capsules = [validate_file(path) for path in args.paths]
    except (CapsuleValidationError, OSError, json.JSONDecodeError) as exc:
        print(exc, file=sys.stderr)
        return 1

    lines = [
        json.dumps(record, ensure_ascii=False, sort_keys=True)
        for record in training_records(capsules)
    ]
    content = "\n".join(lines) + "\n"

    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(content, encoding="utf-8")
        print(f"WROTE {args.output}")
    else:
        print(content, end="")
    return 0


def run_schema(_args: argparse.Namespace) -> int:
    from etymonoetic_interlingua.validator import load_schema

    print(json.dumps(load_schema(), indent=2, sort_keys=True))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return args.func(args)
