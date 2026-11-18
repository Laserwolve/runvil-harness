---
name: fanout
description: Fans out over three members
runvil: 0
---

## Main

```lua
local results = tasks.fanout('../Worker', { 'red', 'green', 'blue' })
local texts = {}
for index, result in ipairs(results) do
  texts[index] = result.text
end
return table.concat(texts, ', ')
```

## Worker

```lua
return item .. ' ' .. sys.index
```
