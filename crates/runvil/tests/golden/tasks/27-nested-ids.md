---
name: nested-ids
description: A task spawns a task of its own
runvil: 0
---

## Main

```lua
local outer = tasks.spawn('../Outer')
local results = tasks.join({ outer })
return results[1].result
```

## Outer

```lua
local inner = tasks.spawn('../Inner')
local results = tasks.join({ inner })
return sys.taskid .. ' owns ' .. results[1].result
```

## Inner

```lua
return sys.taskid
```
