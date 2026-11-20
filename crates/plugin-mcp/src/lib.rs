//! An MCP client Pack: one Pack over every MCP server its configuration
//! names, each reached over Streamable HTTP, whose tools a prompt lists
//! and calls through two static tools.
//!
//! The crate's Plugin, [`install`], inserts the Pack `runvil/mcp`, whose
//! Pack id is `mcp`. A Host calls it with two plain arguments beside its
//! registry. The first is the server list: the `mcpServers` object of an
//! `mcp.json` file, which holds each server's entry under the server's
//! name. A `null` server list means no servers, and any other value that
//! is not an object leaves the Pack unavailable. The second is the handle
//! of the Host's tokio runtime. The Pack has no prelude, so declaring it,
//! as in `uses: [mcp]`, only makes it required.
//!
//! This Pack is compiled into the Host and is not a library Plugin. It
//! uses parts of the Rust Pack API that the C contract has no form for:
//! the caller of a call, the `describes_tools` flag, and a configuration
//! at `install`. Its servers' tools are meant to join a section's
//! Dynamic Set through `Pack::render`, which the contract has no verb
//! for either.
//!
//! The Pack offers exactly two tools under the Pack id its factory
//! receives, such as `mcp/get_dynamic_tools` under `mcp`, modeled on
//! Cursor's `GetDynamicTools` and `CallDynamicTool`:
//!
//! - `get_dynamic_tools` takes the optional strings `namespace` and
//!   `toolName` and answers JSON text. It describes tools, so a script's
//!   call receives the text as the servers wrote it and reads it with
//!   `json.decode`, while a model's call receives it in the untrusted
//!   envelope. With no arguments it lists each server's namespace,
//!   status, and tool names, in name order; with `namespace`, that
//!   server's tools with their input schemas; with both, one tool.
//! - `call_dynamic_tool` takes the strings `namespace` and `toolName` and
//!   an optional object `arguments`, which defaults to `{}`, and calls that
//!   server's tool.
//!
//! Both tools answer a model round with untrusted text, and the section's
//! own script with trusted text, so the script receives the server's text
//! without the envelope.
//!
//! A namespace is the server's name exactly as the configuration writes
//! it, and a tool's name is the one its server lists. Both must match
//! exactly. A server's status is `ready`, `loading` while it is still
//! starting, or `error` with the reason. A tool's description is its
//! `description`, else its `title`, else its name. An unknown namespace
//! or tool, a `toolName` without a `namespace`, and a call to a server
//! that is not ready each give an error a model can read.
//!
//! Each entry is parsed on its own. A remote entry has `url` and optional
//! `headers`, and a `type` of `"sse"`, the legacy transport, is refused. A
//! local entry has `command`, with optional `args`, `env`, and `cwd`. This
//! Pack recognizes that shape and refuses it with `local MCP servers
//! (command) are not supported yet`. Other `type` values and unknown
//! fields are ignored, because the format is defined outside this
//! repository. A refused entry is a namespace with status `error` and the
//! reason, and the other servers still connect, so one bad entry never
//! leaves the Pack unavailable.
//!
//! String values in an entry may use `${env:NAME}`, which an unset
//! variable refuses naming `NAME`, and `${userHome}`. Any other `${...}`
//! form passes through unchanged. `headers` may not set `accept`,
//! `Mcp-Session-Id`, or `Last-Event-Id`, which the client owns.
//!
//! Installing the Pack does no I/O. `install` starts one background task
//! per parsed entry on the Host's runtime, and the task connects, runs
//! the handshake, and lists every page of tools within two minutes. A
//! call to a server's tool is spawned on that runtime too. Every instance
//! the Pack's factory makes shares those servers, so every run sees the
//! same server states. The Pack's `ready` resolves at once, so a run can
//! see the two tools while servers are still connecting, and the listing
//! shows each such server as `loading`. A server that fails to start
//! shows as `error` with the reason, and its task keeps trying in the
//! background, waiting 1 second after the first failure and doubling up
//! to a minute, so the server lists its tools once it answers. A server
//! that is ready stays ready, and a later failure shows as a transport
//! error on the call.
//!
//! A call sends `tools/call` with the tool's name and waits up to five
//! minutes. Text blocks and embedded text resources come back joined
//! with blank lines. An image, audio, or binary resource becomes a
//! placeholder such as `[image omitted: image/png]`, a resource link
//! becomes `[resource: <uri>]`, and a result with no blocks but with
//! `structuredContent` gives that JSON. A result marked `isError` is a
//! backend error with the server's text, and a transport failure or the
//! deadline is a transport error. Dropping a call sends the server
//! `notifications/cancelled` for its request.
//!
//! The client declares no `capabilities`, because an optional extension
//! would make GitHub's remote server answer with interactive forms this
//! Pack cannot show. Its HTTP client refuses redirects, which keeps the
//! configured headers off any other server.
//!
//! ## Invariants
//!
//! - May depend on: `runvil-plugin` and outside libraries.
//!   `cargo test -p build-xtask` enforces the Plugin family's
//!   allow-list. Only [`install`] is public.
//! - Output to a model round is untrusted, because descriptions, schemas,
//!   and results all come from the servers, and output to the section's
//!   own script is trusted, so the script receives the server's text
//!   without the envelope.
//! - No reason, log line, `Debug` output, or error names an `env` or
//!   `headers` value. They may name keys.
//! - It runs on the Host's runtime, whose handle `install` takes as an
//!   argument, and `install` does no I/O.
//! - It starts no process: each local entry is refused at `install`, on
//!   its own.
//! - Dropping a call tells the server to drop the request.

mod connect;
mod dynamic;
mod entry;
mod result;
mod server;

use std::collections::BTreeMap;
use std::sync::Arc;

use runvil_plugin::{
    Error, ErrorCode, Pack, PackFactory, PackId, PackInfo, PackRegistry, make_tool_error,
};
use serde_json::{Map, Value};
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::dynamic::{Mcp, Namespace};
use crate::entry::Vars;
use crate::server::{Server, State};

/// The Pack's `PackInfo`.
///
/// Its FQN is `runvil/mcp`, so its Pack id is `mcp`. It has no prelude.
const MCP: PackInfo =
    PackInfo::new("runvil/mcp").description("Tools from every configured MCP server");

/// The Plugin: inserts the `mcp` Pack into `packs`. `servers` is the
/// `mcpServers` object of an `mcp.json` file, which holds each server's
/// entry under the server's name, and `null` means no servers. `runtime`
/// is the handle of the Host's tokio runtime. `install` starts one
/// connection task per good server entry on `runtime`, into servers
/// every instance shares. A `servers` that is not `null` or an object
/// inserts the Pack unavailable, naming the problem. A bad entry is
/// refused alone, as a namespace with status `error`.
///
/// # Errors
/// Returns the registry's refusal, such as an
/// [`InvalidValue`](runvil_plugin::ErrorCode::InvalidValue) error with the
/// reason `duplicate_pack` when `packs` holds `runvil/mcp` already.
pub fn install(packs: &mut PackRegistry, servers: Value, runtime: Handle) -> Result<(), Error> {
    match Servers::start(servers, runtime) {
        Ok(servers) => packs.insert(MCP, servers),
        Err(reason) => packs.insert_unavailable(MCP, reason),
    }
}

/// The Pack's factory: every configured server under its name, which
/// every instance shares, and the Host's runtime its calls are spawned
/// on. The servers' connection tasks stop when the last of them drops.
struct Servers {
    namespaces: Arc<BTreeMap<String, Namespace>>,
    runtime: Handle,
}

impl Servers {
    /// Parses each server's entry in `servers` on its own, and starts
    /// one connection task per parsed entry on `runtime`. It does no I/O
    /// itself.
    fn start(servers: Value, runtime: Handle) -> Result<Servers, Error> {
        let entries = match servers {
            Value::Null => Map::new(),
            Value::Object(entries) => entries,
            _ => {
                return Err(make_tool_error(
                    ErrorCode::ToolOther,
                    "the MCP configuration must be a JSON object of servers by name, as the \
                     `mcpServers` object of an mcp.json file is",
                ));
            }
        };
        let env = |key: &str| std::env::var(key).ok();
        let home = std::env::home_dir().map(|dir| dir.to_string_lossy().into_owned());
        let vars = Vars { env: &env, home };
        let mut namespaces = BTreeMap::new();
        for (server, config) in entries {
            let namespace = match entry::parse(&config, &vars) {
                Ok(entry) => {
                    let (state, watching) = watch::channel(State::Starting);
                    let connection = runtime.spawn(connect::run(
                        server.clone(),
                        entry,
                        connect::STARTUP_DEADLINE,
                        state,
                    ));
                    Namespace::Server(Server::new(watching, connection))
                }
                Err(refused) => {
                    tracing::warn!(%server, reason = %refused, "the MCP server's entry was refused");
                    Namespace::Refused(refused.to_string())
                }
            };
            namespaces.insert(server, namespace);
        }
        Ok(Servers {
            namespaces: Arc::new(namespaces),
            runtime,
        })
    }
}

impl PackFactory for Servers {
    /// Makes one MCP Pack instance named `id` over the shared servers.
    fn new_pack(&self, id: &PackId) -> Result<Arc<dyn Pack>, Error> {
        let mcp = Mcp::new(
            id,
            Arc::clone(&self.namespaces),
            Handle::clone(&self.runtime),
        )?;
        Ok(Arc::new(mcp))
    }
}
