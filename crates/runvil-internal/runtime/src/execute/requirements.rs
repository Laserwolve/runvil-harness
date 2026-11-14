//! What a run's params lack for its prompts: [`Requirements`], which the
//! run's first step reports as its refusal notice.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use runvil_types::error::place;
use runvil_types::models::{ModelInfo, ModelRole};
use runvil_types::packs::PackId;
use runvil_types::tools::ToolCatalog;

use crate::lua::RosterEntry;
use crate::prompt::Prompt;
use crate::{Error, error};

#[cfg(test)]
#[path = "requirements-tests.rs"]
mod tests;

/// The first line of every refusal notice.
pub(super) const NOTICE_HEADER: &str = "the environment cannot satisfy this prompt:";

/// What the run's params lack for the run's prompts, its own and every
/// prompt that one links, gathered when the run is built. The run's first
/// step fails with its notice unless it is empty.
///
/// The gaps of the run's own prompt are gathered first, then those of each
/// linked prompt in the order of the linked prompts' names, so each list
/// here is in that order.
#[derive(Debug, Default)]
pub(super) struct Requirements {
    /// The model requirements that the models filling the roles fall short
    /// of, and the `default` role a non-empty role map leaves unfilled.
    unmet_models: Vec<UnmetRequirement>,
    /// The declared Packs the roster lacks or lists as unable to serve,
    /// each prompt's in declaration order. A Pack two prompts declare is
    /// here once, for the first of them.
    unmet_packs: Vec<UnmetPack>,
    /// The reserved Pack ids, such as `adhoc`, that the run's tool catalog
    /// or Pack roster names. The Engine holds the tools under a reserved
    /// Pack id itself, so no Pack may take one.
    reserved: BTreeSet<PackId>,
}

impl Requirements {
    /// Checks what the params give every prompt of the run alike: records
    /// each reserved Pack id that a tool of `catalog` or a Pack of
    /// `roster`, usable or not, has, and the `default` role when `models`
    /// holds entries but none for it.
    pub(super) fn check_run(
        &mut self,
        models: &BTreeMap<ModelRole, ModelInfo>,
        catalog: &ToolCatalog,
        roster: Option<&BTreeMap<PackId, RosterEntry>>,
    ) {
        self.reserved = catalog
            .tools()
            .iter()
            .map(|tool| tool.id.pack())
            .chain(roster.into_iter().flat_map(BTreeMap::keys).cloned())
            .filter(PackId::is_reserved)
            .collect();
        if !models.is_empty() && !models.contains_key(&ModelRole::Default) {
            self.unmet_models.push(UnmetRequirement {
                by: None,
                role: ModelRole::Default,
                check: RequirementCheck::Unbound,
                required: "a model".to_owned(),
                actual: "none".to_owned(),
            });
        }
    }

    /// Checks each Pack `prompt` declares against `roster`, when the run
    /// has one. `by` is the name of `prompt` when it is a linked prompt,
    /// and `None` for the run's own. A Pack that an earlier prompt's check
    /// already recorded is not recorded again.
    pub(super) fn check_packs(
        &mut self,
        prompt: &Prompt,
        by: Option<&str>,
        roster: Option<&BTreeMap<PackId, RosterEntry>>,
    ) {
        let Some(roster) = roster else {
            return;
        };
        for entry in prompt.frontmatter().uses() {
            let id = entry.id();
            let reason = match roster.get(id) {
                None => None,
                Some(RosterEntry::Unavailable { reason }) => Some(reason.clone()),
                Some(RosterEntry::Usable { .. }) => continue,
            };
            if self.unmet_packs.iter().all(|unmet| unmet.id != *id) {
                self.unmet_packs.push(UnmetPack {
                    by: by.map(str::to_owned),
                    id: id.clone(),
                    reason,
                });
            }
        }
    }

    /// Records a model requirement that the model filling a role falls
    /// short of.
    pub(super) fn push_model(&mut self, unmet: UnmetRequirement) {
        self.unmet_models.push(unmet);
    }

    /// Returns the `RequirementsUnmet` failure the run's first step ends
    /// with, whose message and prose are the refusal notice, or `None` when
    /// nothing blocks the run. `prompt` is the run's own prompt.
    ///
    /// The failure is placed in the first prompt that caused a gap, the
    /// run's own before the prompts it links and those in the order of
    /// their names, on the frontmatter key of that prompt's first gap in
    /// the notice: `uses:` for a missing or unavailable Pack, and `models:`
    /// for a model that falls short of a role's requirement. A reserved
    /// Pack id and a role map with no `default` entry are the caller's gaps
    /// and point at no key, so a refusal made only of those has no
    /// location.
    pub(super) fn refusal(&self, prompt: &Prompt) -> Option<Error> {
        let notice = self.notice()?;
        let error = error::make_requirements_unmet_error(&notice);
        let own = std::iter::once((prompt, None));
        let linked = prompt
            .linked()
            .map(|linked| (&**linked, Some(linked.file())));
        let placed = own.chain(linked).find_map(|(prompt, by)| {
            let key = self.gap_key(by)?;
            Some((prompt, crate::prompt::detail::key_line(prompt, key)))
        });
        Some(match placed {
            Some((prompt, line)) => place(error, prompt.file(), line),
            None => error,
        })
    }

    /// The frontmatter key of the first gap in the notice that the prompt
    /// `by` names caused, or `None` when it caused none.
    fn gap_key(&self, by: Option<&str>) -> Option<&'static str> {
        if self
            .unmet_packs
            .iter()
            .any(|unmet| unmet.by.as_deref() == by)
        {
            return Some("uses");
        }
        self.unmet_models
            .iter()
            .any(|unmet| {
                unmet.by.as_deref() == by
                    && matches!(
                        unmet.check,
                        RequirementCheck::ContextMinimum | RequirementCheck::Thinking
                    )
            })
            .then_some("models")
    }

    /// Returns the refusal notice, which explains what blocks the run, or
    /// `None` when nothing does.
    ///
    /// Each line names what is missing or falls short. The model lines
    /// carry required versus actual; the Pack lines name the Pack and, for
    /// an unavailable Pack, the reason. The lines come by kind: the
    /// missing Packs, the unavailable Packs, the reserved Pack ids, then
    /// the models. A line about a linked prompt's gap starts with that
    /// prompt's name, as in `- review.md: missing required Pack: web`. The
    /// notice is written for a model to read: concise, factual, and
    /// self-contained.
    fn notice(&self) -> Option<String> {
        // Writing to a String is infallible, so each `write!` result is
        // discarded under the denied `unwrap_used`/`expect_used` lints.
        use std::fmt::Write as _;
        if self.unmet_models.is_empty() && self.unmet_packs.is_empty() && self.reserved.is_empty() {
            return None;
        }
        let mut notice = String::from(NOTICE_HEADER);
        for UnmetPack { by, id, reason } in &self.unmet_packs {
            if reason.is_none() {
                let _ = write!(notice, "\n- {}missing required Pack: {id}", Named(by));
            }
        }
        for UnmetPack { by, id, reason } in &self.unmet_packs {
            if let Some(reason) = reason {
                let _ = write!(notice, "\n- {}{id} is unavailable: {reason}", Named(by));
            }
        }
        for id in &self.reserved {
            let _ = write!(
                notice,
                "\n- the run's tool catalog or Pack roster names the Pack id {id}, \
                 which the Engine reserves"
            );
        }
        for unmet in &self.unmet_models {
            let role = unmet.role.as_str();
            let named = Named(&unmet.by);
            let _ = match unmet.check {
                RequirementCheck::ContextMinimum => write!(
                    notice,
                    "\n- {named}role '{role}': requires a context of at least {} tokens; \
                     the model filling it provides {}",
                    unmet.required, unmet.actual
                ),
                RequirementCheck::Thinking => write!(
                    notice,
                    "\n- {named}role '{role}': requires '{}'; \
                     the thinking capability of the model filling it is {}",
                    unmet.required, unmet.actual
                ),
                RequirementCheck::Unbound => write!(
                    notice,
                    "\n- role '{role}': requires {}; \
                     the caller's role map has no '{role}' entry",
                    unmet.required
                ),
            };
        }
        Some(notice)
    }
}

/// The start of a notice line about a gap of one prompt: the name of a
/// linked prompt and `: `, or nothing for the run's own prompt.
struct Named<'a>(&'a Option<String>);

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(linked) => write!(f, "{linked}: "),
            None => Ok(()),
        }
    }
}

/// A declared Pack the roster cannot give the run.
#[derive(Debug)]
struct UnmetPack {
    /// The name of the linked prompt that declares the Pack, or `None`
    /// when the run's own prompt declares it.
    by: Option<String>,
    /// The Pack's id.
    id: PackId,
    /// Why the roster lists the Pack as unable to serve, or `None` for a
    /// Pack the roster lacks.
    reason: Option<String>,
}

/// A model requirement that the model filling a role falls short of, or a
/// role the caller left without a model: the role, the check that failed,
/// and what the prompt required versus what the caller provided.
#[derive(Debug)]
pub(super) struct UnmetRequirement {
    /// The name of the linked prompt that declares the requirement, or
    /// `None` when the run's own prompt declares it or no prompt does.
    pub(super) by: Option<String>,
    /// The role whose requirement failed.
    pub(super) role: ModelRole,
    /// Which requirement check failed.
    pub(super) check: RequirementCheck,
    /// What the prompt required, such as a context minimum of `200000`, a
    /// thinking switch of `thinking: true`, or `a model`.
    pub(super) required: String,
    /// What the filling model provides, such as a context of `32000` or a
    /// thinking mode of `Never`, or `none` when the caller gave the role no
    /// model.
    pub(super) actual: String,
}

/// Which model requirement check failed.
#[derive(Debug, Clone, Copy)]
pub(super) enum RequirementCheck {
    /// The role's context minimum exceeds the context of the model that
    /// fills the role.
    ContextMinimum,
    /// The role's thinking requirement, `thinking: true` or
    /// `thinking: false`, is one the filling model's info fails to satisfy.
    Thinking,
    /// The caller's role map holds entries but none for the `default`
    /// role, which every role without its own entry falls back to.
    Unbound,
}
