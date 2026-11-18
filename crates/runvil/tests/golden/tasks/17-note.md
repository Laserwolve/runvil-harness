---
name: note
description: Reads the latest progress note a task set
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
tasks.join({ worker }, { timeout = 1 })
log('note: ' .. tasks.status(worker).note)
tasks.join({ worker })
return 'read the note'
```

## Worker

```lua
tasks.note('first note')
tasks.note('second note')
return models.infer('after 5 reply done')
```
