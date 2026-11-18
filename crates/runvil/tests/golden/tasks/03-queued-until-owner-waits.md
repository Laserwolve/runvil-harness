---
name: queued-until-owner-waits
description: Reads a task's status in a loop that never waits
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
for _ = 1, 3 do
  log(tasks.status(worker).blocked)
end
local results = tasks.join({ worker })
return results[1].result
```

## Worker

```lua
return 'done'
```
