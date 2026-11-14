//! The run's fill function: the caller's role map becomes the model set of
//! each of the run's prompts when the run is built.

use std::collections::BTreeMap;

use crate::model::{ModelBinding, ModelInfo, ModelInvocation, ModelRole, ModelSet, ThinkingMode};
use crate::prompt::{Prompt, RoleRequirements};

use super::requirements::{RequirementCheck, Requirements, UnmetRequirement};

/// Builds the model set of `prompt` from the caller's role map and checks
/// what the prompt declares against the models that fill the roles. `by`
/// is the name of `prompt` when it is a linked prompt, and `None` for the
/// run's own.
///
/// A role's model is its own entry in `models`, else the `default` entry.
/// The set holds one binding for each of the five roles, in
/// [`ModelRole::ALL`] order, for every prompt whether or not it declares the
/// role. Each declared role's context minimum and thinking requirement are
/// checked against its model's info, reporting required versus actual into
/// `requirements` under `by`, and a declared thinking requirement sets the
/// thinking switch on that role's binding.
///
/// An empty map means the caller has no model, so the set stays empty and
/// nothing is checked. A map with no `default` entry leaves the set empty
/// too: [`Requirements::check_run`] reports that gap, once for the run.
pub(super) fn fill_model_set(
    prompt: &Prompt,
    by: Option<&str>,
    models: &BTreeMap<ModelRole, ModelInfo>,
    requirements: &mut Requirements,
) -> ModelSet {
    let mut set = ModelSet::default();
    let Some(default) = models.get(&ModelRole::Default) else {
        return set;
    };
    for role in ModelRole::ALL {
        let info = models.get(&role).unwrap_or(default).clone();
        let declared = prompt.frontmatter().role_requirements(role);
        let minimum = declared.and_then(RoleRequirements::min_context);
        let thinking = declared.and_then(RoleRequirements::thinking);
        if let Some(minimum) = minimum
            && info.context() < minimum
        {
            requirements.push_model(UnmetRequirement {
                by: by.map(str::to_owned),
                role,
                check: RequirementCheck::ContextMinimum,
                required: minimum.to_string(),
                actual: info.context().to_string(),
            });
        }
        if let Some((required, actual)) = failed_thinking(thinking, &info) {
            requirements.push_model(UnmetRequirement {
                by: by.map(str::to_owned),
                role,
                check: RequirementCheck::Thinking,
                required: required.to_owned(),
                actual: actual.to_owned(),
            });
        }
        let binding = ModelBinding::new(
            role.as_str(),
            info.description(),
            info.id().clone(),
            ModelInvocation::new().with_thinking(thinking),
            info.context(),
        )
        .with_catalog_info(info);
        set.push(binding);
    }
    set
}

/// The thinking requirement `info`'s thinking mode fails, as what the
/// prompt required and what the model provides, or `None` when the mode
/// meets it: `thinking: true` needs a model that can think, and
/// `thinking: false` needs one that can run without thinking.
fn failed_thinking(
    thinking: Option<bool>,
    info: &ModelInfo,
) -> Option<(&'static str, &'static str)> {
    match thinking {
        Some(true) if info.thinking() == ThinkingMode::Never => Some(("thinking: true", "Never")),
        Some(false) if info.thinking() == ThinkingMode::Always => {
            Some(("thinking: false", "Always"))
        }
        _ => None,
    }
}
