"""Etymonoetic Interlingua MVP package."""

from etymonoetic_interlingua.templates import make_capsule_template
from etymonoetic_interlingua.training import records_for_capsule, training_records
from etymonoetic_interlingua.validator import (
    CapsuleValidationError,
    load_capsule,
    load_schema,
    validate_capsule,
    validate_file,
)

__all__ = [
    "CapsuleValidationError",
    "load_capsule",
    "load_schema",
    "make_capsule_template",
    "records_for_capsule",
    "training_records",
    "validate_capsule",
    "validate_file",
]
