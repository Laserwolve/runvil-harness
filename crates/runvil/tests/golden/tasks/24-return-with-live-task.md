---
name: return-with-live-task
description: Returns while a task that owns a task still runs
runvil: 0
---

## Main

```lua
local owner = tasks.spawn('../Owns')
tasks.join({ owner }, { timeout = 1 })
return 'returned early'
```

## Owns

```lua
local inner = tasks.spawn('../Waits')
local results = tasks.join({ inner })
return results[1].result
```

## Waits

```lua
return models.infer('after 5 reply done')
```
