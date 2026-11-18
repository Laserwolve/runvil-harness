---
name: wait-twice
description: Waits twice on one task
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
local first = tasks.join({ worker })
local ok, err = pcall(tasks.join, { worker })
log('second wait: ' .. tostring(ok) .. ' ' .. err.kind)
return first[1].result
```

## Worker

```lua
return 'done'
```
