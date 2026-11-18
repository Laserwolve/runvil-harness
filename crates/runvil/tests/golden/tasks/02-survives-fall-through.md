---
name: survives-fall-through
description: Joins a task in the section after the one that spawned it
runvil: 0
---

## First

```lua
var.worker = tasks.spawn('../Worker')
local _, timed_out = tasks.join({ var.worker }, { timeout = 1 })
log('timed out in First: ' .. tostring(timed_out))
```

## Second

```lua
local results = tasks.join({ var.worker })
return results[1].result
```

## Worker

```lua
return models.infer('after 5 reply done')
```
