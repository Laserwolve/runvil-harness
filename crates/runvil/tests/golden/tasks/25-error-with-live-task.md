---
name: error-with-live-task
description: Fails with its own error while a task still runs
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Waits')
tasks.join({ worker }, { timeout = 1 })
tasks.cancel('9.9')
```

## Waits

```lua
return models.infer('after 5 reply done')
```
