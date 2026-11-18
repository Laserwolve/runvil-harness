---
name: same-log-twice
description: Two tasks, a timed join, and a delayed model reply
runvil: 0
---

## Main

```lua
local slow = tasks.spawn('../Slow')
local quick = tasks.spawn('../Quick')
local first, timed_out = tasks.join({ slow, quick }, { timeout = 1 })
local second = tasks.join({ slow })
return tostring(timed_out) .. ' ' .. first[2].result .. ' ' .. second[1].result
```

## Slow

```lua
return models.infer('after 5 reply slow')
```

## Quick

```lua
return 'quick'
```
