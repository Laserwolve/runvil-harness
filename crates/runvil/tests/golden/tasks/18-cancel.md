---
name: cancel
description: Cancels a queued task, a task waiting on the model, and a task that owns a task
runvil: 0
---

## Main

```lua
local queued = tasks.spawn('../Waits')
tasks.cancel(queued)
local waiting = tasks.spawn('../Waits')
local owner = tasks.spawn('../Owns')
tasks.join({ waiting, owner }, { timeout = 1 })
tasks.cancel(waiting)
tasks.cancel(owner)
for _, task in ipairs({ queued, waiting, owner }) do
  local _, ok, err = tasks.join_any({ task })
  log(task.task .. ': ' .. tostring(ok) .. ' ' .. err.kind)
end
return 'canceled three'
```

## Waits

```lua
return models.infer('after 5 reply done')
```

## Owns

```lua
local inner = tasks.spawn('../Waits')
local results = tasks.join({ inner })
return results[1].result
```
