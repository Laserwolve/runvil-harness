---
name: spawn-and-join
description: Spawns one task and joins it
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
local results = tasks.join({ worker })
return results[1].result
```

## Worker

```lua
return 'done'
```
