---
name: ready-after-end
description: Reads tasks.ready before and after a task ends
runvil: 0
---

## Main

```lua
local quick = tasks.spawn('../Quick')
local slow = tasks.spawn('../Slow')
log('before: ' .. tostring(tasks.ready(quick)))
tasks.join({ slow })
log('after: ' .. tostring(tasks.ready(quick)))
local results = tasks.join({ quick })
return results[1].result
```

## Quick

```lua
return 'quick'
```

## Slow

```lua
return models.infer('after 5 reply slow')
```
