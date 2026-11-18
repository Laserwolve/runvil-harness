---
name: own-status
description: A task reads its own status
runvil: 0
---

## Main

```lua
local prober = tasks.spawn('../Prober')
local results = tasks.join({ prober })
return results[1].result
```

## Prober

```lua
local status = tasks.status(sys.taskid)
return status.target .. ' is ' .. status.state
```
