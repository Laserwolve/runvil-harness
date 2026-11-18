---
name: result-and-error
description: Joins a task that returns text and a task that raises
runvil: 0
---

## Main

```lua
local good = tasks.spawn('../Good')
local bad = tasks.spawn('../Bad')
local results = tasks.join({ good, bad })
log('good: ' .. tostring(results[1].ok) .. ' ' .. results[1].result)
log('bad: ' .. tostring(results[2].ok) .. ' ' .. results[2].result.kind .. ' ' .. results[2].result.message)
return 'the owner ran on'
```

## Good

```lua
return 'text'
```

## Bad

```lua
error('the task broke')
```
