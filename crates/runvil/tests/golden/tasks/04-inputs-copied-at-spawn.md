---
name: inputs-copied-at-spawn
description: Changes var and the model after a spawn
runvil: 0
---

## Main

```lua
var.color = 'red'
local worker = tasks.spawn('../Worker', { args = { name = 'ada' }, item = 'apple', index = 7 })
var.color = 'blue'
models.use('strong')
local results = tasks.join({ worker })
return results[1].result .. ' | owner: ' .. var.color .. ' ' .. models.current().label
```

## Worker

```lua
return args.name .. ' ' .. item .. ' ' .. sys.index .. ' ' .. var.color .. ' ' .. models.current().label
```
