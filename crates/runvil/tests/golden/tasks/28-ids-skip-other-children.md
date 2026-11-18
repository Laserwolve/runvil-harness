---
name: ids-skip-other-children
description: Spawns tasks after a call, an execute, and a timed wait
runvil: 0
prompts: [greets.md]
---

## Main

```lua
local called = call('../Called')
local executed = execute('greets.md', 'Ada')
local first = tasks.spawn('../Worker')
tasks.join({ first }, { timeout = 1 })
local second = tasks.spawn('../Worker')
tasks.join({ second })
return called .. ', ' .. executed .. ', ' .. first.task .. ', ' .. second.task
```

## Called

```lua
return 'called'
```

## Worker

```lua
return 'done'
```
