//! Read-only assurance declaration discovery and typed missing-ID refusals.
use crate::{PmRustError, Workspace};
use serde_json::{Value, json};

impl Workspace {
    /// Applies native output bounds to read-only declaration discovery or lookup.
    ///
    /// # Errors
    /// Returns declaration refusals or invalid output-control errors before rendering.
    pub fn assurance_read(
        &self,
        kind: &str,
        id: Option<&str>,
        options: &crate::ListOptions,
    ) -> Result<crate::ListOutput, PmRustError> {
        let value = match id {
            Some(id) => self.assurance_declaration(kind, id)?,
            None => self.assurance_declarations(kind)?,
        };
        crate::read_output::apply_command(value.into(), options, "assurance")
    }

    /// Reads configured declarations of a canonical assurance kind without writes.
    ///
    /// # Errors
    /// Returns a usage refusal for unknown kinds and a storage refusal for invalid registries.
    pub fn assurance_declarations(&self, kind: &str) -> Result<Value, PmRustError> {
        let key = match kind {
            "measurement" => "measurements",
            "assertion" => "assertions",
            "gate" => "gates",
            _ => {
                return Err(crate::orientation::invalid(format!(
                    "Unknown assurance declaration kind {kind}. Expected: measurement, assertion, gate"
                )));
            }
        };
        let path = self.pm_root().join("assurance.json");
        let document: Value = match std::fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str(&raw) {
                Ok(value) => value,
                Err(_) => return Err(invalid_registry()),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                json!({"measurements":[],"assertions":[],"gates":[]})
            }
            Err(source) => return Err(PmRustError::Io { path, source }),
        };
        let Some(items) = document[key].as_array() else {
            return Err(invalid_registry());
        };
        Ok(
            json!({"items":items,"count":items.len(),"row_contract":{"row_keys":["items"],"jq_selector":".items[]"}}),
        )
    }

    /// Reads one declaration or returns an expected, typed usage refusal with discovery guidance.
    ///
    /// # Errors
    /// Returns `CommandRefusal` with `assurance_evaluation_refused` for missing IDs.
    pub fn assurance_declaration(&self, kind: &str, id: &str) -> Result<Value, PmRustError> {
        let list = self.assurance_declarations(kind)?;
        if let Some(found) = list["items"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["id"] == id)
        {
            return Ok(found.clone());
        }
        let args = vec![
            "--pm-path".to_owned(),
            self.query_pm_root().to_string_lossy().into_owned(),
            "assurance".to_owned(),
            "list".to_owned(),
            kind.to_owned(),
            "--output-budget".to_owned(),
            "2000".to_owned(),
            "--json".to_owned(),
        ];
        let command = format!(
            "pm {}",
            args.iter()
                .map(|arg| crate::quote_command_arg(arg))
                .collect::<Vec<_>>()
                .join(" ")
        );
        Err(crate::collections::refusal(
            format!("assurance {kind} {id} not found"),
            2,
            json!({"code":"invalid_argument_value","reason":"assurance_evaluation_refused","changed":false,"value":id,"required":format!("Choose a configured {kind} declaration ID using the bounded discovery command."),"why":format!("The requested {kind} declaration does not exist. The registry and history were not changed."),"examples":[command],"recovery":{"next_best_command":command,"suggested_retry":command,"suggested_retry_args":args}}),
        ))
    }
}

/// Refuses corrupt registry reads without changing any declaration or history.
fn invalid_registry() -> PmRustError {
    crate::collections::refusal(
        "The assurance registry is invalid. Repair or restore .agents/pm/assurance.json and retry.",
        1,
        json!({"code":"assurance_registry_invalid"}),
    )
}

#[cfg(test)]
#[path = "../tests/support/assurance_unit.rs"]
mod tests;
