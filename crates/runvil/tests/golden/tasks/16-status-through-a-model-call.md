---
name: status-through-a-model-call
description: Reads a task's status before, during, and after its model call
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
log('before: ' .. tasks.status(worker).blocked)
tasks.join({ worker }, { timeout = 1 })
log('during: ' .. tasks.status(worker).blocked)
tasks.join({ worker })
local status = tasks.status(worker)
log('after: ' .. status.state .. ', ok ' .. tostring(status.ok))
return 'read three times'
```

## Worker

```lua
return models.infer('after 5 reply done')
```
