---
name: join-any-first-to-end
description: Waits for the first of two tasks to end
runvil: 0
---

## Main

```lua
local slow = tasks.spawn('../Slow')
local quick = tasks.spawn('../Quick')
local task, ok, result = tasks.join_any({ slow, quick })
log(task.task .. ' ' .. tostring(ok) .. ' ' .. result)
local rest = tasks.join({ slow })
return result .. ' then ' .. rest[1].result
```

## Slow

```lua
return models.infer('after 5 reply slow')
```

## Quick

```lua
return 'quick'
```
