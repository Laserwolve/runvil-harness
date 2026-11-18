---
name: sibling-not-owned
description: A task tries to act on its sibling
runvil: 0
---

## Main

```lua
local sibling = tasks.spawn('../Sibling')
local prober = tasks.spawn('../Prober', { item = sibling.task })
local results = tasks.join({ prober })
tasks.cancel(sibling)
return results[1].result
```

## Sibling

```lua
return tasks.receive()
```

## Prober

```lua
local tries = {
  { 'join', function() return tasks.join({ item }) end },
  { 'ready', function() return tasks.ready(item) end },
  { 'status', function() return tasks.status(item) end },
  { 'send', function() return tasks.send(item, 'hello') end },
  { 'cancel', function() return tasks.cancel(item) end },
}
for _, try in ipairs(tries) do
  local ok, err = pcall(try[2])
  log(try[1] .. ': ' .. tostring(ok) .. ' ' .. err.kind)
end
return 'tried five calls on ' .. item
```
