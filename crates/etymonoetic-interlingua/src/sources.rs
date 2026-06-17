use anyhow::{Result, bail};
use chrono::Local;
use serde_json::{Value, json};

use crate::templates::slugify;

pub const WIKTIONARY_LICENSE: &str = "CC BY-SA 4.0 and GFDL; see Wiktionary terms for details";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalSource {
    pub id: String,
    pub source_type: String,
    pub citation: String,
    pub url: Option<String>,
    pub license: Option<String>,
    pub accessed: Option<String>,
    pub note: Option<String>,
}

impl LexicalSource {
    pub fn to_provenance(&self) -> Value {
        let mut entry = json!({
            "id": self.id,
            "source_type": self.source_type,
            "citation": self.citation,
        });

        if let Some(url) = &self.url {
            entry["url"] = json!(url);
        }
        if let Some(license) = &self.license {
            entry["license"] = json!(license);
        }
        if let Some(accessed) = &self.accessed {
            entry["accessed"] = json!(accessed);
        }
        if let Some(note) = &self.note {
            entry["note"] = json!(note);
        }

        entry
    }
}

pub fn wiktionary_source(form: &str, language: &str) -> Result<LexicalSource> {
    if language != "en" {
        bail!("Only English Wiktionary source URLs are supported in the MVP");
    }

    let normalized = slugify(form);
    Ok(LexicalSource {
        id: format!("wiktionary-en-{normalized}"),
        source_type: "dictionary".to_owned(),
        citation: format!(
            "Wiktionary contributors, \"{normalized}\", Wiktionary, The Free Dictionary."
        ),
        url: Some(format!("https://en.wiktionary.org/wiki/{normalized}")),
        license: Some(WIKTIONARY_LICENSE.to_owned()),
        accessed: Some(Local::now().date_naive().to_string()),
        note: Some(
            "Use as a cited lexical source; manually normalize claims into EI layers.".to_owned(),
        ),
    })
}
