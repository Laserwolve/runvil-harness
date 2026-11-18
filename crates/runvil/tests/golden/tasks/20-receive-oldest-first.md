---
name: receive-oldest-first
description: Sends a task two messages before it runs and a third while it waits
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
tasks.send(worker, 'one')
tasks.send(worker, 'two')
tasks.join({ worker }, { timeout = 1 })
log('the worker waits on: ' .. tasks.status(worker).blocked)
tasks.send(worker, 'three')
local results = tasks.join({ worker })
return results[1].result
```

## Worker

```lua
local first = tasks.receive()
local second = tasks.receive()
local third = tasks.receive()
return first .. ' ' .. second .. ' ' .. third
```
