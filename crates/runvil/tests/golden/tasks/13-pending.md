---
name: pending
description: Lists the tasks still live after one is joined
runvil: 0
---

## Main

```lua
local first = tasks.spawn('../Slow')
local second = tasks.spawn('../Quick')
local third = tasks.spawn('../Slow')
tasks.join({ second })
local live = {}
for index, task in ipairs(tasks.pending()) do
  live[index] = task.task
end
log('pending: ' .. table.concat(live, ' '))
tasks.join({ first, third })
return #tasks.pending() .. ' pending at the end'
```

## Slow

```lua
return models.infer('after 5 reply slow')
```

## Quick

```lua
return 'quick'
```
