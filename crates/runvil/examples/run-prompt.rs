//! A Host runs one prompt through a Harness: it builds a Host over a
//! fresh Pack registry, which holds only the built-ins, a finder that
//! serves its one prompt, and a tokio timer, parses the prompt by its
//! name, hands the run a recorder, the stock offline broker, and the
//! operator's name, and prints the greeting the prompt returns.

use std::sync::Arc;
use std::time::Duration;

use runvil::harness::{FindTextPrompt, MemoryRecorder, OfflineBroker, Packs, Timer};
use runvil::plugin::PackRegistry;
use runvil::{BoxFuture, Harness, Host, RunSetup};

/// The name the Host finds the greeting prompt by.
const GREET_NAME: &str = "greet.md";

/// Returns a greeting for the name its `text` argument holds.
const GREET: &str = concat!(
    "---\nname: greet\ndescription: Greets the operator by name\nrunvil: 0\n",
    "---\n\n## Answer\n\n```lua\n",
    "return 'Hello, ' .. args.text .. '.'\n",
    "```\n",
);

/// Sleeps on tokio's timer.
struct Clock;

impl Timer for Clock {
    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(duration))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Build the Host once: its installed Packs, how it finds prompts,
    //    and its timer. A stock finder serves the one prompt under its
    //    name and finds no other.
    let host = Arc::new(Host {
        packs: Packs::new(PackRegistry::new()),
        prompts: Arc::new(FindTextPrompt::new(GREET_NAME, GREET)),
        timer: Arc::new(Clock),
    });

    // 2. Find the prompt by its name and parse it.
    let prompt = host.parse(GREET_NAME)?;

    // 3. Give this run its recorder, its broker, and the operator's name.
    //    The prompt asks no model, so the broker is the stock offline
    //    one, which lists no model. Every other field keeps its default,
    //    such as an empty memory filesystem for the run's tool calls.
    let setup = RunSetup::new(Arc::new(MemoryRecorder::new()), Arc::new(OfflineBroker))
        .args(r#"{"text":"desk"}"#);

    // 4. Run it to its end; the Harness is spent. The first `?` is a
    //    recorder's refusal, and the second is how the run ended. A Host
    //    that wants the run's id or its unread messages keeps the report.
    let greeting = Harness::new(host, prompt, setup).run().await?.result?;
    assert_eq!(greeting, "Hello, desk.");
    println!("{greeting}");
    Ok(())
}
