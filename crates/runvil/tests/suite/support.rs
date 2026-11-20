//! What every suite Host hands a Harness: a Host over the suite's Packs
//! that finds the one prompt a test runs and sleeps on tokio's timer,
//! and that timer.

use std::sync::Arc;
use std::time::Duration;

use runvil::harness::{Packs, Timer};
use runvil::plugin::PackRegistry;
use runvil::{BoxFuture, Error, ErrorCode, Harness, Host, RunSetup};

/// The name every suite Host finds its prompt by.
const PROMPT: &str = "prompt.md";

/// Sleeps on tokio's timer, as a Host on tokio does.
pub(crate) struct Clock;

impl Timer for Clock {
    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(duration))
    }
}

/// A Host's Packs with none but the built-ins.
pub(crate) fn no_packs() -> Packs {
    Packs::new(PackRegistry::new())
}

/// A Harness running `source` with `setup` on a Host over `packs` that
/// finds `source` by its name and sleeps on [`Clock`].
pub(crate) fn launch(packs: Packs, source: &str, setup: RunSetup) -> Harness {
    let source = source.to_owned();
    let host = Arc::new(Host {
        packs,
        prompts: Arc::new(move |name: &str| {
            if name == PROMPT {
                return Ok(source.clone());
            }
            let code = ErrorCode::NotFound;
            Err(Error::new(
                code,
                format!("no prompt named {name}"),
                code.prose(),
            ))
        }),
        timer: Arc::new(Clock),
    });
    let prompt = match host.parse(PROMPT) {
        Ok(prompt) => prompt,
        Err(error) => panic!("the suite's prompt parses: {error:?}"),
    };
    Harness::new(host, prompt, setup)
}
