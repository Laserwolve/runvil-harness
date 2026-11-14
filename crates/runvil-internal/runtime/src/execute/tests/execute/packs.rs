//! What each prompt's `uses:` and `models:` do in a run of several
//! prompts: a section installs the preludes of the Packs its own prompt
//! declares, `tools.packs` marks that prompt's Packs as required and
//! lists every other Pack of the run as an extra, a role's thinking
//! switch is the running prompt's, and the executed prompt starts on its
//! own `default` model. None of it changes what the caller sees after the
//! `execute` returns.

use std::collections::BTreeMap;

use runvil_types::packs::PackId;

use super::*;
use crate::execute::tests::preludes::prelude;
use crate::lua::RosterEntry;
use crate::model::{ModelRole, ThinkingMode};
use crate::test_support::scenario::test_model_with;

/// Lua that returns which of the globals `x_loaded`, `y_loaded`, and
/// `both_loaded` the section has, each as its value or `nil`.
const SEEN: &str =
    "tostring(x_loaded) .. ',' .. tostring(y_loaded) .. ',' .. tostring(both_loaded)";

#[test]
fn each_prompts_sections_install_the_preludes_of_the_packs_that_prompt_declares() {
    let main = listing("t", &["child.md"])
        .front("uses: [x, both]")
        .section(
            "Main",
            &format!(
                "local before = {SEEN}
                 var.seen = before .. ' | ' .. execute('child.md') .. ' | ' .. {SEEN}"
            ),
        )
        .section("After", &format!("return var.seen .. ' | ' .. {SEEN}"))
        .build();
    let child = PromptSource::new()
        .name("child")
        .front("uses: [y, both]")
        .section("First", &format!("var.first = {SEEN}"))
        .section("Second", &format!("return var.first .. ' ' .. {SEEN}"))
        .build();
    let trace = Scenario::new(main)
        .prompt("child.md", child)
        .params(|ctx| {
            ctx.with_preludes(vec![
                prelude("y", "y_loaded = 'y'"),
                prelude("both", "both_loaded = 'both'"),
                prelude("x", "x_loaded = 'x'"),
            ])
        })
        .run();
    assert_eq!(
        trace.text(),
        "x,nil,both | nil,y,both nil,y,both | x,nil,both | x,nil,both",
        "the caller's sections before and after the execute, and the section it enters next, \
         have its own Packs' globals, and both sections of the executed prompt have that \
         prompt's"
    );
}

/// A run over two Packs, `p` and `q`, whose preludes both define the
/// global `shared`: its prompt declares `main_uses` and lists `child.md`,
/// which declares `child_uses`.
fn sharing_a_global(main_uses: &str, child_uses: &str) -> Trace {
    let main = listing("t", &["child.md"])
        .front(&format!("uses: [{main_uses}]"))
        .section(
            "Main",
            "local ok, err = pcall(execute, 'child.md')
             return shared .. ' | ' .. tostring(ok) .. ' ' .. tostring(err)",
        )
        .build();
    let child = PromptSource::new()
        .name("child")
        .front(&format!("uses: [{child_uses}]"))
        .section("Only", "return shared")
        .build();
    Scenario::new(main)
        .prompt("child.md", child)
        .params(|ctx| {
            ctx.with_preludes(vec![
                prelude("p", "shared = 'from p'"),
                prelude("q", "shared = 'from q'"),
            ])
        })
        .run()
}

#[test]
fn two_packs_that_define_one_global_collide_only_in_a_prompt_that_declares_both() {
    assert_eq!(
        sharing_a_global("p", "q").text(),
        "from p | true from q",
        "each prompt installs one of the two, so neither collides"
    );

    // The executed prompt declares both: its first section's setup fails,
    // and the failure is the one its caller's `execute` raises.
    let trace = sharing_a_global("p", "p, q");
    let text = trace.text();
    assert!(
        text.starts_with("from p | false ")
            && text.contains("Pack `q`: its prelude defines the global `shared`")
            && text.contains("which Pack `p`'s prelude already defines"),
        "the collision is the executed prompt's alone: {text}"
    );

    // The run's own prompt declares both: its first section's setup fails
    // the run before the execute.
    let trace = sharing_a_global("p, q", "p");
    let error = trace.error();
    assert_eq!(error.code, ErrorCode::LuaRuntime, "{error:?}");
    assert!(
        error
            .to_string()
            .contains("Pack `q`: its prelude defines the global `shared`"),
        "{error}"
    );
}

/// Lua that returns the names of the run's Packs in order, each required
/// one marked `!`, then the names `tools.packs.extras()` lists.
const PACKS: &str = "(function()
       local all, extras = {}, {}
       for i = 1, #tools.packs do
         local pack = tools.packs[i]
         all[#all + 1] = pack.name .. (pack.required and '!' or '')
       end
       for _, pack in ipairs(tools.packs.extras()) do extras[#extras + 1] = pack.name end
       return table.concat(all, ',') .. ' extras ' .. table.concat(extras, ',')
     end)()";

#[test]
fn tools_packs_marks_the_running_prompts_own_packs_as_required_and_lists_the_rest_as_extras() {
    let main = listing("t", &["child.md"])
        .front("uses: [x]")
        .section(
            "Main",
            &format!(
                "local before = {PACKS}
                 return before .. ' | ' .. execute('child.md') .. ' | ' .. {PACKS}"
            ),
        )
        .build();
    let child = PromptSource::new()
        .name("child")
        .front("uses: [y, z]")
        .section("Only", &format!("return {PACKS}"))
        .build();
    let usable = |id: &str| {
        (
            PackId::parse(id).expect("a static valid Pack id"),
            RosterEntry::Usable {
                vendor: "acme".to_owned(),
            },
        )
    };
    let trace = Scenario::new(main)
        .prompt("child.md", child)
        .params(move |ctx| ctx.with_packs(["w", "x", "y", "z"].map(usable)))
        .run();
    assert_eq!(
        trace.text(),
        "adhoc,w,x!,y,z extras w,y,z | adhoc,w,x,y!,z! extras w,x | adhoc,w,x!,y,z extras w,y,z",
        "every prompt sees every Pack of the run, and the caller's list is the same before and \
         after the execute"
    );
}

#[test]
fn a_roles_thinking_switch_is_the_running_prompts_and_the_executed_prompt_starts_on_default() {
    let gateway = ScriptedChat::new(vec![text("a1"), text("b1"), text("b2"), text("a2")]);
    let main = listing("t", &["child.md"])
        .front("models: { strong: { thinking: true } }")
        .section(
            "Main",
            "models.use('strong')
             local first = models.infer('a1')
             local child = execute('child.md')
             return first .. ' ' .. child .. ' ' .. models.infer('a2')",
        )
        .build();
    let child = PromptSource::new()
        .name("child")
        .front("models: { strong: { thinking: false } }")
        .section(
            "Only",
            "local on_default = models.infer('b1')
             models.use('strong')
             return on_default .. ' ' .. models.infer('b2')",
        )
        .build();
    let roles = BTreeMap::from([(ModelRole::Default, "base"), (ModelRole::Strong, "big")]);
    let mut scenario = Scenario::new(main).prompt("child.md", child).chat(&gateway);
    for (role, name) in roles {
        scenario = scenario.role_model(
            role,
            test_model_with(name, 131_072, ThinkingMode::Switchable),
        );
    }
    let trace = scenario.run();
    assert_eq!(trace.text(), "a1 b1 b2 a2");
    let rounds: Vec<(String, Option<bool>)> = gateway
        .requests()
        .iter()
        .map(|body| (body.options.model().to_owned(), body.options.thinking()))
        .collect();
    let round = |model: &str, thinking| (model.to_owned(), thinking);
    assert_eq!(
        rounds,
        [
            round("big", Some(true)),
            round("base", None),
            round("big", Some(false)),
            round("big", Some(true)),
        ],
        "the executed prompt starts on its own `default`, its `strong` has its own switch, \
         and its `models.use` leaves its caller's current model alone"
    );
}

#[test]
fn the_executed_prompts_sections_read_its_own_section_count_and_its_callers_task() {
    let main = listing("t", &["child.md"])
        .section(
            "Main",
            "return sys.section_count .. ' ' .. execute('child.md') .. ' ' .. sys.section_count",
        )
        .section("Other", "return 'unreached'")
        .build();
    let child = PromptSource::new()
        .name("child")
        .preamble("var.count = sys.section_count")
        .section("One", "var.id = sys.id")
        .section("Two", "var.task = sys.taskid")
        .section(
            "Three",
            "return var.count .. ' ' .. var.id .. ' ' .. var.task .. ' ' .. sys.section_name",
        )
        .build();
    let trace = Scenario::new(main).prompt("child.md", child).run();
    assert_eq!(
        trace.text(),
        "2 4 0.0.1 0 Three 2",
        "the executed prompt counts its own top-level sections, its entries are numbered \
         under its own chain, and it runs in its caller's task"
    );
}
