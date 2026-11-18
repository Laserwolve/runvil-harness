---
name: spawns-by-hand
description: Spawns and joins three tasks by hand
runvil: 0
---

## Main

```lua
local set = {}
for index, color in ipairs({ 'red', 'green', 'blue' }) do
  set[index] = tasks.spawn('../Worker', { item = color, index = index })
end
local texts = {}
for index, result in ipairs(tasks.join(set)) do
  texts[index] = result.result
end
return table.concat(texts, ', ')
```

## Worker

```lua
return item .. ' ' .. sys.index
```
