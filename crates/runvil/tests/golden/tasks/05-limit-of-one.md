---
name: limit-of-one
description: Spawns two tasks under a limit of one
runvil: 0
---

## Main

```lua
tasks.concurrency(1)
local first = tasks.spawn('../Worker', { item = 'first' })
local second = tasks.spawn('../Worker', { item = 'second' })
local results = tasks.join({ first, second })
return results[1].result .. ' ' .. results[2].result
```

## Worker

```lua
return models.infer('after 1 reply ' .. item)
```
