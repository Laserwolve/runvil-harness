# runvil-runtime

- Only the executor decides store access. It makes every store of a run, the one the run starts with and one for each prompt an `execute` runs, and it derives every store view from the calling chain's store under the chain's access, at dispatch for a `Vfs` effect. The caller uses the view it was given and never derives, widens, or keeps store scope from it.
