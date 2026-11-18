---
name: self-and-unknown-not-owned
description: A task tries to cancel itself and a made-up id
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
for _, id in ipairs({ sys.taskid, '9.9' }) do
  local ok, err = pcall(tasks.cancel, id)
  log(id .. ': ' .. tostring(ok) .. ' ' .. err.kind)
end
return 'tried two ids'
```
