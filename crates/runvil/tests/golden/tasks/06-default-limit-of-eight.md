---
name: default-limit-of-eight
description: Spawns nine tasks under the default limit
runvil: 0
---

## Main

```lua
local set = {}
for index = 1, 9 do
  set[index] = tasks.spawn('../Worker', { index = index })
end
local results = tasks.join(set)
return #results .. ' joined'
```

## Worker

```lua
return models.infer('after ' .. sys.index .. ' reply ' .. sys.index)
```
