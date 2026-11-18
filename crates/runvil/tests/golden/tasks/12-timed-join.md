---
name: timed-join
description: Time-limits a join on a task that waits on the model
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
local results, timed_out = tasks.join({ worker }, { timeout = 1 })
log('timed out: ' .. tostring(timed_out) .. ', entries: ' .. #results)
local later = tasks.join({ worker })
return later[1].result
```

## Worker

```lua
return models.infer('after 5 reply done')
```
