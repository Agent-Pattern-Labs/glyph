pub mod cli;
pub mod sources;
pub mod templates;
pub mod training;
pub mod validator;

pub use sources::{LexicalSource, wiktionary_source};
pub use templates::{SCHEMA_VERSION, make_capsule_template, slugify};
pub use training::{records_for_capsule, training_records};
pub use validator::{load_capsule, load_schema, validate_capsule, validate_file};
