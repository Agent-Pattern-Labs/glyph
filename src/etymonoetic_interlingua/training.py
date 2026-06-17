from __future__ import annotations

from typing import Any, Iterable


def training_records(capsules: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for capsule in capsules:
        records.extend(records_for_capsule(capsule))
    return records


def records_for_capsule(capsule: dict[str, Any]) -> list[dict[str, Any]]:
    surface = capsule["surface"]
    capsule_id = capsule["id"]
    form = surface["form"]
    language = surface["language"]

    return [
        {
            "id": f"{capsule_id}::text_to_capsule",
            "task": "text_to_capsule",
            "input": {
                "form": form,
                "language": language,
                "instruction": "Represent this lexical item as an etymonoetic semantic capsule.",
            },
            "output": capsule,
        },
        {
            "id": f"{capsule_id}::capsule_to_expansion",
            "task": "capsule_to_expansion",
            "input": {
                "capsule": capsule,
                "instruction": "Expand this etymonoetic semantic capsule into an explainable paragraph.",
            },
            "output": {
                "paragraph": capsule["expansion"]["paragraph"],
                "trace": capsule["expansion"]["trace"],
            },
        },
    ]
